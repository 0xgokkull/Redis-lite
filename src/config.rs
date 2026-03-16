use std::env;
use std::path::PathBuf;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_file: String,
    pub autoload: bool,
    pub autosave: bool,
    pub log_level: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PartialConfig {
    data_file: Option<String>,
    autoload: Option<bool>,
    autosave: Option<bool>,
    log_level: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_file: default_data_file(),
            autoload: false,
            autosave: false,
            log_level: "info".to_string(),
        }
    }
}

impl AppConfig {
    pub fn usage() -> &'static str {
        "redis-lite options:\n\
  --config <file>         Path to JSON config file\n\
  --data-file <file>      Snapshot file path (default: ~/.redis-lite/data.json)\n\
  --autoload              Load snapshot on startup\n\
  --no-autoload           Disable startup load\n\
  --autosave              Save snapshot after mutating commands\n\
  --no-autosave           Disable autosave\n\
  --log-level <level>     Logging level text (info, debug, etc.)\n\
  --help                  Print this help\n\
\n\
Environment variables:\n\
  REDIS_LITE_CONFIG, REDIS_LITE_DATA_FILE, REDIS_LITE_AUTOLOAD,\n\
  REDIS_LITE_AUTOSAVE, REDIS_LITE_LOG_LEVEL\n"
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
        Ok(config)
    }
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
    let autoload = env::var("REDIS_LITE_AUTOLOAD")
        .ok()
        .map(|raw| parse_bool(&raw, "REDIS_LITE_AUTOLOAD"))
        .transpose()?;
    let autosave = env::var("REDIS_LITE_AUTOSAVE")
        .ok()
        .map(|raw| parse_bool(&raw, "REDIS_LITE_AUTOSAVE"))
        .transpose()?;
    let log_level = env::var("REDIS_LITE_LOG_LEVEL").ok();

    Ok(PartialConfig {
        data_file,
        autoload,
        autosave,
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
    if let Some(value) = partial.autoload {
        config.autoload = value;
    }
    if let Some(value) = partial.autosave {
        config.autosave = value;
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

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn args_override_default_values() {
        let args = vec![
            "redis-lite".to_string(),
            "--data-file".to_string(),
            "./custom.json".to_string(),
            "--autoload".to_string(),
            "--autosave".to_string(),
            "--log-level".to_string(),
            "debug".to_string(),
        ];

        let config = AppConfig::load(&args).expect("config should parse");
        assert_eq!(config.data_file, "./custom.json");
        assert!(config.autoload);
        assert!(config.autosave);
        assert_eq!(config.log_level, "debug");
    }
}
