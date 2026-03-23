use crate::state::{SharedState, StateError};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RoomUpdate {
    pub room_id: u64,
    pub container: String,
    pub key: String,
    pub value: Option<Value>,
    pub room_counter: u64,
}

pub async fn process_command(line: &str, state: &SharedState) -> (String, Vec<RoomUpdate>) {
    let mut words = line.splitn(5, ' ');
    let cmd = match words.next() {
        Some(s) => s,
        None => return ("ERROR empty command".into(), vec![]),
    };

    match cmd {
        "ROOM.CREATE" => {
            let mut app = state.write().await;
            let room_id = app.create_room();
            (format!("OK {}", room_id), vec![])
        }
        "ROOM.DELETE" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let mut app = state.write().await;
            match app.delete_room(room_id) {
                Ok(()) => ("OK".into(), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "ROOM.LIST" => {
            let app = state.read().await;
            let rooms = app.list_rooms();
            let payload = serde_json::to_string(&rooms).unwrap_or_else(|_| "[]".into());
            (format!("OK {}", payload), vec![])
        }
        "ROOM.INFO" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let app = state.read().await;
            match app.room_info(room_id) {
                Ok(info) => (format!("OK {}", info), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "SET" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let container = match parse_next_string(&mut words) {
                Ok(c) => c,
                Err(err) => return (err, vec![]),
            };
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return (err, vec![]),
            };
            let value_json = words.next().unwrap_or("");
            let value: Value = match serde_json::from_str(value_json) {
                Ok(v) => v,
                Err(err) => return (format!("ERROR invalid_json {}", err), vec![]),
            };

            let mut app = state.write().await;
            match app.set_fragment(room_id, container.clone(), key.clone(), value.clone()) {
                Ok(()) => {
                    let room_counter = app
                        .rooms
                        .get(&room_id)
                        .and_then(|r| r.read().ok().map(|room| room.room_counter))
                        .unwrap_or(0);
                    (
                        "OK".into(),
                        vec![RoomUpdate {
                            room_id,
                            container,
                            key,
                            value: Some(value),
                            room_counter,
                        }],
                    )
                },
                Err(e) => (error_of(e), vec![]),
            }
        }
        "DEL" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let container = match parse_next_string(&mut words) {
                Ok(c) => c,
                Err(err) => return (err, vec![]),
            };
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return (err, vec![]),
            };
            let mut app = state.write().await;
            match app.del_fragment(room_id, container.clone(), key.clone()) {
                Ok(()) => {
                    let room_counter = app
                        .rooms
                        .get(&room_id)
                        .and_then(|r| r.read().ok().map(|room| room.room_counter))
                        .unwrap_or(0);
                    (
                        "OK".into(),
                        vec![RoomUpdate {
                            room_id,
                            container,
                            key,
                            value: None,
                            room_counter,
                        }],
                    )
                },
                Err(e) => (error_of(e), vec![]),
            }
        }
        "GET" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let container = match parse_next_string(&mut words) {
                Ok(c) => c,
                Err(err) => return (err, vec![]),
            };
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return (err, vec![]),
            };
            let app = state.read().await;
            match app.get_fragment(room_id, &container, &key) {
                Ok((v, kv)) => {
                    let value_text = serde_json::to_string(&v).unwrap_or_else(|_| "null".into());
                    (format!("OK {} {}", value_text, kv), vec![])
                }
                Err(e) => (error_of(e), vec![]),
            }
        }
        "VERSION" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let app = state.read().await;
            match app.room_version(room_id) {
                Ok(v) => (format!("OK {}", v), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "SET.JWTKEY" => {
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return (err, vec![]),
            };
            let mut app = state.write().await;
            app.set_jwt_key(key);
            ("OK".into(), vec![])
        }
        "TX.BEGIN" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let mut app = state.write().await;
            match app.tx_begin(room_id) {
                Ok(()) => ("OK".into(), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "TX.END" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let mut app = state.write().await;
            match app.tx_end(room_id) {
                Ok(()) => {
                    let room_counter = app.room_version(room_id).unwrap_or(0);
                    ("OK".into(), vec![RoomUpdate { room_id, container: "*".to_string(), key: "*".to_string(), value: None, room_counter }])
                }
                Err(e) => (error_of(e), vec![]),
            }
        }
        "TX.ABORT" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return (err, vec![]),
            };
            let mut app = state.write().await;
            match app.tx_abort(room_id) {
                Ok(()) => ("OK".into(), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "TOKEN.GEN" => {
            let mut tokens = line.split_whitespace();
            let _ = tokens.next(); // skip command
            let room_id_str = match tokens.next() {
                Some(v) => v,
                None => return ("ERROR invalid_room_id".into(), vec![]),
            };
            let room_id = match room_id_str.parse::<u64>() {
                Ok(id) => id,
                Err(_) => return ("ERROR invalid_room_id".into(), vec![]),
            };

            let containers: Vec<String> = tokens.map(String::from).collect();

            let app = state.read().await;
            match app.create_room_token(room_id, &containers) {
                Ok(token) => (format!("OK {}", token), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        _ => ("ERROR unknown_command".into(), vec![]),
    }
}

fn parse_next_room_id(words: &mut std::str::SplitN<'_, char>) -> Result<u64, String> {
    words
        .next()
        .ok_or_else(|| "ERROR invalid_room_id".into())
        .and_then(|s| s.parse::<u64>().map_err(|_| "ERROR invalid_room_id".into()))
}

fn parse_next_string(words: &mut std::str::SplitN<'_, char>) -> Result<String, String> {
    let token = words
        .next()
        .ok_or_else(|| String::from("ERROR missing_argument"))?;

    if token.starts_with('"') {
        if token.ends_with('"') && token.len() >= 2 {
            return Ok(token[1..token.len() - 1].to_string());
        }

        let mut accum = token[1..].to_string();
        while let Some(next) = words.next() {
            if next.ends_with('"') {
                accum.push(' ');
                accum.push_str(&next[..next.len() - 1]);
                return Ok(accum);
            }
            accum.push(' ');
            accum.push_str(next);
        }

        return Err("ERROR invalid_argument".into());
    }

    Ok(token.to_string())
}

fn error_of(err: StateError) -> String {
    format!("ERROR {}", err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, SharedState};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn sample_state() -> SharedState {
        let mut app = AppState::new();
        app.set_command_api_key("secret".to_string());
        Arc::new(RwLock::new(app))
    }

    #[tokio::test]
    async fn test_process_command_flow() {
        let state = sample_state();

        let (resp, _) = process_command("ROOM.CREATE", &state).await;
        assert!(resp.starts_with("OK"));

        let (resp, updates) = process_command("SET 1 public foo \"bar\"", &state).await;
        assert_eq!(resp, "OK");
        assert_eq!(updates.len(), 1);

        let (resp, _) = process_command("GET 1 public foo", &state).await;
        assert!(resp.starts_with("OK \"bar\""));

        let (resp, _) = process_command("VERSION 1", &state).await;
        assert_eq!(resp, "OK 1");

        let (resp, updates) = process_command("DEL 1 public foo", &state).await;
        assert_eq!(resp, "OK");
        assert_eq!(updates.len(), 1);

        let (resp, _) = process_command("GET 1 public foo", &state).await;
        assert_eq!(resp, "ERROR not_found");
    }

    #[tokio::test]
    async fn test_process_tx_sequence() {
        let state = sample_state();
        process_command("ROOM.CREATE", &state).await;

        let (resp, _) = process_command("TX.BEGIN 1", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("SET 1 public x 10", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("TX.END 1", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("GET 1 public x", &state).await;
        assert!(resp.starts_with("OK 10"));
    }

    #[tokio::test]
    async fn test_token_gen_requires_jwt_key() {
        let state = sample_state();
        process_command("ROOM.CREATE", &state).await;

        let (resp, _) = process_command("TOKEN.GEN 1 public", &state).await;
        assert_eq!(resp, "ERROR jwt_key_not_configured");
    }
}

