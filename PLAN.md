# Redis-lite Delivery Plan

## Part 1 - Project Foundation (Completed)

- Create Rust workspace layout and module boundaries
- Add typed command model (`Command` enum)
- Add shared application error type (`AppError`)
- Add compile-safe stubs for parser, store, and persistence

## Part 2 - Core Logic (Completed)

- Implement parser for all V1 commands
- Implement in-memory store operations: set/get/delete/list
- Implement JSON persistence: save/load with replace semantics
- Add focused unit tests for parser, store, and persistence

## Part 3 - REPL Integration and UX (Completed)

- Build interactive REPL loop in `main.rs`
- Wire parser + executor + store + persistence end-to-end
- Add clean stdout/stderr behavior and help messaging
- Add final integration checks and polish

## Redis Roadmap Implementation

### Step 1-2 - RESP TCP Server & Multi-Client Concurrency (Completed)

- Implemented RESP protocol parser/serializer
- Async TCP server using tokio
- Per-client task handling with shared state (Arc<Mutex<>>)
- Multi-client connection handling

### Step 3 - Persistence: RDB Snapshots & AOF (Completed)

- Atomic JSON snapshot writes with versioning
- AOF (append-only file) for command logging
- Startup recovery with snapshot-first, then AOF replay
- Format version checks for backward compatibility

### Step 4 - TTL & Eviction Policies (Completed)

- EXPIRE, TTL, PERSIST commands
- Automatic expiry purging before operations
- Two eviction modes: noeviction, allkeys-lru
- LRU tracking with access timestamps

### Step 5 - Rich Data Structures (Completed)

- Hash operations: HSET, HGET
- Set operations: SADD, SMEMBERS
- List operations: LPUSH, RPOP
- Sorted-set operations: ZADD, ZRANGE
- Type safety with WRONGTYPE errors
- Per-key type tracking and validation

### Step 6 - Replication & Failover Basics (Completed)

- Replication state management (master/slave roles)
- SLAVEOF command for replicating from primary
- ROLE command showing replication info
- PSYNC and REPLCONF commands for replica handshake
- Replication ID generation and offset tracking
- Foundation for replica broadcast (prepared in ReplicaConnections struct)

### Step 7 - Observability & Monitoring (Completed)

- INFO command in CLI and RESP mode with structured sections
- Runtime counters: total/read/write commands processed
- Keyspace counters: current key count and expiring key count
- Replication telemetry exposed via INFO
- Connection metrics in RESP INFO: connected clients, total connections, server uptime

### Step 8 - Security Basics (Completed)

- Added server password auth (`AUTH <password>`) with `--requirepass` support
- Per-connection authentication state in RESP server
- Command gating with `NOAUTH` for protected commands until authentication
- Startup config support for `requirepass` via args and env (`REDIS_LITE_REQUIREPASS`)
- Added integration-style server unit test for auth flow (deny, wrong pass, success)

### Step 9 - Deployment Hardening (Completed)

- Graceful server shutdown with `Ctrl+C` signal handling
- Final snapshot flush on shutdown when autosave is enabled
- Startup config validation (non-empty paths, non-empty requirepass, max-keys > 0)
- Added container packaging files: `Dockerfile` and `.dockerignore`
- README deployment guidance for secure server startup and Docker run

### Step 10 - Operational Logging & Tracing (Completed)


### Step 11 - Command-Level ACL (Completed)

- New `src/acl.rs` module: `CommandCategory` enum (Read / Write / Admin), `AclUser`, `AclStore`
- ACL rule format: `<name> <password|nopass> [+@all|+@read|+@write|+@admin|-@...]`
- `--acl-rule` CLI flag (repeatable) and `REDIS_LITE_ACL_RULES` env var (`;`-separated)
- ACL rules validated at startup via `validate_config()`
- Per-connection `Session` struct tracks authenticated username
- `AUTH [username] password` supports both single-arg (legacy) and two-arg (ACL) forms
- `NOPERM` response when an authenticated user lacks the required category permission
- Connection-level commands (`AUTH`, `QUIT`, `ACLWHOAMI`, `ACLCAT`) always bypass ACL check
- New RESP commands: `ACLWHOAMI`, `ACLCAT [category]`, `ACLLIST`
- Test: `acl_restricts_write_for_read_only_user` exercises full ACL auth → deny write → allow read flow
- Test count: 75 passing

