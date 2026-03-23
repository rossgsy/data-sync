use crate::state::{SharedState, StateError};
use serde_json::Value;

pub async fn process_command(line: &str, state: &SharedState) -> String {
    let mut words = line.splitn(5, ' ');
    let cmd = match words.next() {
        Some(s) => s,
        None => return "ERROR empty command".into(),
    };

    match cmd {
        "ROOM.CREATE" => {
            let mut app = state.write().await;
            let room_id = app.create_room();
            format!("OK {}", room_id)
        }
        "ROOM.DELETE" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let mut app = state.write().await;
            match app.delete_room(room_id) {
                Ok(()) => "OK".into(),
                Err(e) => error_of(e),
            }
        }
        "SET" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let container = match parse_next_string(&mut words) {
                Ok(c) => c,
                Err(err) => return err,
            };
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return err,
            };
            let value_json = words.next().unwrap_or("");
            let value: Value = match serde_json::from_str(value_json) {
                Ok(v) => v,
                Err(err) => return format!("ERROR invalid_json {}", err),
            };
            let mut app = state.write().await;
            match app.set_fragment(room_id, container, key, value) {
                Ok(()) => "OK".into(),
                Err(e) => error_of(e),
            }
        }
        "DEL" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let container = match parse_next_string(&mut words) {
                Ok(c) => c,
                Err(err) => return err,
            };
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return err,
            };
            let mut app = state.write().await;
            match app.del_fragment(room_id, container, key) {
                Ok(()) => "OK".into(),
                Err(e) => error_of(e),
            }
        }
        "GET" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let container = match parse_next_string(&mut words) {
                Ok(c) => c,
                Err(err) => return err,
            };
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return err,
            };
            let app = state.read().await;
            match app.get_fragment(room_id, &container, &key) {
                Ok((v, kv)) => {
                    let value_text = serde_json::to_string(v).unwrap_or_else(|_| "null".into());
                    format!("OK {} {}", value_text, kv)
                }
                Err(e) => error_of(e),
            }
        }
        "VERSION" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let app = state.read().await;
            match app.room_version(room_id) {
                Ok(v) => format!("OK {}", v),
                Err(e) => error_of(e),
            }
        }
        "SET.JWTKEY" => {
            let key = match parse_next_string(&mut words) {
                Ok(k) => k,
                Err(err) => return err,
            };
            let mut app = state.write().await;
            app.set_jwt_key(key);
            "OK".into()
        }
        "TX.BEGIN" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let mut app = state.write().await;
            match app.tx_begin(room_id) {
                Ok(()) => "OK".into(),
                Err(e) => error_of(e),
            }
        }
        "TX.END" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let mut app = state.write().await;
            match app.tx_end(room_id) {
                Ok(()) => "OK".into(),
                Err(e) => error_of(e),
            }
        }
        "TX.ABORT" => {
            let room_id = match parse_next_room_id(&mut words) {
                Ok(id) => id,
                Err(err) => return err,
            };
            let mut app = state.write().await;
            match app.tx_abort(room_id) {
                Ok(()) => "OK".into(),
                Err(e) => error_of(e),
            }
        }
        _ => "ERROR unknown_command".into(),
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
