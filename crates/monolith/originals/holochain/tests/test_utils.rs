use anyhow::Result;
use monolith::holochain::conductor::ConductorHandle;
use monolith::holochain_websocket::websocket_connect;
use monolith::holochain_websocket::WebsocketConfig;
use monolith::holochain_websocket::WebsocketReceiver;
use monolith::holochain_websocket::WebsocketSender;
use std::sync::Arc;
use url2::prelude::*;

pub async fn admin_port(conductor: &ConductorHandle) -> u16 {
    conductor
        .get_arbitrary_admin_websocket_port()
        .await
        .expect("No admin port open on conductor")
}

pub async fn websocket_client(
    conductor: &ConductorHandle,
) -> Result<(WebsocketSender, WebsocketReceiver)> {
    let port = admin_port(conductor).await;
    websocket_client_by_port(port).await
}

pub async fn websocket_client_by_port(port: u16) -> Result<(WebsocketSender, WebsocketReceiver)> {
    Ok(websocket_connect(
        url2!("ws://127.0.0.1:{}", port),
        Arc::new(WebsocketConfig::default()),
    )
    .await?)
}
