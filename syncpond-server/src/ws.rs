use crate::rate_limiter::RateLimiter;
use crate::state::{AppState, SharedState};
use crate::commands::RoomUpdate;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, net::SocketAddr, sync::Arc, time::{SystemTime, UNIX_EPOCH}};
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

const WS_AUTH_LIMIT: usize = 10;
const WS_AUTH_WINDOW_SECS: u64 = 60;

fn validate_jwt_claims(app: &AppState, token: &str) -> Result<Claims, String> {
    let jwt_key = app
        .jwt_key
        .as_ref()
        .ok_or_else(|| "no_jwt_key".to_string())?;

    let mut validation = Validation::new(Algorithm::HS256);
    // Explicitly require `exp` and enforce expiration.
    validation.set_required_spec_claims(&["exp"]);
    validation.validate_exp = true;

    if let Some(issuer) = app.jwt_issuer.as_ref() {
        validation.set_issuer(&[issuer]);
    }
    if let Some(audience) = app.jwt_audience.as_ref() {
        validation.set_audience(&[audience]);
    }

    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(jwt_key.as_ref()), &validation)
        .map_err(|e| format!("invalid_jwt:{}", e))?;

    let claims = token_data.claims;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as usize;
    if claims.exp <= now {
        return Err("expired_jwt".to_string());
    }

    Ok(claims)
}

pub async fn handle_ws_connection(
    stream: TcpStream,
    peer: SocketAddr,
    state: SharedState,
    ws_hub: Arc<Mutex<WsHub>>,
    rate_limiter: Arc<RateLimiter>,
) -> Result<()> {
    let key = peer.ip().to_string();
    let allowed = rate_limiter
        .allow(&key, WS_AUTH_LIMIT, std::time::Duration::from_secs(WS_AUTH_WINDOW_SECS))
        .await;
    if !allowed {
        info!(%peer, "ws auth rate limit exceeded");
        return Ok(());
    }

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

    let app = state.read().await;

    let claims = match validate_jwt_claims(&app, &auth_msg.jwt) {
        Ok(claims) => claims,
        Err(reason) => {
            let err = json!({"type":"auth_error","reason":"invalid_jwt","detail": reason});
            ws_sender.send(Message::Text(err.to_string())).await.ok();
            return Ok(());
        }
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use jsonwebtoken::{encode, Header};
    use serde::Serialize;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Serialize)]
    struct IncompleteClaims {
        sub: String,
        room: String,
        containers: Vec<String>,
    }

    #[tokio::test]
    async fn test_validate_jwt_claims_success() {
        let mut app = AppState::new();
        app.set_jwt_key("secret".to_string());
        app.set_jwt_issuer("my-issuer".to_string());
        app.set_jwt_audience("my-aud".to_string());

        let room_id = app.create_room();
        let token = app.create_room_token(room_id, &["public".into()]).unwrap();

        let claims = validate_jwt_claims(&app, &token).expect("token should be valid");
        assert_eq!(claims.room, room_id.to_string());
    }

    #[tokio::test]
    async fn test_validate_jwt_claims_expired() {
        let mut app = AppState::new();
        app.set_jwt_key("secret".to_string());

        let past = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 3600;

        #[derive(Debug, Serialize)]
        struct ExpiredClaims {
            sub: String,
            room: String,
            containers: Vec<String>,
            exp: usize,
        }

        let claims = ExpiredClaims {
            sub: "room:1".into(),
            room: "1".into(),
            containers: vec!["public".into()],
            exp: past as usize,
        };

        let token = encode(&Header::default(), &claims, &jsonwebtoken::EncodingKey::from_secret("secret".as_ref())).unwrap();
        let err = validate_jwt_claims(&app, &token).unwrap_err();
        assert!(err.contains("expired_jwt") || err.contains("invalid_jwt"));
    }

    #[tokio::test]
    async fn test_validate_jwt_claims_missing_exp() {
        let mut app = AppState::new();
        app.set_jwt_key("secret".to_string());

        let claims = IncompleteClaims {
            sub: "room:1".into(),
            room: "1".into(),
            containers: vec!["public".into()],
        };

        let token = encode(&Header::default(), &claims, &jsonwebtoken::EncodingKey::from_secret("secret".as_ref())).unwrap();
        let err = validate_jwt_claims(&app, &token).unwrap_err();
        assert!(err.contains("invalid_jwt"));
    }
}
