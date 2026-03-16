use crate::command::Command;
use crate::error::AppError;

pub fn parse_command(input: &str) -> Result<Command, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::EmptyInput);
    }

    let mut parts = trimmed.split_whitespace();
    let Some(raw_cmd) = parts.next() else {
        return Err(AppError::EmptyInput);
    };

    let cmd = raw_cmd.to_ascii_uppercase();
    match cmd.as_str() {
        "SET" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "SET".to_string(),
                    expected: "<key> <value>",
                });
            };
            let value = parts.collect::<Vec<_>>().join(" ");
            if value.is_empty() {
                return Err(AppError::InvalidArgs {
                    command: "SET".to_string(),
                    expected: "<key> <value>",
                });
            }
            Ok(Command::Set {
                key: key.to_string(),
                value,
            })
        }
        "GET" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "GET".to_string(),
                    expected: "<key>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "GET".to_string(),
                    expected: "<key>",
                });
            }
            Ok(Command::Get {
                key: key.to_string(),
            })
        }
        "DELETE" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "DELETE".to_string(),
                    expected: "<key>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "DELETE".to_string(),
                    expected: "<key>",
                });
            }
            Ok(Command::Delete {
                key: key.to_string(),
            })
        }
        "SAVE" => {
            let Some(file) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "SAVE".to_string(),
                    expected: "<file>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "SAVE".to_string(),
                    expected: "<file>",
                });
            }
            Ok(Command::Save {
                file: file.to_string(),
            })
        }
        "LOAD" => {
            let Some(file) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "LOAD".to_string(),
                    expected: "<file>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "LOAD".to_string(),
                    expected: "<file>",
                });
            }
            Ok(Command::Load {
                file: file.to_string(),
            })
        }
        "BACKUP" => {
            let Some(file) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "BACKUP".to_string(),
                    expected: "<file>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "BACKUP".to_string(),
                    expected: "<file>",
                });
            }
            Ok(Command::Backup {
                file: file.to_string(),
            })
        }
        "RESTORE" => {
            let Some(file) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "RESTORE".to_string(),
                    expected: "<file>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "RESTORE".to_string(),
                    expected: "<file>",
                });
            }
            Ok(Command::Restore {
                file: file.to_string(),
            })
        }
        "LIST" => {
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "LIST".to_string(),
                    expected: "no arguments",
                });
            }
            Ok(Command::List)
        }
        "HELP" => {
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "HELP".to_string(),
                    expected: "no arguments",
                });
            }
            Ok(Command::Help)
        }
        "EXIT" => {
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "EXIT".to_string(),
                    expected: "no arguments",
                });
            }
            Ok(Command::Exit)
        }
        _ => Err(AppError::UnknownCommand(raw_cmd.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_command;
    use crate::command::Command;
    use crate::error::AppError;

    #[test]
    fn parses_set_with_single_word_value() {
        let command = parse_command("SET name gokul").expect("SET should parse");
        assert_eq!(
            command,
            Command::Set {
                key: "name".to_string(),
                value: "gokul".to_string()
            }
        );
    }

    #[test]
    fn parses_set_with_spaces_in_value() {
        let command = parse_command("SET bio rust developer").expect("SET should parse");
        assert_eq!(
            command,
            Command::Set {
                key: "bio".to_string(),
                value: "rust developer".to_string()
            }
        );
    }

    #[test]
    fn parses_get_command() {
        let command = parse_command("GET name").expect("GET should parse");
        assert_eq!(
            command,
            Command::Get {
                key: "name".to_string()
            }
        );
    }

    #[test]
    fn parses_backup_command() {
        let command = parse_command("BACKUP backup.json").expect("BACKUP should parse");
        assert_eq!(
            command,
            Command::Backup {
                file: "backup.json".to_string()
            }
        );
    }

    #[test]
    fn parses_restore_command() {
        let command = parse_command("RESTORE backup.json").expect("RESTORE should parse");
        assert_eq!(
            command,
            Command::Restore {
                file: "backup.json".to_string()
            }
        );
    }

    #[test]
    fn parses_case_insensitive_command() {
        let command = parse_command("help").expect("HELP should parse");
        assert_eq!(command, Command::Help);
    }

    #[test]
    fn rejects_empty_input() {
        let error = parse_command("   ").expect_err("empty input should fail");
        assert!(matches!(error, AppError::EmptyInput));
    }

    #[test]
    fn rejects_unknown_command() {
        let error = parse_command("NOPE key").expect_err("unknown command should fail");
        assert!(matches!(error, AppError::UnknownCommand(_)));
    }

    #[test]
    fn rejects_set_without_value() {
        let error = parse_command("SET username").expect_err("SET missing value should fail");
        assert!(matches!(error, AppError::InvalidArgs { .. }));
    }
}
