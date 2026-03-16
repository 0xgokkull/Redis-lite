use std::env;
use std::path::PathBuf;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub enum EvictionPolicy {
    NoEviction,
    AllKeysLru,
}

impl EvictionPolicy {
    fn parse(value: &str) -> Result<Self, AppError> {
        match value.to_ascii_lowercase().as_str() {
            "noeviction" => Ok(Self::NoEviction),
            "allkeys-lru" => Ok(Self::AllKeysLru),
            _ => Err(AppError::Config(
                "eviction policy must be one of: noeviction, allkeys-lru".to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_file: String,
    pub aof_file: String,
    pub autoload: bool,
    pub autosave: bool,
    pub appendonly: bool,
    pub max_keys: Option<usize>,
    pub eviction_policy: EvictionPolicy,
    pub requirepass: Option<String>,
    pub log_level: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PartialConfig {
    data_file: Option<String>,
    aof_file: Option<String>,
    autoload: Option<bool>,
    autosave: Option<bool>,
    appendonly: Option<bool>,
    max_keys: Option<usize>,
    eviction_policy: Option<EvictionPolicy>,
    requirepass: Option<String>,
    log_level: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_file: default_data_file(),
            aof_file: default_aof_file(),
            autoload: false,
            autosave: false,
            appendonly: false,
            max_keys: None,
            eviction_policy: EvictionPolicy::NoEviction,
            requirepass: None,
            log_level: "info".to_string(),
        }
    }
}

impl AppConfig {
    pub fn usage() -> &'static str {
        "redis-lite options:\n\
  --config <file>         Path to JSON config file\n\
  --data-file <file>      Snapshot file path (default: ~/.redis-lite/data.json)\n\
    --aof-file <file>       AOF command log path (default: ~/.redis-lite/appendonly.aof)\n\
  --autoload              Load snapshot on startup\n\
  --no-autoload           Disable startup load\n\
  --autosave              Save snapshot after mutating commands\n\
  --no-autosave           Disable autosave\n\
    --appendonly            Enable append-only command logging\n\
    --no-appendonly         Disable append-only command logging\n\
    --max-keys <n>          Maximum number of keys kept in memory\n\
    --eviction-policy <p>   Eviction policy: noeviction or allkeys-lru\n\
        --requirepass <pass>    Require AUTH before write/admin commands\n\
  --log-level <level>     Logging level text (info, debug, etc.)\n\
  --help                  Print this help\n\
\n\
Environment variables:\n\
  REDIS_LITE_CONFIG, REDIS_LITE_DATA_FILE, REDIS_LITE_AUTOLOAD,\n\
    REDIS_LITE_AOF_FILE, REDIS_LITE_AUTOSAVE, REDIS_LITE_APPENDONLY,\n\
    REDIS_LITE_MAX_KEYS, REDIS_LITE_EVICTION_POLICY, REDIS_LITE_REQUIREPASS,\n\
    REDIS_LITE_LOG_LEVEL\n"
    }

    pub fn load(args: &[String]) -> Result<Self, AppError> {
        let args_partial = parse_args(args)?;

        let config_path = args_partial
            .config_path
            .or_else(|| env::var("REDIS_LITE_CONFIG").ok());

        let file_partial = match config_path {
            Some(path) => read_config_file(&path)?,
            None => PartialConfig::default(),
        };

        let env_partial = parse_env()?;

        let mut config = AppConfig::default();
        apply_partial(&mut config, &file_partial);
        apply_partial(&mut config, &env_partial);
        apply_partial(&mut config, &args_partial.values);
        validate_config(&config)?;
        Ok(config)
    }
}

fn validate_config(config: &AppConfig) -> Result<(), AppError> {
    if config.data_file.trim().is_empty() {
        return Err(AppError::Config("data file path cannot be empty".to_string()));
    }

    if config.aof_file.trim().is_empty() {
        return Err(AppError::Config("AOF file path cannot be empty".to_string()));
    }

    if let Some(max_keys) = config.max_keys {
        if max_keys == 0 {
            return Err(AppError::Config("max-keys must be greater than 0".to_string()));
        }
    }

    if let Some(password) = &config.requirepass {
        if password.trim().is_empty() {
            return Err(AppError::Config(
                "requirepass cannot be empty or whitespace".to_string(),
            ));
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Default)]
struct ParsedArgs {
    config_path: Option<String>,
    values: PartialConfig,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, AppError> {
    let mut parsed = ParsedArgs::default();
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--help" => {
                index += 1;
            }
            "--config" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| AppError::Config("--config requires a file path".to_string()))?;
                parsed.config_path = Some(value.clone());
                index += 2;
            }
            "--data-file" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    AppError::Config("--data-file requires a file path".to_string())
                })?;
                parsed.values.data_file = Some(value.clone());
                index += 2;
            }
            "--aof-file" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    AppError::Config("--aof-file requires a file path".to_string())
                })?;
                parsed.values.aof_file = Some(value.clone());
                index += 2;
            }
            "--autoload" => {
                parsed.values.autoload = Some(true);
                index += 1;
            }
            "--no-autoload" => {
                parsed.values.autoload = Some(false);
                index += 1;
            }
            "--autosave" => {
                parsed.values.autosave = Some(true);
                index += 1;
            }
            "--no-autosave" => {
                parsed.values.autosave = Some(false);
                index += 1;
            }
            "--appendonly" => {
                parsed.values.appendonly = Some(true);
                index += 1;
            }
            "--no-appendonly" => {
                parsed.values.appendonly = Some(false);
                index += 1;
            }
            "--max-keys" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| AppError::Config("--max-keys requires a value".to_string()))?;
                let parsed_value = value.parse::<usize>().map_err(|_| {
                    AppError::Config("--max-keys must be a non-negative integer".to_string())
                })?;
                parsed.values.max_keys = Some(parsed_value);
                index += 2;
            }
            "--eviction-policy" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    AppError::Config("--eviction-policy requires a value".to_string())
                })?;
                parsed.values.eviction_policy = Some(EvictionPolicy::parse(value)?);
                index += 2;
            }
            "--requirepass" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    AppError::Config("--requirepass requires a value".to_string())
                })?;
                parsed.values.requirepass = Some(value.clone());
                index += 2;
            }
            "--log-level" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| AppError::Config("--log-level requires a value".to_string()))?;
                parsed.values.log_level = Some(value.clone());
                index += 2;
            }
            other => {
                return Err(AppError::Config(format!(
                    "unknown startup argument '{other}'"
                )));
            }
        }
    }

    Ok(parsed)
}

fn parse_env() -> Result<PartialConfig, AppError> {
    let data_file = env::var("REDIS_LITE_DATA_FILE").ok();
    let aof_file = env::var("REDIS_LITE_AOF_FILE").ok();
    let autoload = env::var("REDIS_LITE_AUTOLOAD")
        .ok()
        .map(|raw| parse_bool(&raw, "REDIS_LITE_AUTOLOAD"))
        .transpose()?;
    let autosave = env::var("REDIS_LITE_AUTOSAVE")
        .ok()
        .map(|raw| parse_bool(&raw, "REDIS_LITE_AUTOSAVE"))
        .transpose()?;
    let appendonly = env::var("REDIS_LITE_APPENDONLY")
        .ok()
        .map(|raw| parse_bool(&raw, "REDIS_LITE_APPENDONLY"))
        .transpose()?;
    let max_keys = env::var("REDIS_LITE_MAX_KEYS")
        .ok()
        .map(|raw| {
            raw.parse::<usize>().map_err(|_| {
                AppError::Config("REDIS_LITE_MAX_KEYS must be a non-negative integer".to_string())
            })
        })
        .transpose()?;
    let eviction_policy = env::var("REDIS_LITE_EVICTION_POLICY")
        .ok()
        .map(|raw| EvictionPolicy::parse(&raw))
        .transpose()?;
    let requirepass = env::var("REDIS_LITE_REQUIREPASS").ok();
    let log_level = env::var("REDIS_LITE_LOG_LEVEL").ok();

    Ok(PartialConfig {
        data_file,
        aof_file,
        autoload,
        autosave,
        appendonly,
        max_keys,
        eviction_policy,
        requirepass,
        log_level,
    })
}

fn read_config_file(path: &str) -> Result<PartialConfig, AppError> {
    let raw = std::fs::read_to_string(path)?;
    serde_json::from_str::<PartialConfig>(&raw)
        .map_err(|e| AppError::Config(format!("invalid config file '{path}': {e}")))
}

fn apply_partial(config: &mut AppConfig, partial: &PartialConfig) {
    if let Some(value) = &partial.data_file {
        config.data_file = value.clone();
    }
    if let Some(value) = &partial.aof_file {
        config.aof_file = value.clone();
    }
    if let Some(value) = partial.autoload {
        config.autoload = value;
    }
    if let Some(value) = partial.autosave {
        config.autosave = value;
    }
    if let Some(value) = partial.appendonly {
        config.appendonly = value;
    }
    if let Some(value) = partial.max_keys {
        config.max_keys = Some(value);
    }
    if let Some(value) = partial.eviction_policy {
        config.eviction_policy = value;
    }
    if let Some(value) = &partial.requirepass {
        config.requirepass = Some(value.clone());
    }
    if let Some(value) = &partial.log_level {
        config.log_level = value.clone();
    }
}

fn parse_bool(raw: &str, name: &str) -> Result<bool, AppError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AppError::Config(format!(
            "{name} must be one of: true/false, 1/0, yes/no, on/off"
        ))),
    }
}

fn default_data_file() -> String {
    if let Ok(home) = env::var("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".redis-lite");
        path.push("data.json");
        return path.to_string_lossy().to_string();
    }

    "./redis-lite-data.json".to_string()
}

fn default_aof_file() -> String {
    if let Ok(home) = env::var("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".redis-lite");
        path.push("appendonly.aof");
        return path.to_string_lossy().to_string();
    }

    "./redis-lite-appendonly.aof".to_string()
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, EvictionPolicy};

    #[test]
    fn args_override_default_values() {
        let args = vec![
            "redis-lite".to_string(),
            "--data-file".to_string(),
            "./custom.json".to_string(),
            "--aof-file".to_string(),
            "./custom.aof".to_string(),
            "--autoload".to_string(),
            "--autosave".to_string(),
            "--appendonly".to_string(),
            "--max-keys".to_string(),
            "10".to_string(),
            "--eviction-policy".to_string(),
            "allkeys-lru".to_string(),
            "--requirepass".to_string(),
            "secret".to_string(),
            "--log-level".to_string(),
            "debug".to_string(),
        ];

        let config = AppConfig::load(&args).expect("config should parse");
        assert_eq!(config.data_file, "./custom.json");
        assert_eq!(config.aof_file, "./custom.aof");
        assert!(config.autoload);
        assert!(config.autosave);
        assert!(config.appendonly);
        assert_eq!(config.max_keys, Some(10));
        assert_eq!(config.eviction_policy, EvictionPolicy::AllKeysLru);
        assert_eq!(config.requirepass, Some("secret".to_string()));
        assert_eq!(config.log_level, "debug");
    }

    #[test]
    fn rejects_zero_max_keys() {
        let args = vec![
            "redis-lite".to_string(),
            "--max-keys".to_string(),
            "0".to_string(),
        ];

        let result = AppConfig::load(&args);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_empty_requirepass() {
        let args = vec![
            "redis-lite".to_string(),
            "--requirepass".to_string(),
            "   ".to_string(),
        ];

        let result = AppConfig::load(&args);
        assert!(result.is_err());
    }
}
