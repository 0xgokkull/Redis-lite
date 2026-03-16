use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::command::Command;
use crate::error::AppError;

pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;
pub const AOF_FORMAT_VERSION: u32 = 1;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SnapshotFile {
    format_version: u32,
    data: HashMap<String, String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct AofEntry {
    format_version: u32,
    command: Command,
}

pub fn save_to_file(file: &str, data: &HashMap<String, String>) -> Result<(), AppError> {
    let snapshot = SnapshotFile {
        format_version: SNAPSHOT_FORMAT_VERSION,
        data: data.clone(),
    };
    let json = serde_json::to_string_pretty(&snapshot)?;
    atomic_write_json(file, &json)?;
    Ok(())
}

pub fn load_from_file(file: &str) -> Result<HashMap<String, String>, AppError> {
    let raw = fs::read_to_string(file)?;

    // Backward compatibility: old files were plain HashMap<String, String>.
    if let Ok(legacy) = serde_json::from_str::<HashMap<String, String>>(&raw) {
        return Ok(legacy);
    }

    let snapshot = serde_json::from_str::<SnapshotFile>(&raw)?;
    if snapshot.format_version != SNAPSHOT_FORMAT_VERSION {
        return Err(AppError::FormatVersion(format!(
            "unsupported snapshot format version {}; expected {}",
            snapshot.format_version, SNAPSHOT_FORMAT_VERSION
        )));
    }

    Ok(snapshot.data)
}

pub fn backup_file(from_file: &str, to_file: &str) -> Result<(), AppError> {
    let data = load_from_file(from_file)?;
    save_to_file(to_file, &data)
}

pub fn append_aof_command(file: &str, command: &Command) -> Result<(), AppError> {
    let target_path = Path::new(file);
    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let entry = AofEntry {
        format_version: AOF_FORMAT_VERSION,
        command: command.clone(),
    };
    let encoded = serde_json::to_string(&entry)?;

    let mut handle = File::options()
        .create(true)
        .append(true)
        .open(target_path)
        .map_err(AppError::Io)?;

    handle.write_all(encoded.as_bytes())?;
    handle.write_all(b"\n")?;
    handle.sync_all()?;
    Ok(())
}

pub fn load_aof_commands(file: &str) -> Result<Vec<Command>, AppError> {
    let handle = File::open(file)?;
    let reader = BufReader::new(handle);
    let mut commands = Vec::new();

    for (line_number, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }

        let entry = serde_json::from_str::<AofEntry>(&line).map_err(|error| {
            AppError::Config(format!(
                "invalid AOF entry at line {} in '{}': {}",
                line_number + 1,
                file,
                error
            ))
        })?;

        if entry.format_version != AOF_FORMAT_VERSION {
            return Err(AppError::FormatVersion(format!(
                "unsupported AOF format version {}; expected {}",
                entry.format_version, AOF_FORMAT_VERSION
            )));
        }

        commands.push(entry.command);
    }

    Ok(commands)
}

fn atomic_write_json(file: &str, json: &str) -> Result<(), AppError> {
    let target_path = Path::new(file);
    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let temp_path = temp_path_for_target(target_path)?;
    let mut temp_file = match File::options()
        .create_new(true)
        .write(true)
        .open(&temp_path)
    {
        Ok(file) => file,
        Err(e) => return Err(AppError::Io(e)),
    };

    if let Err(e) = write_and_sync(&mut temp_file, json) {
        let _ = fs::remove_file(&temp_path);
        return Err(e);
    }

    if let Err(e) = fs::rename(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(AppError::Io(e));
    }

    Ok(())
}

fn write_and_sync(file: &mut File, json: &str) -> Result<(), AppError> {
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn temp_path_for_target(target_path: &Path) -> Result<PathBuf, AppError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::Config(format!("system clock error: {e}")))?
        .as_nanos();

    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Config("invalid target file path".to_string()))?;

    let temp_name = format!(".{file_name}.tmp-{nanos}");
    let mut temp_path = target_path.to_path_buf();
    temp_path.set_file_name(temp_name);
    Ok(temp_path)
}

#[cfg(test)]
mod tests {
    use super::{
        append_aof_command, backup_file, load_aof_commands, load_from_file, save_to_file,
        AOF_FORMAT_VERSION,
    };
    use crate::command::Command;
    use std::collections::HashMap;
    use std::fs;

    fn temp_file_path(suffix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("redis_lite_{nanos}_{suffix}.json"));
        path.to_string_lossy().to_string()
    }

    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn saves_populated_store() {
        let file = temp_file_path("save");
        let mut data = HashMap::new();
        data.insert("theme".to_string(), "dark".to_string());

        save_to_file(&file, &data).expect("save should succeed");
        let saved = fs::read_to_string(&file).expect("file should exist after save");

        assert!(saved.contains("format_version"));
        assert!(saved.contains("theme"));
        assert!(saved.contains("dark"));

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn loads_valid_json_file() {
        let file = temp_file_path("load");
        fs::write(&file, r#"{"format_version":1,"data":{"session":"abc123"}}"#)
            .expect("should write fixture file");

        let loaded = load_from_file(&file).expect("load should succeed");
        assert_eq!(loaded.get("session"), Some(&"abc123".to_string()));

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn rejects_invalid_json() {
        let file = temp_file_path("invalid");
        fs::write(&file, "{ not valid json }").expect("should write invalid fixture");

        let result = load_from_file(&file);
        assert!(result.is_err());

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn round_trip_save_and_load_matches_data() {
        let file = temp_file_path("roundtrip");
        let mut original = HashMap::new();
        original.insert("theme".to_string(), "dark".to_string());
        original.insert("timeout".to_string(), "30".to_string());

        save_to_file(&file, &original).expect("save should succeed");
        let loaded = load_from_file(&file).expect("load should succeed");

        assert_eq!(loaded, original);

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn loads_legacy_plain_hashmap_snapshot() {
        let file = temp_file_path("legacy");
        fs::write(&file, r#"{"old":"format"}"#).expect("should write legacy fixture");

        let loaded = load_from_file(&file).expect("legacy format should load");
        assert_eq!(loaded.get("old"), Some(&"format".to_string()));

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn backup_copies_data_between_files() {
        let source = temp_file_path("source");
        let backup = temp_file_path("backup");

        let mut data = HashMap::new();
        data.insert("k".to_string(), "v".to_string());
        save_to_file(&source, &data).expect("source save should succeed");

        backup_file(&source, &backup).expect("backup should succeed");
        let loaded = load_from_file(&backup).expect("backup load should succeed");
        assert_eq!(loaded.get("k"), Some(&"v".to_string()));

        let _ = fs::remove_file(&source);
        let _ = fs::remove_file(&backup);
    }

    #[test]
    fn appends_and_loads_aof_commands() {
        let file = temp_file_path("aof");

        append_aof_command(
            &file,
            &Command::Set {
                key: "name".to_string(),
                value: "gokul".to_string(),
            },
        )
        .expect("append SET should succeed");
        append_aof_command(
            &file,
            &Command::Delete {
                key: "name".to_string(),
            },
        )
        .expect("append DELETE should succeed");

        let commands = load_aof_commands(&file).expect("AOF load should succeed");
        assert_eq!(commands.len(), 2);
        assert!(matches!(commands[0], Command::Set { .. }));
        assert!(matches!(commands[1], Command::Delete { .. }));

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn rejects_unsupported_aof_version() {
        let file = temp_file_path("aof_bad_version");
        let bad_entry = format!(
            "{{\"format_version\":{},\"command\":{{\"Set\":{{\"key\":\"k\",\"value\":\"v\"}}}}}}\n",
            AOF_FORMAT_VERSION + 1
        );
        fs::write(&file, bad_entry).expect("bad AOF should be written");

        let result = load_aof_commands(&file);
        assert!(result.is_err());

        let _ = fs::remove_file(&file);
    }
}
