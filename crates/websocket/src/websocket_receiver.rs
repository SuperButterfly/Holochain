//! defines the read/recv half of a websocket pair

use crate::*;

/// Callback for responding to incoming RPC requests
pub type WebsocketRespond =
    Box<dyn FnOnce(SerializedBytes) -> BoxFuture<'static, Result<()>> + 'static + Send>;

/// You can receive Signals or Requests from the remote side of the websocket.
pub enum WebsocketMessage {
    /// A signal does not require a response.
    Signal(SerializedBytes),

    /// A request that is expecting a response.
    Request(SerializedBytes, WebsocketRespond),
}

/// internal request item
struct RequestItem {
    expires_at: std::time::Instant,
    respond: Option<tokio::sync::oneshot::Sender<Result<Vec<u8>>>>,
}

/// The read half of a websocket connection.
/// Note that due to underlying types this receiver must be awaited
/// for outgoing messages to be sent as well.
pub struct WebsocketReceiver {
    config: Arc<WebsocketConfig>,
    remote_addr: Url2,
    socket: RawSocket,
    send_outgoing: RawSender,
    recv_outgoing: RawReceiver,
    pending_requests: std::collections::HashMap<String, RequestItem>,
}

// unfortunately tokio_tungstenite requires mut self for both send and recv
// so we split the sending out into a channel, and implement Stream such that
// sending and receiving are both handled simultaneously.
impl tokio::stream::Stream for WebsocketReceiver {
    type Item = Result<WebsocketMessage>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context,
    ) -> std::task::Poll<Option<Self::Item>> {
        // first, check to see if we are ready to send any outgoing items
        Self::priv_poll_send(std::pin::Pin::new(&mut self), cx)?;

        // now check if we have any incoming messages
        let p = std::pin::Pin::new(&mut self.socket);
        let result = match futures::stream::Stream::poll_next(p, cx) {
            std::task::Poll::Ready(Some(Ok(i))) => match self.priv_check_incoming(i.into_data()) {
                Ok(Some(output)) => std::task::Poll::Ready(Some(Ok(output))),
                Ok(None) => {
                    // probably received a Response
                    // need to manually trigger a wake, because
                    // this wasn't a real Pending - there might be more data
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
                Err(e) => std::task::Poll::Ready(Some(Err(e))),
            },
            std::task::Poll::Ready(Some(Err(e))) => {
                std::task::Poll::Ready(Some(Err(Error::new(ErrorKind::Other, e))))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        };

        // finally clean up any expired pending requests
        self.priv_prune_pending();

        result
    }
}

impl WebsocketReceiver {
    /// Get the url of the remote end of this websocket.
    pub fn remote_addr(&self) -> &Url2 {
        &self.remote_addr
    }

    /// Get the config associated with this listener.
    pub fn get_config(&self) -> Arc<WebsocketConfig> {
        self.config.clone()
    }

    // -- private -- //

    /// private constructor
    ///  - plucks the remote address
    ///  - generates our sending channel
    pub(crate) fn priv_new(
        config: Arc<WebsocketConfig>,
        socket: RawSocket,
    ) -> Result<(WebsocketSender, Self)> {
        let remote_addr = addr_to_url(socket.get_ref().peer_addr()?, config.scheme);
        let (send_outgoing, recv_outgoing) = tokio::sync::mpsc::channel(10);
        Ok((
            WebsocketSender::priv_new(send_outgoing.clone()),
            Self {
                config,
                remote_addr,
                socket,
                send_outgoing,
                recv_outgoing,
                pending_requests: std::collections::HashMap::new(),
            },
        ))
    }

    /// internal check for sending outgoing messages
    fn priv_poll_send(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context,
    ) -> Result<()> {
        let p = std::pin::Pin::new(&mut self.socket);
        match futures::sink::Sink::poll_ready(p, cx) {
            std::task::Poll::Ready(Ok(_)) => {
                let p = std::pin::Pin::new(&mut self.recv_outgoing);
                match tokio::stream::Stream::poll_next(p, cx) {
                    std::task::Poll::Ready(Some((msg, respond))) => {
                        // prepare the item for send
                        let msg = match self.priv_prep_send(msg, respond) {
                            Ok(msg) => msg,
                            Err(e) => {
                                println!("ERROR");
                                return Err(e);
                            }
                        };

                        println!("got item to send {}", String::from_utf8_lossy(&msg));

                        // send the item
                        let p = std::pin::Pin::new(&mut self.socket);
                        if let Err(e) =
                            futures::sink::Sink::start_send(p, tungstenite::Message::Binary(msg))
                        {
                            println!("ERROR");
                            return Err(Error::new(ErrorKind::Other, e));
                        }

                        // we got Ready on both Sink::poll_ready and
                        // Stream::poll_next if there is more data,
                        // we won't be polled again so we need to
                        // manually trigger the waker.
                        cx.waker().wake_by_ref();
                    }
                    std::task::Poll::Ready(None) => (),
                    std::task::Poll::Pending => (),
                }
            }
            std::task::Poll::Ready(Err(e)) => {
                return Err(Error::new(ErrorKind::Other, e));
            }
            std::task::Poll::Pending => (),
        }
        Ok(())
    }

    /// internal helper for tracking request/response ids
    fn priv_prep_send(
        &mut self,
        msg: Message,
        respond: Option<tokio::sync::oneshot::Sender<Result<Vec<u8>>>>,
    ) -> Result<Vec<u8>> {
        if let Some(id) = msg.clone_id() {
            if respond.is_some() {
                self.pending_requests.insert(
                    id,
                    RequestItem {
                        expires_at: std::time::Instant::now()
                            .checked_add(std::time::Duration::from_millis(
                                self.config.default_request_timeout_ms,
                            ))
                            .expect("can set expires_at"),
                        respond,
                    },
                );
            }
        }
        let bytes: SerializedBytes = msg.try_into()?;
        let bytes: Vec<u8> = UnsafeBytes::from(bytes).into();
        Ok(bytes)
    }

    /// internal helper for processing incoming data
    fn priv_check_incoming(&mut self, msg: Vec<u8>) -> Result<Option<WebsocketMessage>> {
        let bytes: SerializedBytes = UnsafeBytes::from(msg).into();
        let msg: Message = bytes.try_into()?;
        match msg {
            Message::Signal { data } => {
                // we got a signal
                Ok(Some(WebsocketMessage::Signal(
                    UnsafeBytes::from(data).into(),
                )))
            }
            Message::Request { id, data } => {
                println!("RECEIVED REQ: {} {}", id, String::from_utf8_lossy(&data));
                // we got a request
                //  - set up a responder callback
                //  - notify our stream subscriber of the message
                let mut sender = self.send_outgoing.clone();
                let respond: WebsocketRespond = Box::new(|data| {
                    async move {
                        let msg = Message::Response {
                            id,
                            data: UnsafeBytes::from(data).into(),
                        };
                        sender
                            .send((msg, None))
                            .await
                            .map_err(|e| Error::new(ErrorKind::Other, e))?;
                        Ok(())
                    }
                    .boxed()
                });
                Ok(Some(WebsocketMessage::Request(
                    UnsafeBytes::from(data).into(),
                    respond,
                )))
            }
            Message::Response { id, data } => {
                println!("RECEIVED RES: {} {}", id, String::from_utf8_lossy(&data));
                // check our pending table / match up this response
                if let Some(mut item) = self.pending_requests.remove(&id) {
                    if let Some(respond) = item.respond.take() {
                        respond.send(Ok(data)).map_err(|_| {
                            Error::new(
                                ErrorKind::Other,
                                "oneshot channel closed - no one waiting on this response?",
                            )
                        })?;
                    }
                }
                Ok(None)
            }
        }
    }

    /// prune any expired pending responses
    fn priv_prune_pending(&mut self) {
        let now = std::time::Instant::now();
        self.pending_requests.retain(|_k, v| {
            if v.expires_at < now {
                if let Some(respond) = v.respond.take() {
                    let _ = respond.send(Err(ErrorKind::TimedOut.into()));
                }
                false
            } else {
                true
            }
        });
    }
}

/// Establish a new outgoing websocket connection.
pub async fn websocket_connect(
    url: Url2,
    config: Arc<WebsocketConfig>,
) -> Result<(WebsocketSender, WebsocketReceiver)> {
    let addr = url_to_addr(&url, config.scheme).await?;
    let socket = tokio::net::TcpStream::connect(addr).await?;
    let (socket, _) = tokio_tungstenite::client_async_with_config(
        url.as_str(),
        socket,
        Some(tungstenite::protocol::WebSocketConfig {
            max_send_queue: Some(config.max_send_queue),
            max_message_size: Some(config.max_message_size),
            max_frame_size: Some(config.max_frame_size),
        }),
    )
    .await
    .map_err(|e| Error::new(ErrorKind::Other, e))?;
    WebsocketReceiver::priv_new(config, socket)
}
