pub mod command;
pub mod config;
pub mod error;
pub mod parser;
pub mod persistence;
pub mod replication;
pub mod server;
pub mod store;

use command::{Command, HELP_TEXT};
use config::EvictionPolicy;
use error::AppError;
use parser::parse_command;
use persistence::{append_aof_command, load_aof_commands, load_from_file, save_to_file};
use replication::{ReplicationRole, ReplicationState};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use store::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMessage {
    Continue(String),
    Exit(String),
}

pub struct RedisLite {
    store: Store,
    hashes: HashMap<String, HashMap<String, String>>,
    sets: HashMap<String, BTreeSet<String>>,
    lists: HashMap<String, VecDeque<String>>,
    zsets: HashMap<String, Vec<(i64, String)>>,
    key_types: HashMap<String, KeyType>,
    expirations: HashMap<String, u64>,
    access_order: HashMap<String, u64>,
    access_tick: u64,
    max_keys: Option<usize>,
    eviction_policy: EvictionPolicy,
    replication: ReplicationState,
    started_at_secs: u64,
    total_commands_processed: u64,
    read_commands_processed: u64,
    write_commands_processed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyType {
    String,
    Hash,
    Set,
    List,
    ZSet,
}

impl Default for RedisLite {
    fn default() -> Self {
        Self::new()
    }
}

impl RedisLite {
    pub fn new() -> Self {
        Self::with_limits(None, EvictionPolicy::NoEviction)
    }

    pub fn with_limits(max_keys: Option<usize>, eviction_policy: EvictionPolicy) -> Self {
        Self {
            store: Store::new(),
            hashes: HashMap::new(),
            sets: HashMap::new(),
            lists: HashMap::new(),
            zsets: HashMap::new(),
            key_types: HashMap::new(),
            expirations: HashMap::new(),
            access_order: HashMap::new(),
            access_tick: 0,
            max_keys,
            eviction_policy,
            replication: ReplicationState::new(),
            started_at_secs: now_secs(),
            total_commands_processed: 0,
            read_commands_processed: 0,
            write_commands_processed: 0,
        }
    }

    pub fn execute_line(&mut self, input: &str) -> Result<Option<RuntimeMessage>, AppError> {
        let command = match parse_command(input) {
            Ok(command) => command,
            Err(AppError::EmptyInput) => return Ok(None),
            Err(error) => return Err(error),
        };

        self.execute_command(command).map(Some)
    }

    pub fn execute_line_with_autosave(
        &mut self,
        input: &str,
        autosave_file: Option<&str>,
    ) -> Result<Option<RuntimeMessage>, AppError> {
        self.execute_line_with_persistence(input, autosave_file, None)
    }

    pub fn execute_line_with_persistence(
        &mut self,
        input: &str,
        autosave_file: Option<&str>,
        aof_file: Option<&str>,
    ) -> Result<Option<RuntimeMessage>, AppError> {
        let command = match parse_command(input) {
            Ok(command) => command,
            Err(AppError::EmptyInput) => return Ok(None),
            Err(error) => return Err(error),
        };

        let append_to_aof = command_appends_to_aof(&command);
        let mutates_state = command_mutates_state(&command);
        let message = self.execute_command(command.clone())?;

        if append_to_aof {
            if let Some(file) = aof_file {
                append_aof_command(file, &command)?;
            }
        }

        if mutates_state {
            if let Some(file) = autosave_file {
                self.save_to_path(file)?;
            }
        }

        Ok(Some(message))
    }

    pub fn save_to_path(&self, file: &str) -> Result<(), AppError> {
        save_to_file(file, self.store.data())
    }

    pub fn load_from_path(&mut self, file: &str) -> Result<(), AppError> {
        let loaded = load_from_file(file)?;
        self.store.replace_all(loaded);
        self.hashes.clear();
        self.sets.clear();
        self.lists.clear();
        self.zsets.clear();
        self.key_types.clear();
        self.expirations.clear();
        self.access_order.clear();
        let keys: Vec<String> = self.store.data().keys().cloned().collect();
        for key in keys {
            self.key_types.insert(key.clone(), KeyType::String);
            self.touch_key(&key);
        }
        Ok(())
    }

    pub fn replay_aof(&mut self, file: &str) -> Result<usize, AppError> {
        let commands = load_aof_commands(file)?;
        let count = commands.len();
        for command in commands {
            // Replay should not re-append to AOF or autosave while rebuilding state.
            let _ = self.execute_command(command)?;
        }
        Ok(count)
    }

    pub fn execute_command(&mut self, command: Command) -> Result<RuntimeMessage, AppError> {
        self.purge_expired_keys();
        self.total_commands_processed = self.total_commands_processed.saturating_add(1);
        if command_is_write(&command) {
            self.write_commands_processed = self.write_commands_processed.saturating_add(1);
        }
        if command_is_read(&command) {
            self.read_commands_processed = self.read_commands_processed.saturating_add(1);
        }

        match command {
            Command::Set { key, value } => {
                if !self.can_use_key_as(&key, KeyType::String) {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }

                let exists = self.key_types.contains_key(&key);
                if !exists {
                    self.ensure_capacity_for_insert()?;
                    self.key_types.insert(key.clone(), KeyType::String);
                }
                self.expirations.remove(&key);
                self.touch_key(&key);
                self.store.set(key, value);
                Ok(RuntimeMessage::Continue("OK".to_string()))
            }
            Command::Get { key } => {
                if self.key_types.get(&key).copied() != Some(KeyType::String)
                    && self.key_types.contains_key(&key)
                {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }
                let value = self.store.get(&key).cloned();
                if value.is_some() {
                    self.touch_key(&key);
                }
                Ok(RuntimeMessage::Continue(
                    value.unwrap_or_else(|| "(nil)".to_string()),
                ))
            }
            Command::Delete { key } => {
                let message = if self.remove_key_everywhere(&key) {
                    format!("deleted '{key}'")
                } else {
                    format!("key '{key}' not found")
                };
                Ok(RuntimeMessage::Continue(message))
            }
            Command::HSet { key, field, value } => {
                if !self.can_use_key_as(&key, KeyType::Hash) {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }
                if !self.key_types.contains_key(&key) {
                    self.ensure_capacity_for_insert()?;
                    self.key_types.insert(key.clone(), KeyType::Hash);
                }

                let hash = self.hashes.entry(key.clone()).or_default();
                let inserted = hash.insert(field, value).is_none();
                self.touch_key(&key);
                Ok(RuntimeMessage::Continue(
                    if inserted { "1" } else { "0" }.to_string(),
                ))
            }
            Command::HGet { key, field } => {
                if self.key_types.get(&key).copied() != Some(KeyType::Hash)
                    && self.key_types.contains_key(&key)
                {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }
                let value = self.hashes.get(&key).and_then(|h| h.get(&field).cloned());
                if self.hashes.contains_key(&key) {
                    self.touch_key(&key);
                }
                Ok(RuntimeMessage::Continue(
                    value.unwrap_or_else(|| "(nil)".to_string()),
                ))
            }
            Command::SAdd { key, member } => {
                if !self.can_use_key_as(&key, KeyType::Set) {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }
                if !self.key_types.contains_key(&key) {
                    self.ensure_capacity_for_insert()?;
                    self.key_types.insert(key.clone(), KeyType::Set);
                }
                let set = self.sets.entry(key.clone()).or_default();
                let inserted = set.insert(member);
                self.touch_key(&key);
                Ok(RuntimeMessage::Continue(
                    if inserted { "1" } else { "0" }.to_string(),
                ))
            }
            Command::SMembers { key } => {
                if self.key_types.get(&key).copied() != Some(KeyType::Set)
                    && self.key_types.contains_key(&key)
                {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }
                let members = self
                    .sets
                    .get(&key)
                    .map(|set| set.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                if self.sets.contains_key(&key) {
                    self.touch_key(&key);
                }
                if members.is_empty() {
                    Ok(RuntimeMessage::Continue("(empty)".to_string()))
                } else {
                    Ok(RuntimeMessage::Continue(members.join("\n")))
                }
            }
            Command::LPush { key, value } => {
                if !self.can_use_key_as(&key, KeyType::List) {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }
                if !self.key_types.contains_key(&key) {
                    self.ensure_capacity_for_insert()?;
                    self.key_types.insert(key.clone(), KeyType::List);
                }
                let new_len = {
                    let list = self.lists.entry(key.clone()).or_default();
                    list.push_front(value);
                    list.len()
                };
                self.touch_key(&key);
                Ok(RuntimeMessage::Continue(new_len.to_string()))
            }
            Command::RPop { key } => {
                if self.key_types.get(&key).copied() != Some(KeyType::List)
                    && self.key_types.contains_key(&key)
                {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }

                let popped = self.lists.get_mut(&key).and_then(|list| list.pop_back());
                if let Some(value) = popped {
                    if self
                        .lists
                        .get(&key)
                        .map(|list| list.is_empty())
                        .unwrap_or(false)
                    {
                        let _ = self.remove_key_everywhere(&key);
                    } else {
                        self.touch_key(&key);
                    }
                    Ok(RuntimeMessage::Continue(value))
                } else {
                    Ok(RuntimeMessage::Continue("(nil)".to_string()))
                }
            }
            Command::ZAdd { key, score, member } => {
                if !self.can_use_key_as(&key, KeyType::ZSet) {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }
                if !self.key_types.contains_key(&key) {
                    self.ensure_capacity_for_insert()?;
                    self.key_types.insert(key.clone(), KeyType::ZSet);
                }

                let zset = self.zsets.entry(key.clone()).or_default();
                let mut inserted = true;
                if let Some(entry) = zset.iter_mut().find(|(_, m)| *m == member) {
                    inserted = false;
                    entry.0 = score;
                } else {
                    zset.push((score, member));
                }
                zset.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
                self.touch_key(&key);
                Ok(RuntimeMessage::Continue(
                    if inserted { "1" } else { "0" }.to_string(),
                ))
            }
            Command::ZRange { key, start, stop } => {
                if self.key_types.get(&key).copied() != Some(KeyType::ZSet)
                    && self.key_types.contains_key(&key)
                {
                    return Err(AppError::Config(
                        "WRONGTYPE operation against a key holding the wrong kind of value"
                            .to_string(),
                    ));
                }

                let members = if let Some(zset) = self.zsets.get(&key) {
                    let len = zset.len();
                    match normalize_range(len, start, stop) {
                        Some((begin, end)) => zset[begin..=end]
                            .iter()
                            .map(|(_, member)| member.clone())
                            .collect::<Vec<_>>(),
                        None => Vec::new(),
                    }
                } else {
                    Vec::new()
                };

                if self.zsets.contains_key(&key) {
                    self.touch_key(&key);
                }

                if members.is_empty() {
                    Ok(RuntimeMessage::Continue("(empty)".to_string()))
                } else {
                    Ok(RuntimeMessage::Continue(members.join("\n")))
                }
            }
            Command::Expire { key, seconds } => {
                if !self.key_exists(&key) {
                    return Ok(RuntimeMessage::Continue("0".to_string()));
                }
                let expire_at = now_secs().saturating_add(seconds);
                self.expirations.insert(key, expire_at);
                Ok(RuntimeMessage::Continue("1".to_string()))
            }
            Command::Ttl { key } => {
                if !self.key_exists(&key) {
                    return Ok(RuntimeMessage::Continue("-2".to_string()));
                }
                if let Some(expire_at) = self.expirations.get(&key).copied() {
                    let now = now_secs();
                    let remaining = expire_at.saturating_sub(now);
                    Ok(RuntimeMessage::Continue(remaining.to_string()))
                } else {
                    Ok(RuntimeMessage::Continue("-1".to_string()))
                }
            }
            Command::Persist { key } => {
                if self.expirations.remove(&key).is_some() {
                    Ok(RuntimeMessage::Continue("1".to_string()))
                } else {
                    Ok(RuntimeMessage::Continue("0".to_string()))
                }
            }
            Command::Save { file } => {
                save_to_file(&file, self.store.data())?;
                Ok(RuntimeMessage::Continue(format!("saved to {file}")))
            }
            Command::Load { file } => {
                self.load_from_path(&file)?;
                Ok(RuntimeMessage::Continue(format!("loaded from {file}")))
            }
            Command::Backup { file } => {
                self.save_to_path(&file)?;
                Ok(RuntimeMessage::Continue(format!(
                    "backup created at {file}"
                )))
            }
            Command::Restore { file } => {
                self.load_from_path(&file)?;
                Ok(RuntimeMessage::Continue(format!("restored from {file}")))
            }
            Command::Slaveof { host, port } => {
                self.replication.become_slave(host.clone(), port);
                Ok(RuntimeMessage::Continue(format!(
                    "OK - attempting to replicate from {host}:{port}"
                )))
            }
            Command::SlaveofNoOne => {
                self.replication.become_master();
                Ok(RuntimeMessage::Continue(
                    "OK - promoted to master from replica mode".to_string(),
                ))
            }
            Command::Role => {
                let role = &self.replication.role;
                let info = match role {
                    ReplicationRole::Master => format!(
                        "master {} {} 0",
                        self.replication.replication_id, self.replication.replication_offset
                    ),
                    ReplicationRole::Slave => format!(
                        "slave {} {} {}",
                        self.replication.master_host.as_deref().unwrap_or("?"),
                        self.replication.master_port.unwrap_or(0),
                        self.replication.replication_offset
                    ),
                };
                Ok(RuntimeMessage::Continue(info))
            }
            Command::Info => Ok(RuntimeMessage::Continue(self.format_info())),
            Command::Replconf { subcommand, args } => {
                match subcommand.to_lowercase().as_str() {
                    "listening-port" => Ok(RuntimeMessage::Continue("OK".to_string())),
                    "capa" => Ok(RuntimeMessage::Continue("OK".to_string())),
                    "ack" => {
                        if let Some(offset_str) = args.first() {
                            if let Ok(offset) = offset_str.parse::<i64>() {
                                self.replication.replication_offset = offset;
                            }
                        }
                        Ok(RuntimeMessage::Continue("OK".to_string()))
                    }
                    _ => Err(AppError::Config(format!(
                        "unknown REPLCONF subcommand: {subcommand}"
                    ))),
                }
            }
            Command::Psync {
                replication_id,
                offset: _,
            } => {
                if replication_id == "?" {
                    let sync_response = format!(
                        "FULLRESYNC {} {}",
                        self.replication.replication_id, self.replication.replication_offset
                    );
                    Ok(RuntimeMessage::Continue(sync_response))
                } else {
                    let sync_response = format!(
                        "CONTINUE {}",
                        self.replication.replication_offset
                    );
                    Ok(RuntimeMessage::Continue(sync_response))
                }
            }
            Command::List => Ok(RuntimeMessage::Continue(self.format_entries())),
            Command::Help => Ok(RuntimeMessage::Continue(HELP_TEXT.to_string())),
            Command::Exit => Ok(RuntimeMessage::Exit("Exiting redis-lite.".to_string())),
        }
    }

    fn format_entries(&self) -> String {
        let mut keys = self.key_types.keys().cloned().collect::<Vec<_>>();
        keys.sort();

        let entries = keys
            .into_iter()
            .map(|key| {
                let kind = self.key_types.get(&key).copied().unwrap_or(KeyType::String);
                match kind {
                    KeyType::String => {
                        let value = self.store.get(&key).cloned().unwrap_or_default();
                        format!("{key} = {value}")
                    }
                    KeyType::Hash => {
                        let count = self.hashes.get(&key).map(|v| v.len()).unwrap_or(0);
                        format!("{key} (hash fields={count})")
                    }
                    KeyType::Set => {
                        let count = self.sets.get(&key).map(|v| v.len()).unwrap_or(0);
                        format!("{key} (set members={count})")
                    }
                    KeyType::List => {
                        let count = self.lists.get(&key).map(|v| v.len()).unwrap_or(0);
                        format!("{key} (list len={count})")
                    }
                    KeyType::ZSet => {
                        let count = self.zsets.get(&key).map(|v| v.len()).unwrap_or(0);
                        format!("{key} (zset members={count})")
                    }
                }
            })
            .collect::<Vec<_>>();

        if entries.is_empty() {
            return "(empty)".to_string();
        }

        entries.join("\n")
    }

    fn ensure_capacity_for_insert(&mut self) -> Result<(), AppError> {
        let Some(max_keys) = self.max_keys else {
            return Ok(());
        };

        if self.key_types.len() < max_keys {
            return Ok(());
        }

        match self.eviction_policy {
            EvictionPolicy::NoEviction => Err(AppError::NoMemory(
                "max key limit reached and eviction policy is noeviction".to_string(),
            )),
            EvictionPolicy::AllKeysLru => {
                if let Some(key_to_evict) = self.least_recently_used_key() {
                    self.remove_key_everywhere(&key_to_evict);
                    Ok(())
                } else {
                    Err(AppError::NoMemory(
                        "max key limit reached but no key available for eviction".to_string(),
                    ))
                }
            }
        }
    }

    fn least_recently_used_key(&self) -> Option<String> {
        self.access_order
            .iter()
            .min_by_key(|(_, tick)| **tick)
            .map(|(key, _)| key.clone())
    }

    fn touch_key(&mut self, key: &str) {
        self.access_tick = self.access_tick.saturating_add(1);
        self.access_order.insert(key.to_string(), self.access_tick);
    }

    fn purge_expired_keys(&mut self) {
        let now = now_secs();
        let expired: Vec<String> = self
            .expirations
            .iter()
            .filter(|(_, expire_at)| **expire_at <= now)
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired {
            self.remove_key_everywhere(&key);
        }
    }

    fn key_exists(&self, key: &str) -> bool {
        self.key_types.contains_key(key)
    }

    fn can_use_key_as(&self, key: &str, expected: KeyType) -> bool {
        match self.key_types.get(key).copied() {
            Some(found) => found == expected,
            None => true,
        }
    }

    fn remove_key_everywhere(&mut self, key: &str) -> bool {
        let existed = self.key_types.remove(key).is_some()
            || self.store.get(key).is_some()
            || self.hashes.contains_key(key)
            || self.sets.contains_key(key)
            || self.lists.contains_key(key)
            || self.zsets.contains_key(key);

        self.store.delete(key);
        self.hashes.remove(key);
        self.sets.remove(key);
        self.lists.remove(key);
        self.zsets.remove(key);
        self.expirations.remove(key);
        self.access_order.remove(key);
        existed
    }

    fn format_info(&self) -> String {
        let uptime = now_secs().saturating_sub(self.started_at_secs);
        let role_line = match self.replication.role {
            ReplicationRole::Master => "master".to_string(),
            ReplicationRole::Slave => "slave".to_string(),
        };

        format!(
            "# Server\r\nuptime_in_seconds:{uptime}\r\n\
# Stats\r\ntotal_commands_processed:{}\r\nread_commands_processed:{}\r\nwrite_commands_processed:{}\r\n\
# Keyspace\r\nkeys:{}\r\nexpires:{}\r\n\
# Replication\r\nrole:{}\r\nmaster_replid:{}\r\nmaster_repl_offset:{}\r\n",
            self.total_commands_processed,
            self.read_commands_processed,
            self.write_commands_processed,
            self.key_types.len(),
            self.expirations.len(),
            role_line,
            self.replication.replication_id,
            self.replication.replication_offset,
        )
    }
}

fn normalize_range(len: usize, start: i64, stop: i64) -> Option<(usize, usize)> {
    if len == 0 {
        return None;
    }

    let len_i64 = len as i64;
    let mut begin = if start < 0 { len_i64 + start } else { start };
    let mut end = if stop < 0 { len_i64 + stop } else { stop };

    if begin < 0 {
        begin = 0;
    }
    if end >= len_i64 {
        end = len_i64 - 1;
    }
    if begin > end || begin >= len_i64 {
        return None;
    }

    Some((begin as usize, end as usize))
}

fn now_secs() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}

fn command_mutates_state(command: &Command) -> bool {
    matches!(
        command,
        Command::Set { .. }
            | Command::Delete { .. }
            | Command::HSet { .. }
            | Command::SAdd { .. }
            | Command::LPush { .. }
            | Command::RPop { .. }
            | Command::ZAdd { .. }
            | Command::Expire { .. }
            | Command::Persist { .. }
            | Command::Load { .. }
            | Command::Restore { .. }
    )
}

fn command_appends_to_aof(command: &Command) -> bool {
    matches!(
        command,
        Command::Set { .. }
            | Command::Delete { .. }
            | Command::HSet { .. }
            | Command::SAdd { .. }
            | Command::LPush { .. }
            | Command::RPop { .. }
            | Command::ZAdd { .. }
            | Command::Expire { .. }
            | Command::Persist { .. }
    )
}

fn command_is_read(command: &Command) -> bool {
    matches!(
        command,
        Command::Get { .. }
            | Command::HGet { .. }
            | Command::SMembers { .. }
            | Command::ZRange { .. }
            | Command::Ttl { .. }
            | Command::Role
            | Command::Info
            | Command::List
            | Command::Help
    )
}

fn command_is_write(command: &Command) -> bool {
    command_mutates_state(command)
}

#[cfg(test)]
mod tests {
    use crate::config::EvictionPolicy;

    use super::{RedisLite, RuntimeMessage};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file_path(suffix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("redis_lite_lib_{nanos}_{suffix}.json"));
        path.to_string_lossy().to_string()
    }

    #[test]
    fn execute_set_then_get_returns_value() {
        let mut app = RedisLite::new();

        let set_result = app
            .execute_line("SET name gokul")
            .expect("SET should succeed")
            .expect("SET should return output");
        assert_eq!(set_result, RuntimeMessage::Continue("OK".to_string()));

        let get_result = app
            .execute_line("GET name")
            .expect("GET should succeed")
            .expect("GET should return output");
        assert_eq!(get_result, RuntimeMessage::Continue("gokul".to_string()));
    }

    #[test]
    fn execute_list_formats_sorted_entries() {
        let mut app = RedisLite::new();
        app.execute_line("SET theme dark")
            .expect("SET should succeed")
            .expect("SET should return output");
        app.execute_line("SET name gokul")
            .expect("SET should succeed")
            .expect("SET should return output");

        let list_result = app
            .execute_line("LIST")
            .expect("LIST should succeed")
            .expect("LIST should return output");
        assert_eq!(
            list_result,
            RuntimeMessage::Continue("name = gokul\ntheme = dark".to_string())
        );
    }

    #[test]
    fn execute_save_and_load_round_trip() {
        let file = temp_file_path("roundtrip");

        let mut app = RedisLite::new();
        app.execute_line("SET theme dark")
            .expect("SET should succeed")
            .expect("SET should return output");
        app.execute_line(&format!("SAVE {file}"))
            .expect("SAVE should succeed")
            .expect("SAVE should return output");

        let mut reloaded_app = RedisLite::new();
        reloaded_app
            .execute_line(&format!("LOAD {file}"))
            .expect("LOAD should succeed")
            .expect("LOAD should return output");
        let get_result = reloaded_app
            .execute_line("GET theme")
            .expect("GET should succeed")
            .expect("GET should return output");
        assert_eq!(get_result, RuntimeMessage::Continue("dark".to_string()));

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn execute_backup_and_restore_commands() {
        let file = temp_file_path("backup_restore");
        let mut app = RedisLite::new();

        app.execute_line("SET name gokul")
            .expect("SET should succeed")
            .expect("SET should return output");
        app.execute_line(&format!("BACKUP {file}"))
            .expect("BACKUP should succeed")
            .expect("BACKUP should return output");

        let mut restored = RedisLite::new();
        restored
            .execute_line(&format!("RESTORE {file}"))
            .expect("RESTORE should succeed")
            .expect("RESTORE should return output");
        let value = restored
            .execute_line("GET name")
            .expect("GET should succeed")
            .expect("GET should return output");

        assert_eq!(value, RuntimeMessage::Continue("gokul".to_string()));
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn execute_line_with_autosave_persists_mutation() {
        let file = temp_file_path("autosave");
        let mut app = RedisLite::new();

        let _ = app
            .execute_line_with_autosave("SET mode prod", Some(&file))
            .expect("autosave execution should succeed");

        let mut loaded = RedisLite::new();
        loaded.load_from_path(&file).expect("load should succeed");
        let value = loaded
            .execute_line("GET mode")
            .expect("GET should succeed")
            .expect("GET should return output");

        assert_eq!(value, RuntimeMessage::Continue("prod".to_string()));
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn execute_line_with_persistence_writes_and_replays_aof() {
        let aof_file = temp_file_path("commands.aof");

        let mut app = RedisLite::new();
        let _ = app
            .execute_line_with_persistence("SET name gokul", None, Some(&aof_file))
            .expect("SET with AOF should succeed");

        let mut recovered = RedisLite::new();
        let replayed = recovered
            .replay_aof(&aof_file)
            .expect("AOF replay should succeed");
        assert_eq!(replayed, 1);

        let value = recovered
            .execute_line("GET name")
            .expect("GET should succeed")
            .expect("GET should return output");
        assert_eq!(value, RuntimeMessage::Continue("gokul".to_string()));

        let _ = fs::remove_file(&aof_file);
    }

    #[test]
    fn expire_and_ttl_work() {
        let mut app = RedisLite::new();
        let _ = app
            .execute_line("SET session abc")
            .expect("SET should succeed");

        let expire = app
            .execute_line("EXPIRE session 120")
            .expect("EXPIRE should succeed")
            .expect("EXPIRE should return output");
        assert_eq!(expire, RuntimeMessage::Continue("1".to_string()));

        let ttl = app
            .execute_line("TTL session")
            .expect("TTL should succeed")
            .expect("TTL should return output");

        match ttl {
            RuntimeMessage::Continue(value) => {
                let parsed = value.parse::<u64>().expect("TTL should be integer seconds");
                assert!(parsed <= 120);
            }
            RuntimeMessage::Exit(_) => panic!("TTL should not exit"),
        }
    }

    #[test]
    fn noeviction_rejects_when_max_keys_reached() {
        let mut app = RedisLite::with_limits(Some(1), EvictionPolicy::NoEviction);
        let _ = app
            .execute_line("SET k1 v1")
            .expect("first SET should succeed");

        let result = app.execute_line("SET k2 v2");
        assert!(result.is_err());
    }

    #[test]
    fn lru_eviction_evicts_least_recently_used_key() {
        let mut app = RedisLite::with_limits(Some(2), EvictionPolicy::AllKeysLru);
        let _ = app.execute_line("SET a 1").expect("SET a should succeed");
        let _ = app.execute_line("SET b 2").expect("SET b should succeed");
        let _ = app.execute_line("GET b").expect("GET b should succeed");
        let _ = app
            .execute_line("SET c 3")
            .expect("SET c should succeed with eviction");

        let a = app
            .execute_line("GET a")
            .expect("GET a should succeed")
            .expect("GET a should return output");
        let b = app
            .execute_line("GET b")
            .expect("GET b should succeed")
            .expect("GET b should return output");

        assert_eq!(a, RuntimeMessage::Continue("(nil)".to_string()));
        assert_eq!(b, RuntimeMessage::Continue("2".to_string()));
    }

    #[test]
    fn hash_set_get_work() {
        let mut app = RedisLite::new();
        let hset = app
            .execute_line("HSET user name gokul")
            .expect("HSET should succeed")
            .expect("HSET should return output");
        assert_eq!(hset, RuntimeMessage::Continue("1".to_string()));

        let hget = app
            .execute_line("HGET user name")
            .expect("HGET should succeed")
            .expect("HGET should return output");
        assert_eq!(hget, RuntimeMessage::Continue("gokul".to_string()));
    }

    #[test]
    fn set_operations_work() {
        let mut app = RedisLite::new();
        let _ = app
            .execute_line("SADD tags rust")
            .expect("SADD should succeed");
        let members = app
            .execute_line("SMEMBERS tags")
            .expect("SMEMBERS should succeed")
            .expect("SMEMBERS should return output");
        assert_eq!(members, RuntimeMessage::Continue("rust".to_string()));
    }

    #[test]
    fn list_operations_work() {
        let mut app = RedisLite::new();
        let _ = app
            .execute_line("LPUSH jobs one")
            .expect("LPUSH one should work");
        let _ = app
            .execute_line("LPUSH jobs two")
            .expect("LPUSH two should work");

        let pop = app
            .execute_line("RPOP jobs")
            .expect("RPOP should succeed")
            .expect("RPOP should return output");
        assert_eq!(pop, RuntimeMessage::Continue("one".to_string()));
    }

    #[test]
    fn zset_operations_work() {
        let mut app = RedisLite::new();
        let _ = app
            .execute_line("ZADD scores 10 alice")
            .expect("ZADD alice should work");
        let _ = app
            .execute_line("ZADD scores 5 bob")
            .expect("ZADD bob should work");

        let range = app
            .execute_line("ZRANGE scores 0 -1")
            .expect("ZRANGE should succeed")
            .expect("ZRANGE should return output");
        assert_eq!(range, RuntimeMessage::Continue("bob\nalice".to_string()));
    }

    #[test]
    fn info_reports_runtime_metrics() {
        let mut app = RedisLite::new();
        let _ = app.execute_line("SET k v").expect("SET should succeed");
        let _ = app.execute_line("GET k").expect("GET should succeed");

        let info = app
            .execute_line("INFO")
            .expect("INFO should succeed")
            .expect("INFO should return output");

        let RuntimeMessage::Continue(payload) = info else {
            panic!("INFO should return continue message");
        };

        assert!(payload.contains("# Server"));
        assert!(payload.contains("total_commands_processed:"));
        assert!(payload.contains("write_commands_processed:"));
        assert!(payload.contains("read_commands_processed:"));
    }
}
