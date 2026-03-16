use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::RedisLite;

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub bind_addr: String,
    pub data_file: String,
    pub autoload: bool,
    pub autosave: bool,
}

pub async fn run_server(options: ServerOptions) -> Result<(), AppError> {
    let listener = TcpListener::bind(&options.bind_addr)
        .await
        .map_err(|e| AppError::Config(format!("failed to bind {}: {e}", options.bind_addr)))?;

    let mut app = RedisLite::new();
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
    }

    let shared = Arc::new(Mutex::new(app));

    println!("redis-lite RESP server listening on {}", options.bind_addr);

    loop {
        let (stream, peer) = listener
            .accept()
            .await
            .map_err(|e| AppError::Config(format!("failed to accept connection: {e}")))?;

        let state = Arc::clone(&shared);
        let options_clone = options.clone();

        tokio::spawn(async move {
            if let Err(error) = handle_client(stream, state, options_clone).await {
                eprintln!("client {peer} error: {error}");
            }
        });
    }
}

async fn handle_client(
    stream: TcpStream,
    state: Arc<Mutex<RedisLite>>,
    options: ServerOptions,
) -> Result<(), AppError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

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

        let response = execute_resp(&state, &options, &command).await;

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
    command: &[String],
) -> String {
    if command.is_empty() {
        return resp_error("ERR empty command");
    }

    let verb = command[0].to_ascii_uppercase();
    match verb.as_str() {
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
            match db.execute_line_with_autosave(
                &line,
                if options.autosave {
                    Some(options.data_file.as_str())
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
                match db.execute_line_with_autosave(
                    &line,
                    if options.autosave {
                        Some(options.data_file.as_str())
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
        "QUIT" => "+OK\r\n".to_string(),
        _ => resp_error("ERR unknown command"),
    }
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
    use super::{read_resp_command, resp_bulk, resp_integer};
    use tokio::io::BufReader;

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
}
