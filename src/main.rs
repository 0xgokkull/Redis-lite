use std::io::{self, Write};

use redis_lite::config::AppConfig;
use redis_lite::{RedisLite, RuntimeMessage};

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
    let mut app = RedisLite::new();

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
    }

    let stdin = io::stdin();
    let mut input = String::new();

    println!(
        "redis-lite ready. Type HELP for commands. (autosave: {}, data file: {})",
        config.autosave, config.data_file
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

        match app.execute_line_with_autosave(&input, autosave_target) {
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
