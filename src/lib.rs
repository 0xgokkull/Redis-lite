pub mod command;
pub mod config;
pub mod error;
pub mod parser;
pub mod persistence;
pub mod server;
pub mod store;

use command::{Command, HELP_TEXT};
use error::AppError;
use parser::parse_command;
use persistence::{load_from_file, save_to_file};
use store::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMessage {
    Continue(String),
    Exit(String),
}

pub struct RedisLite {
    store: Store,
}

impl Default for RedisLite {
    fn default() -> Self {
        Self::new()
    }
}

impl RedisLite {
    pub fn new() -> Self {
        Self {
            store: Store::new(),
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
        let command = match parse_command(input) {
            Ok(command) => command,
            Err(AppError::EmptyInput) => return Ok(None),
            Err(error) => return Err(error),
        };

        let mutates_state = command_mutates_state(&command);
        let message = self.execute_command(command)?;

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
        Ok(())
    }

    pub fn execute_command(&mut self, command: Command) -> Result<RuntimeMessage, AppError> {
        match command {
            Command::Set { key, value } => {
                self.store.set(key, value);
                Ok(RuntimeMessage::Continue("OK".to_string()))
            }
            Command::Get { key } => Ok(RuntimeMessage::Continue(
                self.store
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| "(nil)".to_string()),
            )),
            Command::Delete { key } => {
                let message = if self.store.delete(&key) {
                    format!("deleted '{key}'")
                } else {
                    format!("key '{key}' not found")
                };
                Ok(RuntimeMessage::Continue(message))
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
            Command::List => Ok(RuntimeMessage::Continue(self.format_entries())),
            Command::Help => Ok(RuntimeMessage::Continue(HELP_TEXT.to_string())),
            Command::Exit => Ok(RuntimeMessage::Exit("Exiting redis-lite.".to_string())),
        }
    }

    fn format_entries(&self) -> String {
        let entries = self.store.list();
        if entries.is_empty() {
            return "(empty)".to_string();
        }

        entries
            .into_iter()
            .map(|(key, value)| format!("{key} = {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn command_mutates_state(command: &Command) -> bool {
    matches!(
        command,
        Command::Set { .. }
            | Command::Delete { .. }
            | Command::Load { .. }
            | Command::Restore { .. }
    )
}

#[cfg(test)]
mod tests {
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
}
