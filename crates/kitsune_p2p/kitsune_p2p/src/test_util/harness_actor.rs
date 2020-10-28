use super::*;

ghost_actor::ghost_chan! {
    /// The api for the test harness controller
    pub chan HarnessControlApi<KitsuneP2pError> {
        /// Create a new random space id
        /// + join all existing harness agents to it
        /// + all new harness agents will also join it
        fn add_space() -> Arc<KitsuneSpace>;

        /// Create a new agent configured to proxy for others.
        fn add_proxy_agent(nick: String) -> (
            Arc<KitsuneAgent>,
            ghost_actor::GhostSender<KitsuneP2p>,
        );

        /// Create a new directly addressable agent that will
        /// reject any proxy requests.
        fn add_direct_agent(nick: String) -> (
            Arc<KitsuneAgent>,
            ghost_actor::GhostSender<KitsuneP2p>,
        );

        /// Create a new agent that will connect via proxy.
        fn add_nat_agent(nick: String, proxy_url: url2::Url2) -> (
            Arc<KitsuneAgent>,
            ghost_actor::GhostSender<KitsuneP2p>,
        );
    }
}

/// construct a test suite around a mem transport
pub async fn spawn_test_harness_mem() -> Result<
    (
        ghost_actor::GhostSender<HarnessControlApi>,
        HarnessEventChannel,
    ),
    KitsuneP2pError,
> {
    spawn_test_harness(TransportConfig::Mem {}).await
}

/// construct a test suite around a quic transport
pub async fn spawn_test_harness_quic() -> Result<
    (
        ghost_actor::GhostSender<HarnessControlApi>,
        HarnessEventChannel,
    ),
    KitsuneP2pError,
> {
    spawn_test_harness(TransportConfig::Quic {
        bind_to: Some(url2::url2!("kitsune-quic://0.0.0.0:0")),
        override_host: None,
        override_port: None,
    })
    .await
}

/// construct a test suite around a sub transport config concept
pub async fn spawn_test_harness(
    sub_config: TransportConfig,
) -> Result<
    (
        ghost_actor::GhostSender<HarnessControlApi>,
        HarnessEventChannel,
    ),
    KitsuneP2pError,
> {
    init_tracing();

    let harness_chan = HarnessEventChannel::new("");

    let builder = ghost_actor::actor_builder::GhostActorBuilder::new();

    let controller = builder
        .channel_factory()
        .create_channel::<HarnessControlApi>()
        .await?;

    let i_s = builder
        .channel_factory()
        .create_channel::<HarnessInner>()
        .await?;

    tokio::task::spawn(builder.spawn(HarnessActor::new(i_s, harness_chan.clone(), sub_config)));

    Ok((controller, harness_chan))
}

ghost_actor::ghost_chan! {
    /// The api for the test harness controller
    chan HarnessInner<KitsuneP2pError> {
        fn finish_agent(
            agent: Arc<KitsuneAgent>,
            p2p: ghost_actor::GhostSender<KitsuneP2p>,
        ) -> ();
    }
}

struct HarnessActor {
    i_s: ghost_actor::GhostSender<HarnessInner>,
    harness_chan: HarnessEventChannel,
    sub_config: TransportConfig,
    space_list: Vec<Arc<KitsuneSpace>>,
    agents: HashMap<Arc<KitsuneAgent>, ghost_actor::GhostSender<KitsuneP2p>>,
}

impl HarnessActor {
    pub fn new(
        i_s: ghost_actor::GhostSender<HarnessInner>,
        harness_chan: HarnessEventChannel,
        sub_config: TransportConfig,
    ) -> Self {
        Self {
            i_s,
            harness_chan,
            sub_config,
            space_list: Vec::new(),
            agents: HashMap::new(),
        }
    }
}

impl ghost_actor::GhostControlHandler for HarnessActor {}

impl ghost_actor::GhostHandler<HarnessInner> for HarnessActor {}

impl HarnessInnerHandler for HarnessActor {
    fn handle_finish_agent(
        &mut self,
        agent: Arc<KitsuneAgent>,
        p2p: ghost_actor::GhostSender<KitsuneP2p>,
    ) -> HarnessInnerHandlerResult<()> {
        self.agents.insert(agent.clone(), p2p.clone());

        let harness_chan = self.harness_chan.clone();
        let space_list = self.space_list.clone();
        Ok(async move {
            for space in space_list {
                p2p.join(space.clone(), agent.clone()).await?;

                harness_chan.publish(HarnessEventType::Join {
                    agent: (&agent).into(),
                    space: space.into(),
                });
            }
            Ok(())
        }
        .boxed()
        .into())
    }
}

impl ghost_actor::GhostHandler<HarnessControlApi> for HarnessActor {}

impl HarnessControlApiHandler for HarnessActor {
    fn handle_add_space(&mut self) -> HarnessControlApiHandlerResult<Arc<KitsuneSpace>> {
        let space: Arc<KitsuneSpace> = TestVal::test_val();
        self.space_list.push(space.clone());
        let mut all = Vec::new();
        for (agent, p2p) in self.agents.iter() {
            all.push(p2p.join(space.clone(), agent.clone()));
        }
        Ok(async move {
            futures::future::try_join_all(all).await?;
            Ok(space)
        }
        .boxed()
        .into())
    }

    fn handle_add_proxy_agent(
        &mut self,
        nick: String,
    ) -> HarnessControlApiHandlerResult<(Arc<KitsuneAgent>, ghost_actor::GhostSender<KitsuneP2p>)>
    {
        let mut proxy_agent_config = KitsuneP2pConfig::default();
        proxy_agent_config
            .transport_pool
            .push(TransportConfig::Proxy {
                sub_transport: Box::new(self.sub_config.clone()),
                proxy_config: ProxyConfig::LocalProxyServer {
                    proxy_accept_config: Some(ProxyAcceptConfig::AcceptAll),
                },
            });

        let sub_harness = self.harness_chan.sub_clone(nick);
        let i_s = self.i_s.clone();
        Ok(async move {
            let (agent, p2p) = spawn_test_agent(sub_harness, proxy_agent_config).await?;

            i_s.finish_agent(agent.clone(), p2p.clone()).await?;

            Ok((agent, p2p))
        }
        .boxed()
        .into())
    }

    fn handle_add_direct_agent(
        &mut self,
        nick: String,
    ) -> HarnessControlApiHandlerResult<(Arc<KitsuneAgent>, ghost_actor::GhostSender<KitsuneP2p>)>
    {
        let mut direct_agent_config = KitsuneP2pConfig::default();
        direct_agent_config
            .transport_pool
            .push(TransportConfig::Proxy {
                sub_transport: Box::new(self.sub_config.clone()),
                proxy_config: ProxyConfig::LocalProxyServer {
                    proxy_accept_config: Some(ProxyAcceptConfig::RejectAll),
                },
            });

        let sub_harness = self.harness_chan.sub_clone(nick);
        let i_s = self.i_s.clone();
        Ok(async move {
            let (agent, p2p) = spawn_test_agent(sub_harness, direct_agent_config).await?;

            i_s.finish_agent(agent.clone(), p2p.clone()).await?;

            Ok((agent, p2p))
        }
        .boxed()
        .into())
    }

    fn handle_add_nat_agent(
        &mut self,
        nick: String,
        proxy_url: url2::Url2,
    ) -> HarnessControlApiHandlerResult<(Arc<KitsuneAgent>, ghost_actor::GhostSender<KitsuneP2p>)>
    {
        let mut nat_agent_config = KitsuneP2pConfig::default();
        nat_agent_config
            .transport_pool
            .push(TransportConfig::Proxy {
                sub_transport: Box::new(self.sub_config.clone()),
                proxy_config: ProxyConfig::RemoteProxyClient { proxy_url },
            });

        let sub_harness = self.harness_chan.sub_clone(nick);
        let i_s = self.i_s.clone();
        Ok(async move {
            let (agent, p2p) = spawn_test_agent(sub_harness, nat_agent_config).await?;

            i_s.finish_agent(agent.clone(), p2p.clone()).await?;

            Ok((agent, p2p))
        }
        .boxed()
        .into())
    }
}

