use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct FragmentEntry {
    pub value: Value,
    pub key_version: u64,
}

#[derive(Debug)]
pub struct RoomState {
    pub containers: HashMap<String, HashMap<String, FragmentEntry>>,
    pub room_counter: u64,
    pub tx_buffer: Option<Vec<RoomCommand>>,
}

#[derive(Debug)]
pub enum RoomCommand {
    Set { container: String, key: String, value: Value },
    Del { container: String, key: String },
}

#[derive(Debug)]
pub enum StateError {
    RoomNotFound,
    ContainerNotFound,
    FragmentNotFound,
    TxNotOpen,
    TxAlreadyOpen,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::RoomNotFound => write!(f, "room_not_found"),
            StateError::ContainerNotFound => write!(f, "container_not_found"),
            StateError::FragmentNotFound => write!(f, "not_found"),
            StateError::TxNotOpen => write!(f, "tx_not_open"),
            StateError::TxAlreadyOpen => write!(f, "tx_already_open"),
        }
    }
}

impl std::error::Error for StateError {}

pub type SharedState = Arc<RwLock<AppState>>;

#[derive(Debug)]
pub struct AppState {
    pub total_ws_connections: usize,
    pub rooms: HashMap<u64, RoomState>,
    pub next_room_id: u64,
    pub jwt_key: Option<String>,
    pub command_api_key: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            total_ws_connections: 0,
            rooms: HashMap::new(),
            next_room_id: 1,
            jwt_key: None,
            command_api_key: None,
        }
    }

    pub fn create_room(&mut self) -> u64 {
        let room_id = self.next_room_id;
        self.next_room_id += 1;
        self.rooms.insert(
            room_id,
            RoomState {
                containers: HashMap::new(),
                room_counter: 0,
                tx_buffer: None,
            },
        );
        room_id
    }

    pub fn delete_room(&mut self, room_id: u64) -> Result<(), StateError> {
        if self.rooms.remove(&room_id).is_some() {
            Ok(())
        } else {
            Err(StateError::RoomNotFound)
        }
    }

    pub fn set_fragment(
        &mut self,
        room_id: u64,
        container: String,
        key: String,
        value: Value,
    ) -> Result<(), StateError> {
        let room = self.rooms.get_mut(&room_id).ok_or(StateError::RoomNotFound)?;

        if let Some(buffer) = room.tx_buffer.as_mut() {
            buffer.push(RoomCommand::Set { container, key, value });
            return Ok(());
        }

        room.room_counter += 1;
        let container_map = room.containers.entry(container).or_default();
        container_map.insert(
            key,
            FragmentEntry {
                value,
                key_version: room.room_counter,
            },
        );

        Ok(())
    }

    pub fn del_fragment(&mut self, room_id: u64, container: String, key: String) -> Result<(), StateError> {
        let room = self.rooms.get_mut(&room_id).ok_or(StateError::RoomNotFound)?;

        if let Some(buffer) = room.tx_buffer.as_mut() {
            buffer.push(RoomCommand::Del { container, key });
            return Ok(());
        }

        let container_map = room
            .containers
            .get_mut(&container)
            .ok_or(StateError::ContainerNotFound)?;

        container_map.remove(&key);
        room.room_counter += 1;
        Ok(())
    }

    pub fn get_fragment(&self, room_id: u64, container: &str, key: &str) -> Result<(&Value, u64), StateError> {
        let room = self.rooms.get(&room_id).ok_or(StateError::RoomNotFound)?;
        let container_map = room
            .containers
            .get(container)
            .ok_or(StateError::ContainerNotFound)?;
        let fragment = container_map.get(key).ok_or(StateError::FragmentNotFound)?;

        Ok((&fragment.value, fragment.key_version))
    }

    pub fn room_version(&self, room_id: u64) -> Result<u64, StateError> {
        let room = self.rooms.get(&room_id).ok_or(StateError::RoomNotFound)?;
        Ok(room.room_counter)
    }

    pub fn set_jwt_key(&mut self, key: String) {
        self.jwt_key = Some(key);
    }

    pub fn set_command_api_key(&mut self, key: String) {
        self.command_api_key = Some(key);
    }

    pub fn tx_begin(&mut self, room_id: u64) -> Result<(), StateError> {
        let room = self.rooms.get_mut(&room_id).ok_or(StateError::RoomNotFound)?;
        if room.tx_buffer.is_some() {
            return Err(StateError::TxAlreadyOpen);
        }
        room.tx_buffer = Some(Vec::new());
        Ok(())
    }

    pub fn tx_end(&mut self, room_id: u64) -> Result<(), StateError> {
        let room = self.rooms.get_mut(&room_id).ok_or(StateError::RoomNotFound)?;
        let mut buffer = room.tx_buffer.take().ok_or(StateError::TxNotOpen)?;

        for op in buffer.drain(..) {
            match op {
                RoomCommand::Set { container, key, value } => {
                    room.room_counter += 1;
                    let container_map = room.containers.entry(container).or_default();
                    container_map.insert(
                        key,
                        FragmentEntry {
                            value,
                            key_version: room.room_counter,
                        },
                    );
                }
                RoomCommand::Del { container, key } => {
                    if let Some(container_map) = room.containers.get_mut(&container) {
                        if container_map.remove(&key).is_some() {
                            room.room_counter += 1;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn tx_abort(&mut self, room_id: u64) -> Result<(), StateError> {
        let room = self.rooms.get_mut(&room_id).ok_or(StateError::RoomNotFound)?;
        room.tx_buffer = None;
        Ok(())
    }

    pub fn room_snapshot(&self, room_id: u64, allowed_containers: &std::collections::HashSet<String>) -> Option<serde_json::Value> {
        let room = self.rooms.get(&room_id)?;
        let mut containers_json = serde_json::Map::new();

        for (container_name, fragments) in &room.containers {
            if container_name == "public" || allowed_containers.contains(container_name) {
                let mut container_map = serde_json::Map::new();
                for (key, entry) in fragments {
                    container_map.insert(key.clone(), entry.value.clone());
                }
                containers_json.insert(container_name.clone(), serde_json::Value::Object(container_map));
            }
        }

        Some(serde_json::json!({
            "room_counter": room.room_counter,
            "containers": serde_json::Value::Object(containers_json),
        }))
    }

    pub fn room_delta(
        &self,
        room_id: u64,
        since: u64,
        allowed_containers: &std::collections::HashSet<String>,
    ) -> Option<serde_json::Value> {
        let room = self.rooms.get(&room_id)?;
        if since >= room.room_counter {
            return Some(serde_json::json!({
                "room_counter": room.room_counter,
                "containers": serde_json::Value::Object(serde_json::Map::new()),
            }));
        }

        let mut containers_json = serde_json::Map::new();

        for (container_name, fragments) in &room.containers {
            if container_name != "public" && !allowed_containers.contains(container_name) {
                continue;
            }

            let mut container_map = serde_json::Map::new();
            for (key, entry) in fragments {
                if entry.key_version > since {
                    container_map.insert(key.clone(), entry.value.clone());
                }
            }

            if !container_map.is_empty() {
                containers_json.insert(container_name.clone(), serde_json::Value::Object(container_map));
            }
        }

        Some(serde_json::json!({
            "room_counter": room.room_counter,
            "containers": serde_json::Value::Object(containers_json),
        }))
    }
}
