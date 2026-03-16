use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;

pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SnapshotFile {
    format_version: u32,
    data: HashMap<String, String>,
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
    use super::{backup_file, load_from_file, save_to_file};
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
}
