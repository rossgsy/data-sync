use crate::state::SharedState;
use crate::commands::RoomUpdate;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, net::SocketAddr, sync::Arc};
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex},
};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info};
use uuid::Uuid;

pub struct ClientInfo {
    pub allowed_containers: HashSet<String>,
    pub sender: mpsc::UnboundedSender<Value>,
}

pub struct WsHub {
    rooms: HashMap<u64, HashMap<Uuid, ClientInfo>>,
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
        }
    }

    pub fn add_client(&mut self, room_id: u64, client_id: Uuid, client: ClientInfo) {
        self.rooms
            .entry(room_id)
            .or_insert_with(HashMap::new)
            .insert(client_id, client);
    }

    pub fn remove_client(&mut self, room_id: u64, client_id: &Uuid) {
        if let Some(room_clients) = self.rooms.get_mut(&room_id) {
            room_clients.remove(client_id);
            if room_clients.is_empty() {
                self.rooms.remove(&room_id);
            }
        }
    }

    pub async fn broadcast_update(&mut self, update: RoomUpdate) {
        let event = if update.container == "*" && update.key == "*" {
            json!({
                "type": "room_update",
                "room_id": update.room_id,
                "room_counter": update.room_counter,
            })
        } else if update.value.is_some() {
            json!({
                "type": "update",
                "room_id": update.room_id,
                "room_counter": update.room_counter,
                "container": update.container,
                "key": update.key,
                "value": update.value,
            })
        } else {
            json!({
                "type": "update",
                "room_id": update.room_id,
                "room_counter": update.room_counter,
                "container": update.container,
                "key": update.key,
                "deleted": true,
            })
        };

        let senders: Vec<_> = match self.rooms.get(&update.room_id) {
            Some(room_clients) => room_clients
                .values()
                .filter(|client| {
                    update.container == "*"
                        || update.container == "public"
                        || client.allowed_containers.contains(&update.container)
                })
                .map(|client| client.sender.clone())
                .collect(),
            None => return,
        };

        for sender in senders {
            let _ = sender.send(event.clone());
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub room: String,
    pub containers: Option<Vec<String>>,
    pub exp: usize,
}

#[derive(Debug, Deserialize)]
pub struct AuthMessage {
    #[serde(rename = "type")]
    pub typ: String,
    pub jwt: String,
    pub last_seen_counter: Option<u64>,
}

pub async fn handle_ws_connection(
    stream: TcpStream,
    peer: SocketAddr,
    state: SharedState,
    ws_hub: Arc<Mutex<WsHub>>,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!(%peer, "websocket connection established");

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let auth_text = match ws_receiver.next().await {
        Some(Ok(Message::Text(txt))) => txt,
        Some(Ok(_)) => {
            let err = json!({"type":"auth_error","reason":"invalid_auth_message"});
            ws_sender.send(Message::Text(err.to_string())).await.ok();
            return Ok(());
        }
        Some(Err(err)) => return Err(err.into()),
        None => return Ok(()),
    };

    let auth_msg: AuthMessage = match serde_json::from_str(&auth_text) {
        Ok(v) => v,
        Err(_) => {
            let err = json!({"type":"auth_error","reason":"invalid_json"});
            ws_sender.send(Message::Text(err.to_string())).await.ok();
            return Ok(());
        }
    };

    if auth_msg.typ != "auth" {
        let err = json!({"type":"auth_error","reason":"missing_auth"});
        ws_sender.send(Message::Text(err.to_string())).await.ok();
        return Ok(());
    }

    let jwt_key = {
        let app = state.read().await;
        match app.jwt_key.clone() {
            Some(k) => k,
            None => {
                let err = json!({"type":"auth_error","reason":"no_jwt_key"});
                ws_sender.send(Message::Text(err.to_string())).await.ok();
                return Ok(());
            }
        }
    };

    let validation = {
        let app = state.read().await;
        let mut v = Validation::new(Algorithm::HS256);
        if let Some(issuer) = app.jwt_issuer.as_ref() {
            v.set_issuer(&[issuer.clone()]);
        }
        if let Some(audience) = app.jwt_audience.as_ref() {
            v.set_audience(&[audience.clone()]);
        }
        v
    };

    let token_data = match decode::<Claims>(
        &auth_msg.jwt,
        &DecodingKey::from_secret(jwt_key.as_ref()),
        &validation,
    ) {
        Ok(data) => data,
        Err(_) => {
            let err = json!({"type":"auth_error","reason":"invalid_jwt"});
            ws_sender.send(Message::Text(err.to_string())).await.ok();
            return Ok(());
        }
    };

    let claims = token_data.claims;
    let room_id: u64 = match claims.room.parse() {
        Ok(id) => id,
        Err(_) => {
            let err = json!({"type":"auth_error","reason":"invalid_room_claim"});
            ws_sender.send(Message::Text(err.to_string())).await.ok();
            return Ok(());
        }
    };

    let _last_seen_counter = auth_msg.last_seen_counter;

    let mut allowed_containers: HashSet<String> = claims
        .containers
        .unwrap_or_default()
        .into_iter()
        .collect();
    allowed_containers.insert("public".to_string());

    let room_state_snapshot = {
        let app = state.read().await;
        match app.room_snapshot(room_id, &allowed_containers) {
            Some(s) => s,
            None => {
                let err = json!({"type":"auth_error","reason":"room_not_found"});
                ws_sender.send(Message::Text(err.to_string())).await.ok();
                return Ok(());
            }
        }
    };

    let room_counter = {
        let app = state.read().await;
        app.room_version(room_id).unwrap_or(0)
    };

    let auth_ok = json!({
        "type": "auth_ok",
        "room_counter": room_counter,
        "state": room_state_snapshot,
    });

    ws_sender.send(Message::Text(auth_ok.to_string())).await?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let client_id = Uuid::new_v4();

    {
        let mut hub = ws_hub.lock().await;
        hub.add_client(
            room_id,
            client_id,
            ClientInfo {
                allowed_containers: allowed_containers.clone(),
                sender: tx,
            },
        );
    }

    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Ping(payload))) => {
                        ws_sender.send(Message::Pong(payload)).await.ok();
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(frame))) => {
                        info!(%peer, ?frame, "ws close by client");
                        ws_sender.close().await.ok();
                        break;
                    }
                    Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) => {
                        let err = json!({"type":"auth_error","reason":"unexpected_message_after_auth"});
                        ws_sender.send(Message::Text(err.to_string())).await.ok();
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        error!(%err, "ws receive error");
                        break;
                    }
                    None => break,
                }
            }
            event = rx.recv() => {
                match event {
                    Some(event) => {
                        if let Err(err) = ws_sender.send(Message::Text(event.to_string())).await {
                            error!(%err, "ws outgoing error");
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    {
        let mut hub = ws_hub.lock().await;
        hub.remove_client(room_id, &client_id);
    }

    {
        let mut app = state.write().await;
        app.total_ws_connections = app.total_ws_connections.saturating_sub(1);
        info!(total_ws_connections = app.total_ws_connections, "ws connections after close");
    }

    Ok(())
}