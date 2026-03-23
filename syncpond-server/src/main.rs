mod commands;
mod state;
mod ws;

use crate::commands::{process_command, RoomUpdate};
use crate::state::{AppState, SharedState};
use crate::ws::{handle_ws_connection, WsHub};
use anyhow::Result;
use serde::Deserialize;
use std::{fs, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock},
};
use tracing::{error, info};


#[derive(Debug, Deserialize)]
struct SyncpondConfig {
    command_api_key: String,
    ws_addr: Option<String>,
    command_addr: Option<String>,
    jwt_key: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config_path = std::env::var("SYNCPOND_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());
    let config_text = fs::read_to_string(config_path)?;
    let config: SyncpondConfig = serde_yaml::from_str(&config_text)?;

    let mut base_state = AppState::new();
    base_state.set_command_api_key(config.command_api_key.clone());
    if let Some(jwt) = config.jwt_key.clone() {
        base_state.set_jwt_key(jwt);
    }

    let shared_state = Arc::new(RwLock::new(base_state));
    let ws_hub = Arc::new(Mutex::new(WsHub::new()));

    let ws_addr = config.ws_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let command_addr = config.command_addr.unwrap_or_else(|| "127.0.0.1:9090".to_string());

    let ws_state = shared_state.clone();
    let ws_hub_for_ws = ws_hub.clone();
    let command_state = shared_state.clone();
    let ws_hub_for_cmd = ws_hub.clone();

    let ws_addr_for_task = ws_addr.clone();
    let command_addr_for_task = command_addr.clone();

    let ws_server = tokio::spawn(async move {
        let listener = TcpListener::bind(&ws_addr_for_task).await.expect("ws bind");
        info!("syncpond websocket server listening on {}", ws_addr_for_task);

        while let Ok((stream, peer)) = listener.accept().await {
            let state = ws_state.clone();
            let hub = ws_hub_for_ws.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_ws_connection(stream, peer, state, hub).await {
                    error!(%err, peer = %peer, "ws connection error");
                }
            });
        }
    });

    let command_server = tokio::spawn(async move {
        let listener = TcpListener::bind(&command_addr_for_task).await.expect("cmd bind");
        info!("syncpond command socket listening on {}", command_addr_for_task);

        while let Ok((stream, peer)) = listener.accept().await {
            let state = command_state.clone();
            let hub = ws_hub_for_cmd.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_command_connection(stream, peer, state, hub).await {
                    error!(%err, peer = %peer, "command connection error");
                }
            });
        }
    });

    tokio::try_join!(ws_server, command_server)?;

    Ok(())
}

async fn handle_command_connection(
    stream: TcpStream,
    peer: SocketAddr,
    state: SharedState,
    ws_hub: Arc<Mutex<WsHub>>,
) -> Result<()> {
    info!(%peer, "command connection established");

    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut line = String::new();

    // first message must be API key
    line.clear();
    let first_bytes = reader.read_line(&mut line).await?;
    if first_bytes == 0 {
        return Ok(());
    }
    let provided_key = line.trim();

    let expected_key = {
        let app = state.read().await;
        app.command_api_key.clone()
    };

    if let Some(expected_key) = expected_key {
        if provided_key != expected_key {
            writer.write_all(b"ERROR invalid_api_key\n").await?;
            writer.flush().await?;
            return Ok(());
        }
    } else {
        writer.write_all(b"ERROR api_key_not_configured\n").await?;
        writer.flush().await?;
        return Ok(());
    }

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (resp, updates) = process_command(trimmed, &state).await;
        writer.write_all(resp.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        if !updates.is_empty() {
            let mut hub = ws_hub.lock().await;
            for update in updates {
                hub.broadcast_update(update).await;
            }
        }
    }

    info!(%peer, "command disconnected");
    Ok(())
}


