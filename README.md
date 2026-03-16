# Redis-lite Key-Value Store

## Purpose

This project is a small Rust-based key-value store designed for learning how a database works at the most basic level.

It should:

- accept commands from the terminal
- parse those commands into structured operations
- store data in memory using `HashMap`
- save data to a file
- load data back from a file
- support startup config via flags, env vars, and config file
- print clear output to the terminal

This is a good project because it stays small enough to finish, but still teaches real systems concepts:

- command parsing
- ownership and borrowing
- mutable state management
- file I/O
- serialization and deserialization
- error handling
- project structure in Rust

It is also a strong foundation for a future TCP server version.

---

## Use In Other Projects

You can use this project in two ways:

- as a CLI (`cargo run`)
- as a Rust library dependency
- as a RESP-compatible TCP server (`cargo run --bin redis-lite-server`)

### Run RESP server

Start server mode on default Redis port:

```text
cargo run --bin redis-lite-server -- --bind 127.0.0.1:6379 --autoload --autosave --appendonly
```

Quick protocol test using netcat:

```text
printf '*1\r\n$4\r\nPING\r\n' | nc 127.0.0.1 6379
```

Current RESP commands in server mode:

- `PING [message]`
- `ECHO <message>`
- `SET <key> <value>`
- `GET <key>`
- `DEL <key> [key ...]`
- `ROLE`
- `INFO [section]`
- `SAVE [file]`
- `QUIT`

Persistence behavior in current server mode:

- Snapshot (RDB-like JSON) with atomic writes
- AOF command logging for mutating commands (`SET`, `DEL`)
- Startup recovery order: snapshot first, then AOF replay

### Add as dependency

In another Rust project `Cargo.toml`, add a path or git dependency.

Path dependency example:

```toml
[dependencies]
redis-lite = { path = "../redis-lite" }
```

### Library API example

```rust
use redis_lite::{RedisLite, RuntimeMessage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = RedisLite::new();

    if let Some(RuntimeMessage::Continue(message)) = app.execute_line("SET project api")? {
        println!("{message}");
    }

    if let Some(RuntimeMessage::Continue(message)) = app.execute_line("GET project")? {
        println!("value: {message}");
    }

    Ok(())
}
```

`execute_line(...)` returns:

- `Ok(None)` for empty input
- `Ok(Some(RuntimeMessage::Continue(msg)))` for normal output
- `Ok(Some(RuntimeMessage::Exit(msg)))` when `EXIT` is issued
- `Err(...)` for invalid commands and I/O or JSON errors

### Startup config

The CLI supports production-friendly startup options:

- `--config <file>`
- `--data-file <file>`
- `--autoload` / `--no-autoload`
- `--autosave` / `--no-autosave`
- `--log-level <level>`

Environment variables are also supported:

- `REDIS_LITE_CONFIG`
- `REDIS_LITE_DATA_FILE`
- `REDIS_LITE_AUTOLOAD`
- `REDIS_LITE_AUTOSAVE`
- `REDIS_LITE_LOG_LEVEL`

---

## Research Summary

The best first version is a **single-process CLI application with an in-memory store and JSON persistence**.

That choice is correct for this stage because:

- `HashMap<String, String>` is the simplest useful store in Rust
- JSON is easy to inspect manually during development
- `serde` and `serde_json` are the standard Rust tools for serialization
- command parsing should be handled by your own parser for REPL commands, not by `clap`
- terminal input can be handled with `stdin().read_line(...)`

### Recommended Rust building blocks

#### In-memory store

Use:

```rust
use std::collections::HashMap;
```

Why:

- standard library type
- fast key lookup
- perfect fit for `SET`, `GET`, and `DELETE`

Recommended first store shape:

```rust
HashMap<String, String>
```

Do not start with mixed value types, TTL, binary persistence, or concurrency. Those are later-stage upgrades.

#### File persistence

Use:

- `serde`
- `serde_json`

Why:

- standard and well-supported in Rust
- can serialize a `HashMap<String, String>` directly
- makes saved data human-readable
- easy to debug when things go wrong

#### CLI and command input

Use two layers:

1. `std::io` for interactive command input
2. an internal parser for commands like `SET name gokul`

Important distinction:

- `clap` is useful for program startup flags like `--file data.json`
- `clap` is not the right tool for parsing commands typed inside your own REPL loop

For this project, `clap` is optional, not required.

---

## Correct Scope For Version 1

To keep the project clean and finishable, version 1 should support only these commands:

- `SET <key> <value>`
- `GET <key>`
- `DELETE <key>`
- `SAVE <file>`
- `LOAD <file>`
- `BACKUP <file>`
- `RESTORE <file>`
- `LIST`
- `HELP`
- `EXIT`

### Recommended command behavior

#### `SET <key> <value>`

- inserts a new key if it does not exist
- overwrites the old value if the key already exists
- returns a success message such as `OK`

#### `GET <key>`

- prints the stored value if the key exists
- prints a clear "not found" message if it does not exist

#### `DELETE <key>`

- removes the key if it exists
- prints whether deletion happened or the key was missing

#### `SAVE <file>`

- writes the current in-memory data to disk
- should serialize the full store, not partial entries
- should print success or a detailed error

#### `LOAD <file>`

- reads store data from disk
- should replace the current in-memory data in version 1
- should reject invalid file content cleanly

#### `LIST`

- prints all keys, or all key-value pairs
- useful for debugging and demoing the store

#### `HELP`

- prints available commands and examples

#### `EXIT`

- closes the program cleanly

---

## Recommended Architecture

The most correct structure is a small modular CLI app, not a single large `main.rs`.

### Suggested project layout

```text
redis-lite/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs
    ├── command.rs
    ├── parser.rs
    ├── store.rs
    ├── persistence.rs
    └── error.rs
```

### Module responsibilities

#### `main.rs`

Owns the application loop.

Responsibilities:

- print welcome text
- read user input from terminal
- call the parser
- execute parsed commands against the store
- print results and errors

#### `command.rs`

Defines the command model.

Example direction:

```rust
pub enum Command {
    Set { key: String, value: String },
    Get { key: String },
    Delete { key: String },
    Save { file: String },
    Load { file: String },
    List,
    Help,
    Exit,
}
```

Why this is correct:

- commands become typed data instead of raw strings
- parsing and execution stay separate
- easier to test than mixing parsing logic with I/O

#### `parser.rs`

Converts raw terminal input into `Command`.

Responsibilities:

- trim whitespace
- split the command word from the rest
- validate argument counts
- return structured errors for invalid input

Key design rule:

For `SET`, treat everything after the key as the value so values can contain spaces.

Example:

```text
SET theme dark blue
```

Should become:

- key: `theme`
- value: `dark blue`

This is one of the most important parser decisions in the whole project.

#### `store.rs`

Owns the in-memory database.

Recommended shape:

```rust
pub struct Store {
    data: HashMap<String, String>,
}
```

Responsibilities:

- set key-value pairs
- get values
- delete values
- list entries
- replace whole state during load

Best practice:

Keep file I/O out of `store.rs`. The store should manage data, not files.

#### `persistence.rs`

Owns save/load logic.

Responsibilities:

- serialize store data to JSON
- write JSON to disk
- read JSON from disk
- deserialize JSON back into a store-compatible structure

Keep persistence separate from terminal printing.

#### `error.rs`

Defines application errors.

Recommended error categories:

- invalid command format
- unknown command
- missing argument
- I/O failure
- serialization failure
- deserialization failure
- file not found

---

## Data Model Recommendation

For version 1, use only:

```rust
HashMap<String, String>
```

This is the correct choice because it keeps the project focused on database behavior instead of type-system expansion.

### Why not generic values yet?

You could imagine values like numbers, booleans, lists, or JSON objects, but adding those now creates extra complexity in:

- parsing
- serialization rules
- command syntax
- printing behavior
- tests

For a learning project, `String -> String` is the right first milestone.

---

## Persistence Format

### Best first choice: JSON

Example saved file:

```json
{
  "theme": "dark",
  "timeout": "30",
  "session": "abc123"
}
```

Why JSON is the best first persistence format:

- readable by humans
- easy to inspect in an editor
- easy to debug
- supported directly by `serde_json`

### Save strategy

At minimum:

- serialize the full map
- write to the requested file path
- return success or failure

Better practice:

- write to a temporary file first
- rename the temp file to the final file

That reduces corruption risk if the program crashes during save.

### Load strategy

For version 1, use **replace semantics**.

That means:

- file contents become the new full state
- current in-memory data is replaced

This is better than merge semantics for a beginner project because behavior stays predictable.

---

## Parser Design Notes

This part matters more than it seems.

Do not directly mix these concerns together:

- reading terminal input
- parsing command syntax
- executing store actions
- formatting output

Keep the flow like this:

```text
stdin line -> parser -> Command enum -> executor/store -> terminal output
```

This separation is what makes the code maintainable.

### Recommended parsing rules

- commands should be case-insensitive or clearly documented as case-sensitive
- extra leading and trailing spaces should be ignored
- unknown commands should show help guidance
- invalid argument counts should produce exact feedback

Example:

```text
SET username
```

Should not fail vaguely. It should say something like:

```text
error: SET requires a key and a value
```

---

## Terminal Output Rules

Keep terminal behavior simple and consistent.

### Print to standard output

Use standard output for:

- successful command results
- values returned by `GET`
- help text
- normal status messages

### Print to standard error

Use standard error for:

- invalid commands
- failed file operations
- parse errors
- load failures

This is a small detail, but it is good CLI discipline and matches how serious tools behave.

---

## Recommended Dependencies

### Required for a clean implementation

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### Useful but optional

```toml
[dependencies]
thiserror = "2"
clap = { version = "4", features = ["derive"] }
```

Use `thiserror` if you want cleaner custom error types.

Use `clap` only if you want startup arguments like:

```text
redis-lite --file data.json
redis-lite --load startup.json
```

For the core REPL itself, your own parser is still the right design.

---

## Step-By-Step Way To Achieve This

This is the cleanest implementation path.

### Phase 1: Create the command model

Goal:

- define the `Command` enum
- define command variants and required data

Done correctly when:

- every supported command has a typed representation
- no command execution logic is mixed in yet

### Phase 2: Build the parser

Goal:

- convert raw strings into `Command`

Done correctly when:

- valid commands parse successfully
- invalid commands return precise errors
- `SET` handles values containing spaces

### Phase 3: Build the in-memory store

Goal:

- create a `Store` type around `HashMap<String, String>`

Done correctly when:

- `set`, `get`, `delete`, and `list` behave independently of terminal code

### Phase 4: Add persistence

Goal:

- save store data to JSON
- load store data from JSON

Done correctly when:

- saved files can be opened and inspected by hand
- loading malformed JSON returns a clean error

### Phase 5: Build the REPL loop

Goal:

- read commands from terminal continuously until `EXIT`

Done correctly when:

- I/O, parsing, execution, and output remain separate
- the program never crashes on bad input

### Phase 6: Add tests

Goal:

- verify parser behavior
- verify store behavior
- verify persistence behavior

Done correctly when:

- command parsing is covered for success and failure cases
- save/load round-trip tests pass

---

## Testing Strategy

If this project is done like a professional Rust project, most logic should be testable without launching the full app.

### Unit tests to write

#### Parser tests

- parse `SET name gokul`
- parse `SET bio rust developer`
- parse `GET name`
- reject empty input
- reject unknown commands
- reject missing arguments

#### Store tests

- insert a new key
- overwrite an existing key
- get an existing value
- delete an existing value
- delete a missing key

#### Persistence tests

- save a populated store
- load a valid JSON file
- reject invalid JSON
- round-trip save then load and compare equality

### Integration test target

Later, you can add CLI-level tests for full command execution, but that should come after the modules are already stable.

---

## Common Mistakes To Avoid

These are the main design mistakes that would make the project harder than necessary.

### 1. Putting everything in `main.rs`

This makes parsing, storage, persistence, and output tightly coupled.

### 2. Using `clap` for REPL commands

`clap` is for command-line arguments passed when the program starts, not for repeated interactive commands typed after startup.

### 3. Storing borrowed `&str` values in the map

Use owned `String` values. Borrowed references will create lifetime problems for no benefit here.

### 4. Mixing file I/O into store logic

Persistence should be in its own module.

### 5. Making the value type too advanced too early

Do not begin with enums for integers, booleans, lists, and null. First finish the simpler string-based store.

### 6. Weak error messages

Bad input should produce exact messages, not generic failure output.

---

## Use Cases

This small project is useful for:

- storing app settings
- saving temporary session data
- caching simple values
- fast local lookup for small tools

It is also useful as a teaching project for:

- command-driven application design
- REPL architecture
- Rust ownership with collections
- serialization workflows
- clean module boundaries

---

## Best Professional Recommendation

If this project is being built for learning, the most correct version is:

- interactive terminal app
- custom `Command` enum
- custom parser
- `Store` wrapper around `HashMap<String, String>`
- JSON save/load with `serde` and `serde_json`
- clear separation between parser, storage, persistence, and output
- tests for parser, store, and persistence before any advanced features

That is the right architecture because it stays small, idiomatic, and extensible.

---

## Future Growth Path

Once version 1 is complete, the natural next upgrades are:

1. startup arguments with `clap`
2. atomic save strategy with temp-file rename
3. command history and better REPL UX
4. typed values instead of only strings
5. expiration or TTL support
6. append-only log instead of snapshot saves
7. TCP server mode
8. concurrent access handling

The future TCP server version should reuse the same parser, store, and persistence design where possible.

---

## Final Analysis

This project should not start as a "mini Redis clone" in the full sense. That would push it toward networking, protocols, concurrency, persistence guarantees, and much more complexity than you need.

The correct first interpretation of "Redis-lite" is:

- local process only
- string keys and values
- command-driven interface
- snapshot persistence
- strong modular structure

If you build it in that order, you will learn the right Rust concepts without overcomplicating the project.