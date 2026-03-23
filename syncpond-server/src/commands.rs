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
    let mut remainder = line.trim_start();

    let (cmd, rest) = match take_token(remainder) {
        Ok((token, rest)) => (token, rest),
        Err(e) => return (e, vec![]),
    };
    remainder = rest;

    match cmd.as_ref() {
        "ROOM.CREATE" => {
            if !remainder.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let mut app = state.write().await;
            let room_id = app.create_room();
            (format!("OK {}", room_id), vec![])
        }
        "ROOM.DELETE" => {
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let mut app = state.write().await;
            match app.delete_room(room_id) {
                Ok(()) => ("OK".into(), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "ROOM.LIST" => {
            if !remainder.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let app = state.read().await;
            let rooms = app.list_rooms();
            let payload = serde_json::to_string(&rooms).unwrap_or_else(|_| "[]".into());
            (format!("OK {}", payload), vec![])
        }
        "ROOM.INFO" => {
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let app = state.read().await;
            match app.room_info(room_id) {
                Ok(info) => (format!("OK {}", info), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "SET" => {
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let (container, rest) = match take_token(rest) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let (key, rest) = match take_token(rest) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let value_json = rest.trim_start();
            if value_json.is_empty() {
                return ("ERROR missing_value".into(), vec![]);
            }
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
                }
                Err(e) => (error_of(e), vec![]),
            }
        }
        "DEL" => {
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let (container, rest) = match take_token(rest) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let (key, rest) = match take_token(rest) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
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
                            value: Some(serde_json::Value::Null),
                            room_counter,
                        }],
                    )
                }
                Err(e) => (error_of(e), vec![]),
            }
        }
        "GET" => {
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let (container, rest) = match take_token(rest) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let (key, rest) = match take_token(rest) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
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
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let app = state.read().await;
            match app.room_version(room_id) {
                Ok(v) => (format!("OK {}", v), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "SET.JWTKEY" => {
            let (key, rest) = match take_token(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let mut app = state.write().await;
            app.set_jwt_key(key);
            ("OK".into(), vec![])
        }
        "TX.BEGIN" => {
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let mut app = state.write().await;
            match app.tx_begin(room_id) {
                Ok(()) => ("OK".into(), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "TX.END" => {
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
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
            let (room_id, rest) = match parse_room_id_from_remainder(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            if !rest.trim().is_empty() {
                return ("ERROR extra_arguments".into(), vec![]);
            }
            let mut app = state.write().await;
            match app.tx_abort(room_id) {
                Ok(()) => ("OK".into(), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        "TOKEN.GEN" => {
            let (room_id_str, rest) = match take_token(remainder) {
                Ok(x) => x,
                Err(err) => return (err, vec![]),
            };
            let room_id = match room_id_str.parse::<u64>() {
                Ok(id) => id,
                Err(_) => return ("ERROR invalid_room_id".into(), vec![]),
            };
            let mut containers = Vec::new();
            let mut leftover = rest;
            while !leftover.trim().is_empty() {
                match take_token(leftover) {
                    Ok((tok, rem)) => {
                        containers.push(tok);
                        leftover = rem;
                    }
                    Err(err) => return (err, vec![]),
                }
            }
            let app = state.read().await;
            match app.create_room_token(room_id, &containers) {
                Ok(token) => (format!("OK {}", token), vec![]),
                Err(e) => (error_of(e), vec![]),
            }
        }
        _ => ("ERROR unknown_command".into(), vec![]),
    }
}

fn take_token(input: &str) -> Result<(String, &str), String> {
    let input = input.trim_start();
    if input.is_empty() {
        return Err("ERROR missing_argument".into());
    }

    if input.starts_with('"') {
        let mut buf = String::new();
        let mut escaped = false;
        let mut found_end = false;
        for (i, c) in input[1..].char_indices() {
            if escaped {
                match c {
                    '\\' => buf.push('\\'),
                    '"' => buf.push('"'),
                    'n' => buf.push('\n'),
                    'r' => buf.push('\r'),
                    't' => buf.push('\t'),
                    other => buf.push(other),
                }
                escaped = false;
                continue;
            }

            if c == '\\' {
                escaped = true;
                continue;
            }

            if c == '"' {
                let end = 1 + i + c.len_utf8();
                let rest = &input[end..];
                return Ok((buf, rest));
            }

            buf.push(c);
        }

        Err("ERROR invalid_argument".into())
    } else {
        let mut end = input.len();
        for (i, c) in input.char_indices() {
            if c.is_whitespace() {
                end = i;
                break;
            }
        }
        let token = input[..end].to_string();
        let rest = &input[end..];
        Ok((token, rest))
    }
}

fn parse_room_id_from_remainder(remainder: &str) -> Result<(u64, &str), String> {
    let (room_id, rest) = take_token(remainder)?;
    let parsed = room_id
        .parse::<u64>()
        .map_err(|_| "ERROR invalid_room_id".to_string())?;
    Ok((parsed, rest))
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
        assert_eq!(resp, "ERROR tombstone");
    }

    #[tokio::test]
    async fn test_process_command_quoted_and_json_spaces() {
        let state = sample_state();
        process_command("ROOM.CREATE", &state).await;

        let (resp, updates) = process_command("SET 1 \"my container\" \"complex key\" {\"a\": \"hello world\", \"b\": 123}", &state).await;
        assert_eq!(resp, "OK");
        assert_eq!(updates.len(), 1);

        let (resp, _) = process_command("GET 1 \"my container\" \"complex key\"", &state).await;
        assert!(resp.starts_with("OK {\"a\":\"hello world\",\"b\":123}"));
    }

    #[tokio::test]
    async fn test_process_command_malformed_command() {
        let state = sample_state();

        let (resp, _) = process_command("ROOM.CREATE extra", &state).await;
        assert_eq!(resp, "ERROR extra_arguments");

        let (resp, _) = process_command("SET 1 public key", &state).await;
        assert_eq!(resp, "ERROR missing_value");

        let (resp, _) = process_command("SET 1 public key invalid_json", &state).await;
        assert!(resp.starts_with("ERROR invalid_json"));
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
    async fn test_tx_end_with_del_tombstones_missing_key() {
        let state = sample_state();
        process_command("ROOM.CREATE", &state).await;

        let (resp, _) = process_command("TX.BEGIN 1", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("DEL 1 public missing-key", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("TX.END 1", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("GET 1 public missing-key", &state).await;
        assert_eq!(resp, "ERROR tombstone");
    }

    #[tokio::test]
    async fn test_tx_conflict_and_ordering_edge_cases() {
        let state = sample_state();
        process_command("ROOM.CREATE", &state).await;

        // Cannot begin second transaction while first is open.
        let (resp, _) = process_command("TX.BEGIN 1", &state).await;
        assert_eq!(resp, "OK");
        let (resp, _) = process_command("TX.BEGIN 1", &state).await;
        assert_eq!(resp, "ERROR tx_already_open");

        // Interleaved transactional operations are committed in order.
        let (resp, _) = process_command("SET 1 public a 1", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("DEL 1 public a", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("SET 1 public a 2", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("TX.END 1", &state).await;
        assert_eq!(resp, "OK");

        let (resp, _) = process_command("GET 1 public a", &state).await;
        assert!(resp.starts_with("OK 2"));
    }

    #[tokio::test]
    async fn test_token_gen_requires_jwt_key() {
        let state = sample_state();
        process_command("ROOM.CREATE", &state).await;

        let (resp, _) = process_command("TOKEN.GEN 1 public", &state).await;
        assert_eq!(resp, "ERROR jwt_key_not_configured");
    }
}

