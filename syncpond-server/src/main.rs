mod commands;
mod state;

use crate::commands::process_command;
use crate::state::{AppState, SharedState};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let shared_state = Arc::new(RwLock::new(AppState::new()));

    let ws_state = shared_state.clone();
    let command_state = shared_state.clone();

    let ws_server = tokio::spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:8080").await.expect("ws bind");
        info!("syncpond websocket server listening on 127.0.0.1:8080");

        while let Ok((stream, peer)) = listener.accept().await {
            let state = ws_state.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_ws_connection(stream, peer, state).await {
                    error!(%err, peer = %peer, "ws connection error");
                }
            });
        }
    });

    let command_server = tokio::spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:9090").await.expect("cmd bind");
        info!("syncpond command socket listening on 127.0.0.1:9090");

        while let Ok((stream, peer)) = listener.accept().await {
            let state = command_state.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_command_connection(stream, peer, state).await {
                    error!(%err, peer = %peer, "command connection error");
                }
            });
        }
    });

    tokio::try_join!(ws_server, command_server)?;

    Ok(())
}

async fn handle_ws_connection(stream: TcpStream, peer: SocketAddr, state: SharedState) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!(%peer, "websocket connection established");

    {
        let mut app = state.write().await;
        app.total_ws_connections += 1;
        info!(total_ws_connections = app.total_ws_connections, "ws open connections");
    }

    let result = ws_echo_loop(ws_stream, peer).await;

    {
        let mut app = state.write().await;
        app.total_ws_connections = app.total_ws_connections.saturating_sub(1);
        info!(total_ws_connections = app.total_ws_connections, "ws connections after close");
    }

    result
}

async fn ws_echo_loop(mut ws_stream: WebSocketStream<TcpStream>, peer: SocketAddr) -> Result<()> {
    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;

        match msg {
            Message::Text(text) => {
                info!(%peer, %text, "ws text received");
                ws_stream.send(Message::Text(format!("echo: {}", text))).await?;
            }
            Message::Binary(bin) => {
                info!(%peer, bytes = bin.len(), "ws binary received");
                ws_stream.send(Message::Binary(bin)).await?;
            }
            Message::Ping(payload) => {
                ws_stream.send(Message::Pong(payload)).await?;
            }
            Message::Close(frame) => {
                info!(%peer, ?frame, "ws close");
                ws_stream.close(None).await?;
                break;
            }
            _ => {}
        }
    }

    info!(%peer, "ws disconnected");
    Ok(())
}

async fn handle_command_connection(stream: TcpStream, peer: SocketAddr, state: SharedState) -> Result<()> {
    info!(%peer, "command connection established");

    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut line = String::new();

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

        let resp = process_command(trimmed, &state).await;
        writer.write_all(resp.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    info!(%peer, "command disconnected");
    Ok(())
}


