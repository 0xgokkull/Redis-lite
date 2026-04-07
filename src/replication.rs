use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationRole {
    Master,
    Slave,
}

impl std::fmt::Display for ReplicationRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Master => write!(f, "master"),
            Self::Slave => write!(f, "slave"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReplicationState {
    pub role: ReplicationRole,
    pub replication_id: String,
    pub replication_offset: i64,
    pub master_host: Option<String>,
    pub master_port: Option<u16>,
}

impl Default for ReplicationState {
    fn default() -> Self {
        Self {
            role: ReplicationRole::Master,
            replication_id: generate_replication_id(),
            replication_offset: 0,
            master_host: None,
            master_port: None,
        }
    }
}

impl ReplicationState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn become_master(&mut self) {
        self.role = ReplicationRole::Master;
        self.replication_id = generate_replication_id();
        self.master_host = None;
        self.master_port = None;
    }

    pub fn become_slave(&mut self, host: String, port: u16) {
        self.role = ReplicationRole::Slave;
        self.master_host = Some(host);
        self.master_port = Some(port);
        self.replication_offset = 0;
    }

    pub fn increment_offset(&mut self, bytes: i64) {
        self.replication_offset = self.replication_offset.saturating_add(bytes);
    }

    pub fn info_string(&self) -> String {
        match self.role {
            ReplicationRole::Master => format!(
                "role:{}\r\nreplication_id:{}\r\nreplication_offset:{}\r\n",
                self.role, self.replication_id, self.replication_offset
            ),
            ReplicationRole::Slave => {
                let master_host = self.master_host.as_deref().unwrap_or("unknown");
                let master_port = self.master_port.unwrap_or(0);
                format!(
                    "role:{}\r\nmaster_host:{}\r\nmaster_port:{}\r\nmaster_repl_offset:{}\r\n",
                    self.role, master_host, master_port, self.replication_offset
                )
            }
        }
    }
}

#[derive(Debug)]
pub struct ReplicaConnections {
    connections: HashMap<String, Arc<Mutex<TcpStream>>>,
}

impl ReplicaConnections {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub async fn add_replica(&mut self, id: String, stream: TcpStream) {
        self.connections.insert(id, Arc::new(Mutex::new(stream)));
    }

    pub async fn broadcast_command(&self, command_line: &str) -> Result<(), AppError> {
        let command_bytes = format!("{}\n", command_line).into_bytes();
        for stream_arc in self.connections.values() {
            let mut stream = stream_arc.lock().await;
            stream
                .write_all(&command_bytes)
                .await
                .map_err(AppError::Io)?;
        }
        Ok(())
    }

    pub fn replica_count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for ReplicaConnections {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_replication_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"0123456789abcdef";
    let mut rng = rand::thread_rng();
    (0..40)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replication_state_defaults_to_master() {
        let state = ReplicationState::new();
        assert_eq!(state.role, ReplicationRole::Master);
        assert_eq!(state.replication_offset, 0);
    }

    #[test]
    fn become_slave_updates_role_and_master() {
        let mut state = ReplicationState::new();
        state.become_slave("localhost".to_string(), 6379);
        assert_eq!(state.role, ReplicationRole::Slave);
        assert_eq!(state.master_host, Some("localhost".to_string()));
        assert_eq!(state.master_port, Some(6379));
    }

    #[test]
    fn become_master_resets_slave_state() {
        let mut state = ReplicationState::new();
        state.become_slave("localhost".to_string(), 6379);
        state.become_master();
        assert_eq!(state.role, ReplicationRole::Master);
        assert_eq!(state.master_host, None);
    }

    #[test]
    fn increment_offset_works() {
        let mut state = ReplicationState::new();
        state.increment_offset(100);
        assert_eq!(state.replication_offset, 100);
        state.increment_offset(50);
        assert_eq!(state.replication_offset, 150);
    }

    #[test]
    fn info_string_contains_role() {
        let state = ReplicationState::new();
        let info = state.info_string();
        assert!(info.contains("role:master"));
    }
}
