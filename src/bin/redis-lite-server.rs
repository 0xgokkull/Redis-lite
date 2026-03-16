use redis_lite::config::AppConfig;
use redis_lite::server::{run_server, ServerOptions};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help") {
        println!(
            "redis-lite-server options:\n\
  --bind <addr>           TCP bind address (default: 127.0.0.1:6379)\n\
  --config <file>         Path to JSON config file\n\
  --data-file <file>      Snapshot file path\n\
  --autoload              Load snapshot on startup\n\
  --autosave              Save snapshot after mutating commands\n\
  --help                  Print this help\n"
        );
        return Ok(());
    }

    let bind_addr = parse_bind_addr(&args).unwrap_or_else(|| "127.0.0.1:6379".to_string());
    let config_args = args_without_bind(&args);
    let config = AppConfig::load(&config_args)?;

    let options = ServerOptions {
        bind_addr,
        data_file: config.data_file,
        autoload: config.autoload,
        autosave: config.autosave,
    };

    run_server(options).await?;
    Ok(())
}

fn parse_bind_addr(args: &[String]) -> Option<String> {
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--bind" {
            return args.get(index + 1).cloned();
        }
        index += 1;
    }
    None
}

fn args_without_bind(args: &[String]) -> Vec<String> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--bind" {
            index += 2;
            continue;
        }

        filtered.push(args[index].clone());
        index += 1;
    }
    filtered
}
