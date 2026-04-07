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
        "HSET" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "HSET".to_string(),
                    expected: "<key> <field> <value>",
                });
            };
            let Some(field) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "HSET".to_string(),
                    expected: "<key> <field> <value>",
                });
            };
            let value = parts.collect::<Vec<_>>().join(" ");
            if value.is_empty() {
                return Err(AppError::InvalidArgs {
                    command: "HSET".to_string(),
                    expected: "<key> <field> <value>",
                });
            }
            Ok(Command::HSet {
                key: key.to_string(),
                field: field.to_string(),
                value,
            })
        }
        "HGET" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "HGET".to_string(),
                    expected: "<key> <field>",
                });
            };
            let Some(field) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "HGET".to_string(),
                    expected: "<key> <field>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "HGET".to_string(),
                    expected: "<key> <field>",
                });
            }
            Ok(Command::HGet {
                key: key.to_string(),
                field: field.to_string(),
            })
        }
        "SADD" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "SADD".to_string(),
                    expected: "<key> <member>",
                });
            };
            let member = parts.collect::<Vec<_>>().join(" ");
            if member.is_empty() {
                return Err(AppError::InvalidArgs {
                    command: "SADD".to_string(),
                    expected: "<key> <member>",
                });
            }
            Ok(Command::SAdd {
                key: key.to_string(),
                member,
            })
        }
        "SMEMBERS" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "SMEMBERS".to_string(),
                    expected: "<key>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "SMEMBERS".to_string(),
                    expected: "<key>",
                });
            }
            Ok(Command::SMembers {
                key: key.to_string(),
            })
        }
        "LPUSH" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "LPUSH".to_string(),
                    expected: "<key> <value>",
                });
            };
            let value = parts.collect::<Vec<_>>().join(" ");
            if value.is_empty() {
                return Err(AppError::InvalidArgs {
                    command: "LPUSH".to_string(),
                    expected: "<key> <value>",
                });
            }
            Ok(Command::LPush {
                key: key.to_string(),
                value,
            })
        }
        "RPOP" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "RPOP".to_string(),
                    expected: "<key>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "RPOP".to_string(),
                    expected: "<key>",
                });
            }
            Ok(Command::RPop {
                key: key.to_string(),
            })
        }
        "ZADD" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "ZADD".to_string(),
                    expected: "<key> <score> <member>",
                });
            };
            let Some(raw_score) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "ZADD".to_string(),
                    expected: "<key> <score> <member>",
                });
            };
            let score = raw_score
                .parse::<i64>()
                .map_err(|_| AppError::InvalidArgs {
                    command: "ZADD".to_string(),
                    expected: "<key> <score> <member>",
                })?;
            let member = parts.collect::<Vec<_>>().join(" ");
            if member.is_empty() {
                return Err(AppError::InvalidArgs {
                    command: "ZADD".to_string(),
                    expected: "<key> <score> <member>",
                });
            }
            Ok(Command::ZAdd {
                key: key.to_string(),
                score,
                member,
            })
        }
        "ZRANGE" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "ZRANGE".to_string(),
                    expected: "<key> <start> <stop>",
                });
            };
            let Some(raw_start) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "ZRANGE".to_string(),
                    expected: "<key> <start> <stop>",
                });
            };
            let Some(raw_stop) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "ZRANGE".to_string(),
                    expected: "<key> <start> <stop>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "ZRANGE".to_string(),
                    expected: "<key> <start> <stop>",
                });
            }
            let start = raw_start
                .parse::<i64>()
                .map_err(|_| AppError::InvalidArgs {
                    command: "ZRANGE".to_string(),
                    expected: "<key> <start> <stop>",
                })?;
            let stop = raw_stop.parse::<i64>().map_err(|_| AppError::InvalidArgs {
                command: "ZRANGE".to_string(),
                expected: "<key> <start> <stop>",
            })?;
            Ok(Command::ZRange {
                key: key.to_string(),
                start,
                stop,
            })
        }
        "EXPIRE" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "EXPIRE".to_string(),
                    expected: "<key> <seconds>",
                });
            };
            let Some(raw_seconds) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "EXPIRE".to_string(),
                    expected: "<key> <seconds>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "EXPIRE".to_string(),
                    expected: "<key> <seconds>",
                });
            }
            let seconds = raw_seconds
                .parse::<u64>()
                .map_err(|_| AppError::InvalidArgs {
                    command: "EXPIRE".to_string(),
                    expected: "<key> <seconds>",
                })?;
            Ok(Command::Expire {
                key: key.to_string(),
                seconds,
            })
        }
        "TTL" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "TTL".to_string(),
                    expected: "<key>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "TTL".to_string(),
                    expected: "<key>",
                });
            }
            Ok(Command::Ttl {
                key: key.to_string(),
            })
        }
        "PERSIST" => {
            let Some(key) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "PERSIST".to_string(),
                    expected: "<key>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "PERSIST".to_string(),
                    expected: "<key>",
                });
            }
            Ok(Command::Persist {
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
        "SLAVEOF" => {
            let Some(host_or_no) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "SLAVEOF".to_string(),
                    expected: "<host> <port> or NO ONE",
                });
            };
            if host_or_no.to_uppercase() == "NO" {
                let Some(one) = parts.next() else {
                    return Err(AppError::InvalidArgs {
                        command: "SLAVEOF".to_string(),
                        expected: "<host> <port> or NO ONE",
                    });
                };
                if one.to_uppercase() != "ONE" {
                    return Err(AppError::InvalidArgs {
                        command: "SLAVEOF".to_string(),
                        expected: "<host> <port> or NO ONE",
                    });
                }
                if parts.next().is_some() {
                    return Err(AppError::InvalidArgs {
                        command: "SLAVEOF".to_string(),
                        expected: "<host> <port> or NO ONE",
                    });
                }
                Ok(Command::SlaveofNoOne)
            } else {
                let Some(raw_port) = parts.next() else {
                    return Err(AppError::InvalidArgs {
                        command: "SLAVEOF".to_string(),
                        expected: "<host> <port> or NO ONE",
                    });
                };
                if parts.next().is_some() {
                    return Err(AppError::InvalidArgs {
                        command: "SLAVEOF".to_string(),
                        expected: "<host> <port> or NO ONE",
                    });
                }
                let port = raw_port.parse::<u16>().map_err(|_| AppError::InvalidArgs {
                    command: "SLAVEOF".to_string(),
                    expected: "<host> <port> or NO ONE",
                })?;
                Ok(Command::Slaveof {
                    host: host_or_no.to_string(),
                    port,
                })
            }
        }
        "ROLE" => {
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "ROLE".to_string(),
                    expected: "no arguments",
                });
            }
            Ok(Command::Role)
        }
        "INFO" => {
            // Keep behavior simple and Redis-like for now: INFO or INFO <section> are accepted.
            if parts.next().is_some() && parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "INFO".to_string(),
                    expected: "no arguments or one optional section",
                });
            }
            Ok(Command::Info)
        }
        "MULTI" => {
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "MULTI".to_string(),
                    expected: "no arguments",
                });
            }
            Ok(Command::Multi)
        }
        "EXEC" => {
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "EXEC".to_string(),
                    expected: "no arguments",
                });
            }
            Ok(Command::Exec)
        }
        "DISCARD" => {
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "DISCARD".to_string(),
                    expected: "no arguments",
                });
            }
            Ok(Command::Discard)
        }
        "REPLCONF" => {
            let Some(subcommand) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "REPLCONF".to_string(),
                    expected: "<subcommand> [args...]",
                });
            };
            let args = parts.map(|s| s.to_string()).collect::<Vec<_>>();
            Ok(Command::Replconf {
                subcommand: subcommand.to_string(),
                args,
            })
        }
        "PSYNC" => {
            let Some(replication_id) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "PSYNC".to_string(),
                    expected: "<replication_id> <offset>",
                });
            };
            let Some(raw_offset) = parts.next() else {
                return Err(AppError::InvalidArgs {
                    command: "PSYNC".to_string(),
                    expected: "<replication_id> <offset>",
                });
            };
            if parts.next().is_some() {
                return Err(AppError::InvalidArgs {
                    command: "PSYNC".to_string(),
                    expected: "<replication_id> <offset>",
                });
            }
            let offset = raw_offset
                .parse::<i64>()
                .map_err(|_| AppError::InvalidArgs {
                    command: "PSYNC".to_string(),
                    expected: "<replication_id> <offset>",
                })?;
            Ok(Command::Psync {
                replication_id: replication_id.to_string(),
                offset,
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
    fn parses_expire_command() {
        let command = parse_command("EXPIRE key 30").expect("EXPIRE should parse");
        assert_eq!(
            command,
            Command::Expire {
                key: "key".to_string(),
                seconds: 30
            }
        );
    }

    #[test]
    fn parses_ttl_command() {
        let command = parse_command("TTL key").expect("TTL should parse");
        assert_eq!(
            command,
            Command::Ttl {
                key: "key".to_string()
            }
        );
    }

    #[test]
    fn parses_persist_command() {
        let command = parse_command("PERSIST key").expect("PERSIST should parse");
        assert_eq!(
            command,
            Command::Persist {
                key: "key".to_string()
            }
        );
    }

    #[test]
    fn parses_hset_command() {
        let command = parse_command("HSET user name gokul").expect("HSET should parse");
        assert_eq!(
            command,
            Command::HSet {
                key: "user".to_string(),
                field: "name".to_string(),
                value: "gokul".to_string()
            }
        );
    }

    #[test]
    fn parses_sadd_command() {
        let command = parse_command("SADD tags rust").expect("SADD should parse");
        assert_eq!(
            command,
            Command::SAdd {
                key: "tags".to_string(),
                member: "rust".to_string()
            }
        );
    }

    #[test]
    fn parses_lpush_command() {
        let command = parse_command("LPUSH jobs one").expect("LPUSH should parse");
        assert_eq!(
            command,
            Command::LPush {
                key: "jobs".to_string(),
                value: "one".to_string()
            }
        );
    }

    #[test]
    fn parses_zadd_command() {
        let command = parse_command("ZADD scores 10 alice").expect("ZADD should parse");
        assert_eq!(
            command,
            Command::ZAdd {
                key: "scores".to_string(),
                score: 10,
                member: "alice".to_string()
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

    #[test]
    fn parses_slaveof_with_host_and_port() {
        let command = parse_command("SLAVEOF localhost 6379").expect("SLAVEOF should parse");
        assert_eq!(
            command,
            Command::Slaveof {
                host: "localhost".to_string(),
                port: 6379
            }
        );
    }

    #[test]
    fn parses_slaveof_no_one() {
        let command = parse_command("SLAVEOF NO ONE").expect("SLAVEOF NO ONE should parse");
        assert_eq!(command, Command::SlaveofNoOne);
    }

    #[test]
    fn parses_role_command() {
        let command = parse_command("ROLE").expect("ROLE should parse");
        assert_eq!(command, Command::Role);
    }

    #[test]
    fn parses_info_command() {
        let command = parse_command("INFO").expect("INFO should parse");
        assert_eq!(command, Command::Info);
    }

    #[test]
    fn parses_multi_command() {
        let command = parse_command("MULTI").expect("MULTI should parse");
        assert_eq!(command, Command::Multi);
    }

    #[test]
    fn parses_exec_command() {
        let command = parse_command("EXEC").expect("EXEC should parse");
        assert_eq!(command, Command::Exec);
    }

    #[test]
    fn parses_discard_command() {
        let command = parse_command("DISCARD").expect("DISCARD should parse");
        assert_eq!(command, Command::Discard);
    }

    #[test]
    fn parses_replconf_command() {
        let command = parse_command("REPLCONF listening-port 6380").expect("REPLCONF should parse");
        assert!(matches!(command, Command::Replconf { .. }));
    }

    #[test]
    fn parses_psync_command() {
        let command = parse_command("PSYNC ? -1").expect("PSYNC should parse");
        assert_eq!(
            command,
            Command::Psync {
                replication_id: "?".to_string(),
                offset: -1
            }
        );
    }
}
