mod commands;
mod rate_limiter;
mod state;
mod ws;

use crate::commands::process_command;
use crate::rate_limiter::RateLimiter;
use crate::state::{AppState, SharedState};
use crate::ws::{handle_ws_connection, WsHub};
use anyhow::{Context, Result};
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
    health_addr: Option<String>,
    jwt_key: Option<String>,
    jwt_issuer: Option<String>,
    jwt_audience: Option<String>,
    jwt_ttl_seconds: Option<u64>,
    require_tls: Option<bool>,
    health_bind_loopback_only: Option<bool>,
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
    if let Some(issuer) = config.jwt_issuer.clone() {
        base_state.set_jwt_issuer(issuer);
    }
    if let Some(audience) = config.jwt_audience.clone() {
        base_state.set_jwt_audience(audience);
    }
    if let Some(ttl) = config.jwt_ttl_seconds {
        base_state.set_jwt_ttl(ttl);
    }

    let shared_state = Arc::new(RwLock::new(base_state));
    let ws_hub = Arc::new(Mutex::new(WsHub::new()));
    let ws_rate_limiter = Arc::new(RateLimiter::new());
    let command_rate_limiter = Arc::new(RateLimiter::new());

    if config.command_api_key.trim().is_empty() {
        anyhow::bail!("command_api_key must be configured and non-empty");
    }

    let require_tls = config.require_tls.unwrap_or(false);
    if require_tls {
        anyhow::bail!("TLS transport required in config, but this binary does not terminate TLS; use reverse proxy for TLS termination");
    }

    let ws_addr = config.ws_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let command_addr = config.command_addr.unwrap_or_else(|| "127.0.0.1:9090".to_string());
    let health_addr = config.health_addr.unwrap_or_else(|| "127.0.0.1:7070".to_string());
    let health_bind_loopback_only = config.health_bind_loopback_only.unwrap_or(true);

    let ws_addr: SocketAddr = ws_addr
        .parse()
        .with_context(|| format!("invalid ws_addr: {}", ws_addr))?;
    let command_addr: SocketAddr = command_addr
        .parse()
        .with_context(|| format!("invalid command_addr: {}", command_addr))?;
    let health_addr: SocketAddr = health_addr
        .parse()
        .with_context(|| format!("invalid health_addr: {}", health_addr))?;

    if health_bind_loopback_only && !health_addr.ip().is_loopback() {
        anyhow::bail!("health_bind_loopback_only=true but health_addr is not loopback");
    }

    let ws_state = shared_state.clone();
    let ws_hub_for_ws = ws_hub.clone();
    let ws_rate_limiter_for_ws = ws_rate_limiter.clone();
    let command_state = shared_state.clone();
    let ws_hub_for_cmd = ws_hub.clone();
    let command_rate_limiter_for_cmd = command_rate_limiter.clone();

    let ws_addr_for_task = ws_addr.clone();
    let command_addr_for_task = command_addr.clone();

    let ws_server = tokio::spawn(async move {
        let listener = TcpListener::bind(ws_addr_for_task).await.context("ws bind failed")?;
        info!("syncpond websocket server listening on {}", ws_addr_for_task);

        loop {
            let (stream, peer) = listener.accept().await?;
            let state = ws_state.clone();
            let hub = ws_hub_for_ws.clone();
            let rate_limiter = ws_rate_limiter_for_ws.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_ws_connection(stream, peer, state, hub, rate_limiter).await {
                    error!(%err, peer = %peer, "ws connection error");
                }
            });
        }

        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    let command_server = tokio::spawn(async move {
        let listener = TcpListener::bind(command_addr_for_task).await.context("cmd bind failed")?;
        info!("syncpond command socket listening on {}", command_addr_for_task);

        loop {
            let (stream, peer) = listener.accept().await?;
            let state = command_state.clone();
            let hub = ws_hub_for_cmd.clone();
            let rate_limiter = command_rate_limiter_for_cmd.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_command_connection(stream, peer, state, hub, rate_limiter).await {
                    error!(%err, peer = %peer, "command connection error");
                }
            });
        }

        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    let health_state = shared_state.clone();
    let health_addr_for_task = health_addr.clone();
    let health_server = tokio::spawn(async move {
        info!("syncpond health server listening on {}", health_addr_for_task);
        let listener = TcpListener::bind(health_addr_for_task).await.context("health bind failed")?;

        loop {
            let (stream, peer) = listener.accept().await?;
            let state = health_state.clone();
            tokio::spawn(async move {
                if health_bind_loopback_only && !peer.ip().is_loopback() {
                    error!(%peer, "rejected non-loopback health connection");
                    return;
                }

                if let Err(err) = handle_health_connection(stream, state).await {
                    error!(%err, peer = %peer, "health connection error");
                }
            });
        }

        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    let shutdown = async {
        tokio::signal::ctrl_c().await.context("failed to listen for ctrl-c")?;
        info!("shutdown signal received");
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        res = shutdown => res?,
        res = ws_server => res??,
        res = command_server => res??,
        res = health_server => res??,
    }

    info!("server shutdown complete");
    Ok(())
}

const MAX_COMMAND_LINE_LEN: usize = 8192;
const CMD_RATE_LIMIT: usize = 120;
const CMD_RATE_WINDOW_SECS: u64 = 60;

async fn read_line_with_limit<R>(reader: &mut BufReader<R>, line: &mut String) -> Result<usize>
where
    R: tokio::io::AsyncRead + Unpin,
{
    line.clear();
    let bytes = reader.read_line(line).await?;
    if line.len() > MAX_COMMAND_LINE_LEN {
        anyhow::bail!("line_too_long");
    }
    Ok(bytes)
}

async fn handle_health_connection(
    stream: TcpStream,
    state: SharedState,
) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut line = String::new();

    let bytes = read_line_with_limit(&mut reader, &mut line).await?;
    if bytes == 0 {
        return Ok(());
    }

    let parts: Vec<&str> = line.trim_end().split_whitespace().collect();
    let (status, body) = if parts.len() >= 2 && parts[0] == "GET" {
        match parts[1] {
            "/health" => ("200 OK", "ok".to_string()),
            "/metrics" => {
                let app = state.read().await;
                ("200 OK", serde_json::to_string(&app.metrics()).unwrap_or_else(|_| "{}".into()))
            }
            _ => ("404 Not Found", "not found".to_string()),
        }
    } else {
        ("400 Bad Request", "bad request".to_string())
    };

    let response = format!(
        "HTTP/1.1 {}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\n\r\n{}",
        status,
        body.len(),
        body
    );

    writer.write_all(response.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn handle_command_connection(
    stream: TcpStream,
    peer: SocketAddr,
    state: SharedState,
    ws_hub: Arc<Mutex<WsHub>>,
    rate_limiter: Arc<RateLimiter>,
) -> Result<()> {
    let key = peer.ip().to_string();
    let allowed = rate_limiter
        .allow(&key, CMD_RATE_LIMIT, std::time::Duration::from_secs(CMD_RATE_WINDOW_SECS))
        .await;
    if !allowed {
        info!(%peer, "command rate limit exceeded");
        return Ok(());
    }

    info!(%peer, "command connection established");

    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut line = String::new();

    // first message must be API key
    line.clear();
    let first_bytes = read_line_with_limit(&mut reader, &mut line).await?;
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
        let bytes = match read_line_with_limit(&mut reader, &mut line).await {
            Ok(n) => n,
            Err(err) => {
                let msg = format!("ERROR {}\n", err);
                writer.write_all(msg.as_bytes()).await?;
                writer.flush().await?;
                return Ok(());
            }
        };
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


