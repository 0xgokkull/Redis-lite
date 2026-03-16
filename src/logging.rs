use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Info,
    Debug,
}

impl LogLevel {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            _ => Err(AppError::Config(
                "log-level must be one of: error, info, debug".to_string(),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }

    pub fn enabled_for(self, event_level: Self) -> bool {
        self >= event_level
    }

    pub fn log(self, event_level: Self, message: &str) {
        if self.enabled_for(event_level) {
            eprintln!("[{}] {message}", event_level.as_str());
        }
    }
}
