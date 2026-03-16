#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    Set {
        key: String,
        value: String,
    },
    Get {
        key: String,
    },
    Delete {
        key: String,
    },
    HSet {
        key: String,
        field: String,
        value: String,
    },
    HGet {
        key: String,
        field: String,
    },
    SAdd {
        key: String,
        member: String,
    },
    SMembers {
        key: String,
    },
    LPush {
        key: String,
        value: String,
    },
    RPop {
        key: String,
    },
    ZAdd {
        key: String,
        score: i64,
        member: String,
    },
    ZRange {
        key: String,
        start: i64,
        stop: i64,
    },
    Expire {
        key: String,
        seconds: u64,
    },
    Ttl {
        key: String,
    },
    Persist {
        key: String,
    },
    Save {
        file: String,
    },
    Load {
        file: String,
    },
    Backup {
        file: String,
    },
    Restore {
        file: String,
    },
    Slaveof {
        host: String,
        port: u16,
    },
    SlaveofNoOne,
    Role,
    Replconf {
        subcommand: String,
        args: Vec<String>,
    },
    Psync {
        replication_id: String,
        offset: i64,
    },
    Info,
    List,
    Help,
    Exit,
}

pub const HELP_TEXT: &str = "\
Available commands:\n\
  SET <key> <value>   Insert or overwrite a key with a value\n\
  GET <key>           Read the value for a key\n\
  DELETE <key>        Remove a key\n\
  HSET <k> <f> <v>    Set field in a hash\n\
  HGET <k> <f>        Read field from a hash\n\
  SADD <k> <member>   Add member to a set\n\
  SMEMBERS <k>        List set members\n\
  LPUSH <k> <v>       Push value at list head\n\
  RPOP <k>            Pop value from list tail\n\
  ZADD <k> <s> <m>    Add/update sorted-set member with integer score\n\
  ZRANGE <k> <a> <b>  List sorted-set members in rank range\n\
  EXPIRE <key> <sec>  Set key expiration in seconds\n\
  TTL <key>           Show seconds to live (-1 no ttl, -2 missing)\n\
  PERSIST <key>       Remove expiration from a key\n\
  SAVE <file>         Persist the full store to a JSON file\n\
  LOAD <file>         Replace in-memory state from a JSON file\n\
  BACKUP <file>       Create a snapshot backup file\n\
  RESTORE <file>      Restore in-memory state from a backup\n\
  SLAVEOF <host> <p>  Replicate from primary (SLAVEOF NO ONE to stop)\n\
  ROLE                Show if role is master/slave and replication info\n\
    INFO                Show runtime server and stats information\n\
  LIST                Show all stored entries\n\
  HELP                Show this help\n\
  EXIT                Quit the application\n";
