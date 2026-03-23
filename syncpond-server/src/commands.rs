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
                Ok(()) => (
                    "OK".into(),
                    vec![RoomUpdate {
                        room_id,
                        container,
                        key,
                        value: Some(value),
                        room_counter: app.rooms.get(&room_id).map(|r| r.room_counter).unwrap_or(0),
                    }],
                ),
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
                Ok(()) => (
                    "OK".into(),
                    vec![RoomUpdate {
                        room_id,
                        container,
                        key,
                        value: None,
                        room_counter: app.rooms.get(&room_id).map(|r| r.room_counter).unwrap_or(0),
                    }],
                ),
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
                    let value_text = serde_json::to_string(v).unwrap_or_else(|_| "null".into());
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
                    let room_counter = app.rooms.get(&room_id).map(|r| r.room_counter).unwrap_or(0);
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
    words
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| "ERROR missing_argument".into())
}

fn error_of(err: StateError) -> String {
    format!("ERROR {}", err)
}
