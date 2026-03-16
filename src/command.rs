#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Set { key: String, value: String },
    Get { key: String },
    Delete { key: String },
    Save { file: String },
    Load { file: String },
    Backup { file: String },
    Restore { file: String },
    List,
    Help,
    Exit,
}

pub const HELP_TEXT: &str = "\
Available commands:\n\
  SET <key> <value>   Insert or overwrite a key with a value\n\
  GET <key>           Read the value for a key\n\
  DELETE <key>        Remove a key\n\
  SAVE <file>         Persist the full store to a JSON file\n\
  LOAD <file>         Replace in-memory state from a JSON file\n\
  BACKUP <file>       Create a snapshot backup file\n\
  RESTORE <file>      Restore in-memory state from a backup\n\
  LIST                Show all stored entries\n\
  HELP                Show this help\n\
  EXIT                Quit the application\n";
