use futures_util::{sink::SinkExt, stream::StreamExt};
use native_tls::TlsConnector as NativeTlsConnector;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use tokio_tungstenite::{client_async_tls_with_config, Connector, tungstenite::Message};
use url::Url;

#[derive(Deserialize, Default)]
struct ClientSettings {
    ws_addr: Option<String>,
    jwt: Option<String>,
    last_seen_counter: Option<u64>,
}

#[derive(Serialize)]
struct AuthMessage<'a> {
    #[serde(rename = "type")]
    typ: &'a str,
    jwt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_seen_counter: Option<u64>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerEvent {
    AuthOk {
        room_counter: u64,
        state: serde_json::Value,
    },
    AuthError { reason: String },
    RoomUpdate {
        room_id: u64,
        room_counter: u64,
    },
    Update {
        room_id: u64,
        room_counter: u64,
        container: String,
        key: String,
        value: Option<serde_json::Value>,
        deleted: Option<bool>,
    },
    #[serde(other)]
    Other,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let cfg_path = args
        .iter()
        .find_map(|arg| {
            if arg.starts_with("--config=") {
                Some(arg.trim_start_matches("--config=").to_string())
            } else {
                None
            }
        })
        .or_else(|| args.get(1).cloned());

    let mut config = ClientSettings::default();
    if let Some(path) = cfg_path {
        if let Ok(text) = fs::read_to_string(&path) {
            match serde_yaml::from_str::<ClientSettings>(&text) {
                Ok(parsed) => config = parsed,
                Err(err) => eprintln!("Warning: failed to parse config {}: {}", path, err),
            }
        } else {
            eprintln!("Warning: failed to read config file {}, using defaults", path);
        }
    }

    let address = config.ws_addr.as_deref().unwrap_or("ws://localhost:8080/ws");
    let jwt = config.jwt.as_deref().unwrap_or("");

    println!("Connecting to {}", address);
    let url = match Url::parse(address) {
        Ok(u) => u,
        Err(err) => {
            eprintln!("Invalid websocket URL {}: {}", address, err);
            return;
        }
    };

    let host = match url.host_str() {
        Some(h) => h,
        None => {
            eprintln!("Invalid URL, missing host: {}", address);
            return;
        }
    };

    let port = url.port_or_known_default().unwrap_or(80);

    let tcp_addr = format!("{}:{}", host, port);
    let tcp_stream = match tokio::net::TcpStream::connect(tcp_addr).await {
        Ok(s) => s,
        Err(err) => {
            eprintln!("TCP connect failed: {}", err);
            return;
        }
    };

    let tls_connector = NativeTlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .expect("Failed to build TLS connector");

    let connector = Connector::NativeTls(tls_connector.into());

    let (mut ws_stream, _) = match client_async_tls_with_config(address, tcp_stream, None, Some(connector)).await {
        Ok(pair) => pair,
        Err(err) => {
            eprintln!("WebSocket handshake failed: {}", err);
            return;
        }
    };

    let auth = AuthMessage {
        typ: "auth",
        jwt,
        last_seen_counter: config.last_seen_counter,
    };

    if let Err(err) = ws_stream
        .send(Message::Text(serde_json::to_string(&auth).unwrap()))
        .await
    {
        eprintln!("Failed to send auth: {}", err);
        return;
    }

    println!("Auth request sent");

    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(Message::Text(txt)) => {
                match serde_json::from_str::<ServerEvent>(&txt) {
                    Ok(ServerEvent::AuthOk { room_counter, state }) => {
                        println!("AUTH_OK room_counter={} state={}", room_counter, state);
                    }
                    Ok(ServerEvent::AuthError { reason }) => {
                        println!("AUTH_ERROR: {}", reason);
                    }
                    Ok(ServerEvent::RoomUpdate { room_id, room_counter }) => {
                        println!("ROOM_UPDATE room={} counter={}", room_id, room_counter);
                    }
                    Ok(ServerEvent::Update { room_id, room_counter, container, key, value, deleted }) => {
                        if deleted.unwrap_or(false) {
                            println!("FRAGMENT_DELETED room={} container={} key={} counter={}", room_id, container, key, room_counter);
                        } else {
                            println!("FRAGMENT_UPDATED room={} container={} key={} value={} counter={}", room_id, container, key, value.unwrap_or(serde_json::Value::Null), room_counter);
                        }
                    }
                    Ok(ServerEvent::Other) => println!("UNKNOWN_EVENT {}", txt),
                    Err(_) => println!("RAW: {}", txt),
                }
            }
            Ok(Message::Ping(_)) => println!("Ping received"),
            Ok(Message::Pong(_)) => println!("Pong received"),
            Ok(Message::Close(frame)) => {
                println!("Closed {:?}", frame);
                break;
            }
            Ok(other) => println!("Non-text message: {:?}", other),
            Err(err) => {
                eprintln!("WS error: {}", err);
                break;
            }
        }
    }

    println!("Connection loop ended");
}
