use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::config::EvictionPolicy;
use crate::error::AppError;
use crate::RedisLite;

#[derive(Debug, Default)]
struct ServerMetrics {
    started_at_secs: u64,
    current_connections: u64,
    total_connections_received: u64,
}

impl ServerMetrics {
    fn new() -> Self {
        Self {
            started_at_secs: now_secs(),
            current_connections: 0,
            total_connections_received: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub bind_addr: String,
    pub data_file: String,
    pub aof_file: String,
    pub autoload: bool,
    pub autosave: bool,
    pub appendonly: bool,
    pub max_keys: Option<usize>,
    pub eviction_policy: EvictionPolicy,
    pub requirepass: Option<String>,
}

pub async fn run_server(options: ServerOptions) -> Result<(), AppError> {
    let listener = TcpListener::bind(&options.bind_addr)
        .await
        .map_err(|e| AppError::Config(format!("failed to bind {}: {e}", options.bind_addr)))?;

    let mut app = RedisLite::with_limits(options.max_keys, options.eviction_policy);
    if options.autoload {
        match app.load_from_path(&options.data_file) {
            Ok(()) => {
                println!("autoloaded data from {}", options.data_file);
            }
            Err(AppError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "autoload skipped: snapshot file not found at {}",
                    options.data_file
                );
            }
            Err(error) => return Err(error),
        }

        if options.appendonly {
            match app.replay_aof(&options.aof_file) {
                Ok(count) => println!("replayed {count} AOF commands from {}", options.aof_file),
                Err(AppError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("AOF replay skipped: file not found at {}", options.aof_file);
                }
                Err(error) => return Err(error),
            }
        }
    }

    let shared = Arc::new(Mutex::new(app));
    let metrics = Arc::new(Mutex::new(ServerMetrics::new()));

    println!("redis-lite RESP server listening on {}", options.bind_addr);

    loop {
        let (stream, peer) = listener
            .accept()
            .await
            .map_err(|e| AppError::Config(format!("failed to accept connection: {e}")))?;

        let state = Arc::clone(&shared);
        let metrics_clone = Arc::clone(&metrics);
        let options_clone = options.clone();

        {
            let mut m = metrics.lock().await;
            m.total_connections_received = m.total_connections_received.saturating_add(1);
            m.current_connections = m.current_connections.saturating_add(1);
        }

        tokio::spawn(async move {
            if let Err(error) = handle_client(stream, state, options_clone, Arc::clone(&metrics_clone)).await {
                eprintln!("client {peer} error: {error}");
            }

            let mut m = metrics_clone.lock().await;
            m.current_connections = m.current_connections.saturating_sub(1);
        });
    }
}

async fn handle_client(
    stream: TcpStream,
    state: Arc<Mutex<RedisLite>>,
    options: ServerOptions,
    metrics: Arc<Mutex<ServerMetrics>>,
) -> Result<(), AppError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut authenticated = options.requirepass.is_none();

    loop {
        let command = match read_resp_command(&mut reader).await {
            Ok(Some(command)) => command,
            Ok(None) => return Ok(()),
            Err(error) => {
                write_half
                    .write_all(resp_error(&error).as_bytes())
                    .await
                    .map_err(AppError::Io)?;
                continue;
            }
        };

        let response = execute_resp(&state, &options, &metrics, &command, &mut authenticated).await;

        if let Err(write_error) = write_half.write_all(response.as_bytes()).await {
            return Err(AppError::Io(write_error));
        }

        if command
            .first()
            .map(|c| c.eq_ignore_ascii_case("QUIT"))
            .unwrap_or(false)
        {
            return Ok(());
        }
    }
}

async fn execute_resp(
    state: &Arc<Mutex<RedisLite>>,
    options: &ServerOptions,
    metrics: &Arc<Mutex<ServerMetrics>>,
    command: &[String],
    authenticated: &mut bool,
) -> String {
    if command.is_empty() {
        return resp_error("ERR empty command");
    }

    let verb = command[0].to_ascii_uppercase();

    if options.requirepass.is_some() && !*authenticated && !is_auth_exempt_command(&verb) {
        return resp_error("NOAUTH Authentication required");
    }

    match verb.as_str() {
        "AUTH" => {
            if command.len() != 2 {
                return resp_error("ERR wrong number of arguments for 'AUTH'");
            }

            match &options.requirepass {
                Some(password) if password == &command[1] => {
                    *authenticated = true;
                    "+OK\r\n".to_string()
                }
                Some(_) => resp_error("WRONGPASS invalid password"),
                None => "+OK\r\n".to_string(),
            }
        }
        "PING" => {
            if command.len() > 2 {
                return resp_error("ERR wrong number of arguments for 'PING'");
            }
            if let Some(message) = command.get(1) {
                resp_bulk(Some(message))
            } else {
                "+PONG\r\n".to_string()
            }
        }
        "ECHO" => {
            if command.len() != 2 {
                return resp_error("ERR wrong number of arguments for 'ECHO'");
            }
            resp_bulk(command.get(1).map(|value| value.as_str()))
        }
        "SET" => {
            if command.len() != 3 {
                return resp_error("ERR wrong number of arguments for 'SET'");
            }
            let line = format!("SET {} {}", command[1], command[2]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(_) => "+OK\r\n".to_string(),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "GET" => {
            if command.len() != 2 {
                return resp_error("ERR wrong number of arguments for 'GET'");
            }
            let line = format!("GET {}", command[1]);
            let mut db = state.lock().await;
            match db.execute_line(&line) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    if value == "(nil)" {
                        resp_bulk(None)
                    } else {
                        resp_bulk(Some(&value))
                    }
                }
                Ok(_) => resp_bulk(None),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "DEL" => {
            if command.len() < 2 {
                return resp_error("ERR wrong number of arguments for 'DEL'");
            }
            let mut deleted_count = 0;
            let mut db = state.lock().await;

            for key in &command[1..] {
                let line = format!("DELETE {key}");
                match db.execute_line_with_persistence(
                    &line,
                    if options.autosave {
                        Some(options.data_file.as_str())
                    } else {
                        None
                    },
                    if options.appendonly {
                        Some(options.aof_file.as_str())
                    } else {
                        None
                    },
                ) {
                    Ok(Some(crate::RuntimeMessage::Continue(message))) => {
                        if message.starts_with("deleted") {
                            deleted_count += 1;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => return resp_error(&format!("ERR {error}")),
                }
            }

            resp_integer(deleted_count)
        }
        "HSET" => {
            if command.len() != 4 {
                return resp_error("ERR wrong number of arguments for 'HSET'");
            }
            let line = format!("HSET {} {} {}", command[1], command[2], command[3]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    resp_integer(value.parse::<i64>().unwrap_or(0))
                }
                Ok(_) => resp_integer(0),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "HGET" => {
            if command.len() != 3 {
                return resp_error("ERR wrong number of arguments for 'HGET'");
            }
            let line = format!("HGET {} {}", command[1], command[2]);
            let mut db = state.lock().await;
            match db.execute_line(&line) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    if value == "(nil)" {
                        resp_bulk(None)
                    } else {
                        resp_bulk(Some(&value))
                    }
                }
                Ok(_) => resp_bulk(None),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "SADD" => {
            if command.len() != 3 {
                return resp_error("ERR wrong number of arguments for 'SADD'");
            }
            let line = format!("SADD {} {}", command[1], command[2]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    resp_integer(value.parse::<i64>().unwrap_or(0))
                }
                Ok(_) => resp_integer(0),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "SMEMBERS" => {
            if command.len() != 2 {
                return resp_error("ERR wrong number of arguments for 'SMEMBERS'");
            }
            let line = format!("SMEMBERS {}", command[1]);
            let mut db = state.lock().await;
            match db.execute_line(&line) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    if value == "(empty)" {
                        "*0\r\n".to_string()
                    } else {
                        let items = value.split('\n').collect::<Vec<_>>();
                        let mut response = format!("*{}\r\n", items.len());
                        for item in items {
                            response.push_str(&resp_bulk(Some(item)));
                        }
                        response
                    }
                }
                Ok(_) => "*0\r\n".to_string(),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "LPUSH" => {
            if command.len() != 3 {
                return resp_error("ERR wrong number of arguments for 'LPUSH'");
            }
            let line = format!("LPUSH {} {}", command[1], command[2]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    resp_integer(value.parse::<i64>().unwrap_or(0))
                }
                Ok(_) => resp_integer(0),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "RPOP" => {
            if command.len() != 2 {
                return resp_error("ERR wrong number of arguments for 'RPOP'");
            }
            let line = format!("RPOP {}", command[1]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    if value == "(nil)" {
                        resp_bulk(None)
                    } else {
                        resp_bulk(Some(&value))
                    }
                }
                Ok(_) => resp_bulk(None),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "ZADD" => {
            if command.len() != 4 {
                return resp_error("ERR wrong number of arguments for 'ZADD'");
            }
            let line = format!("ZADD {} {} {}", command[1], command[2], command[3]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    resp_integer(value.parse::<i64>().unwrap_or(0))
                }
                Ok(_) => resp_integer(0),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "ZRANGE" => {
            if command.len() != 4 {
                return resp_error("ERR wrong number of arguments for 'ZRANGE'");
            }
            let line = format!("ZRANGE {} {} {}", command[1], command[2], command[3]);
            let mut db = state.lock().await;
            match db.execute_line(&line) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    if value == "(empty)" {
                        "*0\r\n".to_string()
                    } else {
                        let items = value.split('\n').collect::<Vec<_>>();
                        let mut response = format!("*{}\r\n", items.len());
                        for item in items {
                            response.push_str(&resp_bulk(Some(item)));
                        }
                        response
                    }
                }
                Ok(_) => "*0\r\n".to_string(),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "EXPIRE" => {
            if command.len() != 3 {
                return resp_error("ERR wrong number of arguments for 'EXPIRE'");
            }
            let line = format!("EXPIRE {} {}", command[1], command[2]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    let parsed = value.parse::<i64>().unwrap_or(0);
                    resp_integer(parsed)
                }
                Ok(_) => resp_integer(0),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "TTL" => {
            if command.len() != 2 {
                return resp_error("ERR wrong number of arguments for 'TTL'");
            }
            let line = format!("TTL {}", command[1]);
            let mut db = state.lock().await;
            match db.execute_line(&line) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    let parsed = value.parse::<i64>().unwrap_or(-2);
                    resp_integer(parsed)
                }
                Ok(_) => resp_integer(-2),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "PERSIST" => {
            if command.len() != 2 {
                return resp_error("ERR wrong number of arguments for 'PERSIST'");
            }
            let line = format!("PERSIST {}", command[1]);
            let mut db = state.lock().await;
            match db.execute_line_with_persistence(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
                } else {
                    None
                },
                if options.appendonly {
                    Some(options.aof_file.as_str())
                } else {
                    None
                },
            ) {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => {
                    let parsed = value.parse::<i64>().unwrap_or(0);
                    resp_integer(parsed)
                }
                Ok(_) => resp_integer(0),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "SAVE" => {
            let target = if command.len() == 2 {
                command[1].as_str()
            } else if command.len() == 1 {
                options.data_file.as_str()
            } else {
                return resp_error("ERR wrong number of arguments for 'SAVE'");
            };

            let db = state.lock().await;
            match db.save_to_path(target) {
                Ok(()) => "+OK\r\n".to_string(),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "ROLE" => {
            if command.len() != 1 {
                return resp_error("ERR wrong number of arguments for 'ROLE'");
            }

            let mut db = state.lock().await;
            match db.execute_line("ROLE") {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => resp_bulk(Some(&value)),
                Ok(_) => resp_bulk(None),
                Err(error) => resp_error(&format!("ERR {error}")),
            }
        }
        "INFO" => {
            if command.len() > 2 {
                return resp_error("ERR wrong number of arguments for 'INFO'");
            }

            let mut db = state.lock().await;
            let base_info = match db.execute_line("INFO") {
                Ok(Some(crate::RuntimeMessage::Continue(value))) => value,
                Ok(_) => String::new(),
                Err(error) => return resp_error(&format!("ERR {error}")),
            };

            let m = metrics.lock().await;
            let uptime = now_secs().saturating_sub(m.started_at_secs);
            let payload = format!(
                "{}# Connections\r\nconnected_clients:{}\r\ntotal_connections_received:{}\r\nserver_uptime_in_seconds:{}\r\n",
                base_info, m.current_connections, m.total_connections_received, uptime
            );
            resp_bulk(Some(&payload))
        }
        "QUIT" => "+OK\r\n".to_string(),
        _ => resp_error("ERR unknown command"),
    }
}

fn now_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}

fn is_auth_exempt_command(verb: &str) -> bool {
    matches!(verb, "AUTH" | "PING" | "ECHO" | "QUIT")
}

async fn read_resp_command<R>(reader: &mut BufReader<R>) -> Result<Option<Vec<String>>, String>
where
    R: AsyncRead + Unpin,
{
    let mut first = [0u8; 1];
    match reader.read_exact(&mut first).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(format!("ERR read failed: {e}")),
    }

    if first[0] != b'*' {
        return Err("ERR protocol error: expected array".to_string());
    }

    let count = read_integer_line(reader).await?;
    if count < 0 {
        return Err("ERR protocol error: negative array length".to_string());
    }

    let mut parts = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let mut marker = [0u8; 1];
        reader
            .read_exact(&mut marker)
            .await
            .map_err(|e| format!("ERR read failed: {e}"))?;

        if marker[0] != b'$' {
            return Err("ERR protocol error: expected bulk string".to_string());
        }

        let len = read_integer_line(reader).await?;
        if len < 0 {
            return Err("ERR protocol error: null bulk string is unsupported".to_string());
        }

        let mut buf = vec![0u8; len as usize];
        reader
            .read_exact(&mut buf)
            .await
            .map_err(|e| format!("ERR read failed: {e}"))?;

        let mut crlf = [0u8; 2];
        reader
            .read_exact(&mut crlf)
            .await
            .map_err(|e| format!("ERR read failed: {e}"))?;
        if crlf != [b'\r', b'\n'] {
            return Err("ERR protocol error: missing CRLF".to_string());
        }

        let value = String::from_utf8(buf)
            .map_err(|_| "ERR protocol error: invalid UTF-8 bulk string".to_string())?;
        parts.push(value);
    }

    Ok(Some(parts))
}

async fn read_integer_line<R>(reader: &mut BufReader<R>) -> Result<i64, String>
where
    R: AsyncRead + Unpin,
{
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("ERR read failed: {e}"))?;

    if !line.ends_with("\r\n") {
        return Err("ERR protocol error: expected CRLF".to_string());
    }

    let trimmed = line.trim_end_matches("\r\n");
    trimmed
        .parse::<i64>()
        .map_err(|_| "ERR protocol error: invalid length".to_string())
}

fn resp_error(message: &str) -> String {
    format!("-{message}\r\n")
}

fn resp_bulk(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("${}\r\n{}\r\n", value.len(), value),
        None => "$-1\r\n".to_string(),
    }
}

fn resp_integer(value: i64) -> String {
    format!(":{value}\r\n")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{execute_resp, read_resp_command, resp_bulk, resp_integer, ServerMetrics, ServerOptions};
    use crate::config::EvictionPolicy;
    use crate::RedisLite;
    use tokio::io::BufReader;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn parses_resp_array_command() {
        let input = b"*2\r\n$4\r\nPING\r\n$5\r\nhello\r\n";
        let mut reader = BufReader::new(&input[..]);

        let command = read_resp_command(&mut reader)
            .await
            .expect("command should parse")
            .expect("command should exist");
        assert_eq!(command, vec!["PING".to_string(), "hello".to_string()]);
    }

    #[test]
    fn formats_bulk_and_integer_responses() {
        assert_eq!(resp_bulk(Some("ok")), "$2\r\nok\r\n");
        assert_eq!(resp_bulk(None), "$-1\r\n");
        assert_eq!(resp_integer(2), ":2\r\n");
    }

    #[tokio::test]
    async fn auth_required_blocks_write_until_auth() {
        let state = Arc::new(Mutex::new(RedisLite::new()));
        let metrics = Arc::new(Mutex::new(ServerMetrics::new()));
        let options = ServerOptions {
            bind_addr: "127.0.0.1:0".to_string(),
            data_file: "./tmp.json".to_string(),
            aof_file: "./tmp.aof".to_string(),
            autoload: false,
            autosave: false,
            appendonly: false,
            max_keys: None,
            eviction_policy: EvictionPolicy::NoEviction,
            requirepass: Some("secret".to_string()),
        };

        let mut authed = false;
        let blocked = execute_resp(
            &state,
            &options,
            &metrics,
            &["SET".to_string(), "k".to_string(), "v".to_string()],
            &mut authed,
        )
        .await;
        assert!(blocked.starts_with("-NOAUTH"));

        let wrong = execute_resp(
            &state,
            &options,
            &metrics,
            &["AUTH".to_string(), "bad".to_string()],
            &mut authed,
        )
        .await;
        assert!(wrong.starts_with("-WRONGPASS"));

        let ok = execute_resp(
            &state,
            &options,
            &metrics,
            &["AUTH".to_string(), "secret".to_string()],
            &mut authed,
        )
        .await;
        assert_eq!(ok, "+OK\r\n");
        assert!(authed);

        let set_ok = execute_resp(
            &state,
            &options,
            &metrics,
            &["SET".to_string(), "k".to_string(), "v".to_string()],
            &mut authed,
        )
        .await;
        assert_eq!(set_ok, "+OK\r\n");
    }
}
