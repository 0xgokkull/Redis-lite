use std::collections::HashMap;

use crate::error::AppError;

/// Category a RESP command belongs to for ACL permission checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    Read,
    Write,
    Admin,
}

impl CommandCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
        }
    }

    /// Names of all defined categories (including the `"all"` alias).
    pub fn all_names() -> &'static [&'static str] {
        &["all", "read", "write", "admin"]
    }

    /// Commands belonging to the named category, or `None` for an unknown category name.
    pub fn commands_in_category(cat: &str) -> Option<Vec<&'static str>> {
        match cat.to_ascii_lowercase().as_str() {
            "read" => Some(vec![
                "GET", "HGET", "SMEMBERS", "ZRANGE", "TTL", "INFO", "ROLE", "PING", "ECHO",
            ]),
            "write" => Some(vec![
                "SET", "DEL", "HSET", "SADD", "LPUSH", "RPOP", "ZADD", "EXPIRE", "PERSIST",
                "SAVE",
            ]),
            "admin" => Some(vec![
                "AUTH", "ACLLIST", "SLAVEOF", "REPLCONF", "PSYNC", "QUIT",
            ]),
            "all" => {
                let mut all: Vec<&'static str> = Vec::new();
                all.extend(Self::commands_in_category("read").unwrap());
                all.extend(Self::commands_in_category("write").unwrap());
                all.extend(Self::commands_in_category("admin").unwrap());
                Some(all)
            }
            _ => None,
        }
    }
}

/// Maps a RESP verb to its permission category.
pub fn command_category(verb: &str) -> CommandCategory {
    match verb.to_ascii_uppercase().as_str() {
        "GET" | "HGET" | "SMEMBERS" | "ZRANGE" | "TTL" | "INFO" | "ROLE" | "PING" | "ECHO" => {
            CommandCategory::Read
        }
        "SET" | "DEL" | "HSET" | "SADD" | "LPUSH" | "RPOP" | "ZADD" | "EXPIRE" | "PERSIST"
        | "SAVE" => CommandCategory::Write,
        _ => CommandCategory::Admin,
    }
}

/// A single configured ACL user with per-category permission flags.
#[derive(Debug, Clone)]
pub struct AclUser {
    pub name: String,
    /// `None` means `nopass` — the user authenticates without providing a password.
    pub password: Option<String>,
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_admin: bool,
}

impl AclUser {
    /// Returns `true` when this user is allowed to execute `verb`.
    ///
    /// Connection-level commands (`AUTH`, `QUIT`, `ACLWHOAMI`, `ACLCAT`, `ACLLIST`) are always
    /// permitted regardless of category flags.
    pub fn can_run(&self, verb: &str) -> bool {
        let upper = verb.to_ascii_uppercase();
        if matches!(
            upper.as_str(),
            "AUTH" | "QUIT" | "ACLWHOAMI" | "ACLCAT" | "ACLLIST"
        ) {
            return true;
        }
        match command_category(verb) {
            CommandCategory::Read => self.allow_read,
            CommandCategory::Write => self.allow_write,
            CommandCategory::Admin => self.allow_admin,
        }
    }

    /// Returns a human-readable rule string with the password masked.
    pub fn to_rule_string(&self) -> String {
        let pass = if self.password.is_some() {
            "#<masked>".to_string()
        } else {
            "nopass".to_string()
        };
        let mut perms = Vec::new();
        if self.allow_read && self.allow_write && self.allow_admin {
            perms.push("+@all");
        } else {
            if self.allow_read {
                perms.push("+@read");
            }
            if self.allow_write {
                perms.push("+@write");
            }
            if self.allow_admin {
                perms.push("+@admin");
            }
        }
        if perms.is_empty() {
            perms.push("-@all");
        }
        format!("user {} on {} {}", self.name, pass, perms.join(" "))
    }
}

/// Parse one ACL rule string into an [`AclUser`].
///
/// Format: `<name> <password|nopass> [+@all|+@read|+@write|+@admin|-@...] ...`
///
/// Permissions tokens are applied left-to-right.  Omitting all permission tokens results in a
/// user with no allowed categories.
pub fn parse_acl_rule(rule: &str) -> Result<AclUser, AppError> {
    let tokens: Vec<&str> = rule.split_whitespace().collect();
    if tokens.len() < 2 {
        return Err(AppError::Config(format!(
            "invalid ACL rule (expected '<name> <password|nopass> [+@cat...]'): '{rule}'"
        )));
    }

    let name = tokens[0].to_string();
    let password = if tokens[1].eq_ignore_ascii_case("nopass") {
        None
    } else {
        Some(tokens[1].to_string())
    };

    let mut allow_read = false;
    let mut allow_write = false;
    let mut allow_admin = false;

    for token in &tokens[2..] {
        match token.to_ascii_lowercase().as_str() {
            "+@all" => {
                allow_read = true;
                allow_write = true;
                allow_admin = true;
            }
            "+@read" => allow_read = true,
            "+@write" => allow_write = true,
            "+@admin" => allow_admin = true,
            "-@all" => {
                allow_read = false;
                allow_write = false;
                allow_admin = false;
            }
            "-@read" => allow_read = false,
            "-@write" => allow_write = false,
            "-@admin" => allow_admin = false,
            other => {
                return Err(AppError::Config(format!(
                    "unknown ACL permission token '{other}' in rule: '{rule}'"
                )));
            }
        }
    }

    Ok(AclUser {
        name,
        password,
        allow_read,
        allow_write,
        allow_admin,
    })
}

/// In-memory store of configured ACL users.
#[derive(Debug, Clone, Default)]
pub struct AclStore {
    users: HashMap<String, AclUser>,
}

impl AclStore {
    /// Construct an empty store (ACL mode is off; server uses legacy `requirepass`).
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
        }
    }

    /// Build a store from a slice of rule strings, returning an error on the first invalid rule.
    pub fn from_rules(rules: &[String]) -> Result<Self, AppError> {
        let mut store = Self::new();
        for rule in rules {
            let user = parse_acl_rule(rule)?;
            store.users.insert(user.name.clone(), user);
        }
        Ok(store)
    }

    /// Returns `true` when no users are configured (ACL mode disabled).
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    /// Authenticate `username` with `password`.  Returns a reference to the user on success.
    pub fn authenticate(&self, username: &str, password: &str) -> Option<&AclUser> {
        let user = self.users.get(username)?;
        match &user.password {
            None => Some(user),                     // nopass — any password accepted
            Some(p) if p == password => Some(user), // correct password
            _ => None,                              // wrong password
        }
    }

    /// Returns `true` when `username` is allowed to execute `verb`.
    pub fn can_run(&self, username: &str, verb: &str) -> bool {
        match self.users.get(username) {
            Some(user) => user.can_run(verb),
            None => false, // unknown user: deny
        }
    }

    /// Look up a user by name.
    pub fn get_user(&self, name: &str) -> Option<&AclUser> {
        self.users.get(name)
    }

    /// Returns all rules sorted by username, with passwords masked.
    pub fn list_rules(&self) -> Vec<String> {
        let mut rules: Vec<String> = self.users.values().map(|u| u.to_rule_string()).collect();
        rules.sort();
        rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nopass_all_permissions() {
        let user = parse_acl_rule("default nopass +@all").unwrap();
        assert_eq!(user.name, "default");
        assert!(user.password.is_none());
        assert!(user.allow_read && user.allow_write && user.allow_admin);
    }

    #[test]
    fn parse_password_read_only() {
        let user = parse_acl_rule("alice secret123 +@read").unwrap();
        assert_eq!(user.name, "alice");
        assert_eq!(user.password.as_deref(), Some("secret123"));
        assert!(user.allow_read);
        assert!(!user.allow_write);
        assert!(!user.allow_admin);
    }

    #[test]
    fn parse_rule_too_short_returns_error() {
        let err = parse_acl_rule("onlyname").unwrap_err();
        assert!(matches!(err, crate::error::AppError::Config(_)));
    }

    #[test]
    fn parse_unknown_permission_token_returns_error() {
        let err = parse_acl_rule("alice pass +@unknown").unwrap_err();
        assert!(matches!(err, crate::error::AppError::Config(_)));
    }

    #[test]
    fn acl_store_authenticate_success_and_failure() {
        let rules = vec!["alice pass123 +@all".to_string()];
        let store = AclStore::from_rules(&rules).unwrap();
        assert!(store.authenticate("alice", "pass123").is_some());
        assert!(store.authenticate("alice", "wrong").is_none());
        assert!(store.authenticate("nobody", "pass123").is_none());
    }

    #[test]
    fn acl_store_nopass_authenticates_with_any_password() {
        let rules = vec!["anon nopass +@read".to_string()];
        let store = AclStore::from_rules(&rules).unwrap();
        assert!(store.authenticate("anon", "any_password").is_some());
        assert!(store.authenticate("anon", "").is_some());
    }

    #[test]
    fn acl_store_read_only_user_cannot_write() {
        let rules = vec!["reader nopass +@read".to_string()];
        let store = AclStore::from_rules(&rules).unwrap();
        assert!(store.can_run("reader", "GET"));
        assert!(store.can_run("reader", "HGET"));
        assert!(!store.can_run("reader", "SET"));
        assert!(!store.can_run("reader", "DEL"));
    }

    #[test]
    fn acl_user_connection_commands_always_permitted() {
        let user = AclUser {
            name: "minimal".to_string(),
            password: None,
            allow_read: false,
            allow_write: false,
            allow_admin: false,
        };
        assert!(user.can_run("QUIT"));
        assert!(user.can_run("AUTH"));
        assert!(user.can_run("ACLWHOAMI"));
        assert!(user.can_run("ACLCAT"));
        assert!(user.can_run("ACLLIST"));
    }

    #[test]
    fn to_rule_string_masks_password() {
        let user = parse_acl_rule("alice secret +@read +@write").unwrap();
        let rule = user.to_rule_string();
        assert!(!rule.contains("secret"), "password must be masked in rule output");
        assert!(rule.contains("alice"));
        assert!(rule.contains("+@read"));
        assert!(rule.contains("+@write"));
    }

    #[test]
    fn command_categories_cover_main_verbs() {
        assert_eq!(command_category("GET"), CommandCategory::Read);
        assert_eq!(command_category("SET"), CommandCategory::Write);
        assert_eq!(command_category("SLAVEOF"), CommandCategory::Admin);
    }

    #[test]
    fn commands_in_category_returns_known_entries() {
        let read_cmds = CommandCategory::commands_in_category("read").unwrap();
        assert!(read_cmds.contains(&"GET"));
        assert!(read_cmds.contains(&"TTL"));
        assert!(!read_cmds.contains(&"SET"));
        assert!(CommandCategory::commands_in_category("unknown").is_none());
        let all_cmds = CommandCategory::commands_in_category("all").unwrap();
        assert!(all_cmds.contains(&"GET"));
        assert!(all_cmds.contains(&"SET"));
        assert!(all_cmds.contains(&"AUTH"));
    }
}
