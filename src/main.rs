use std::io::{self, Write};

use redis_lite::command::Command;
use redis_lite::config::AppConfig;
use redis_lite::parser::parse_command;
use redis_lite::{RedisLite, RuntimeMessage};

#[derive(Default)]
struct ReplTransactionState {
    queued: Vec<String>,
}

fn main() {
    if let Err(error) = run_repl() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run_repl() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help") {
        println!("{}", AppConfig::usage());
        return Ok(());
    }

    let config = AppConfig::load(&args)?;
    let mut app = RedisLite::with_limits(config.max_keys, config.eviction_policy);

    if config.autoload {
        match app.load_from_path(&config.data_file) {
            Ok(()) => println!("autoloaded data from {}", config.data_file),
            Err(redis_lite::error::AppError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "autoload skipped: snapshot file not found at {}",
                    config.data_file
                );
            }
            Err(error) => return Err(Box::new(error)),
        }

        if config.appendonly {
            match app.replay_aof(&config.aof_file) {
                Ok(count) => println!("replayed {count} AOF commands from {}", config.aof_file),
                Err(redis_lite::error::AppError::Io(e))
                    if e.kind() == std::io::ErrorKind::NotFound =>
                {
                    eprintln!("AOF replay skipped: file not found at {}", config.aof_file);
                }
                Err(error) => return Err(Box::new(error)),
            }
        }
    }

    let stdin = io::stdin();
    let mut input = String::new();
    let mut transaction = None::<ReplTransactionState>;

    println!(
        "redis-lite ready. Type HELP for commands. (autosave: {}, appendonly: {}, max_keys: {:?}, eviction: {:?}, data file: {}, aof file: {})",
        config.autosave,
        config.appendonly,
        config.max_keys,
        config.eviction_policy,
        config.data_file,
        config.aof_file
    );

    loop {
        print!("redis-lite> ");
        io::stdout().flush()?;

        input.clear();
        let bytes_read = stdin.read_line(&mut input)?;
        if bytes_read == 0 {
            println!("Exiting redis-lite.");
            break;
        }

        let autosave_target = if config.autosave {
            Some(config.data_file.as_str())
        } else {
            None
        };

        let aof_target = if config.appendonly {
            Some(config.aof_file.as_str())
        } else {
            None
        };

        let trimmed = input.trim();
        match parse_command(trimmed) {
            Ok(Command::Multi) => {
                if transaction.is_some() {
                    eprintln!("error: MULTI calls cannot be nested");
                } else {
                    transaction = Some(ReplTransactionState::default());
                    println!("OK");
                }
                continue;
            }
            Ok(Command::Discard) => {
                if transaction.take().is_some() {
                    println!("OK");
                } else {
                    eprintln!("error: DISCARD without active MULTI");
                }
                continue;
            }
            Ok(Command::Exec) => {
                let Some(mut queued) = transaction.take() else {
                    eprintln!("error: EXEC without active MULTI");
                    continue;
                };

                if queued.queued.is_empty() {
                    println!("(empty)");
                    continue;
                }

                for line in queued.queued.drain(..) {
                    match app.execute_line_with_persistence(&line, autosave_target, aof_target) {
                        Ok(Some(RuntimeMessage::Continue(message))) => println!("{message}"),
                        Ok(Some(RuntimeMessage::Exit(message))) => {
                            println!("{message}");
                            return Ok(());
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("{error}"),
                    }
                }
                continue;
            }
            Ok(command) => {
                if let Some(state) = &mut transaction {
                    if matches!(command, Command::Exit) {
                        eprintln!("error: EXIT cannot be queued inside MULTI");
                        continue;
                    }
                    state.queued.push(trimmed.to_string());
                    println!("QUEUED");
                    continue;
                }
            }
            Err(redis_lite::error::AppError::EmptyInput) => continue,
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        }

        match app.execute_line_with_persistence(&input, autosave_target, aof_target) {
            Ok(Some(RuntimeMessage::Continue(message))) => println!("{message}"),
            Ok(Some(RuntimeMessage::Exit(message))) => {
                println!("{message}");
                break;
            }
            Ok(None) => continue,
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        }
    }

    Ok(())
}
