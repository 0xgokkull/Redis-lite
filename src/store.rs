use std::collections::HashMap;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Store {
    data: HashMap<String, String>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn data(&self) -> &HashMap<String, String> {
        &self.data
    }

    pub fn set(&mut self, key: String, value: String) {
        self.data.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    pub fn list(&self) -> Vec<(&String, &String)> {
        let mut entries: Vec<(&String, &String)> = self.data.iter().collect();
        entries.sort_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));
        entries
    }

    pub fn replace_all(&mut self, new_data: HashMap<String, String>) {
        self.data = new_data;
    }
}

#[cfg(test)]
mod tests {
    use super::Store;

    #[test]
    fn inserts_new_key() {
        let mut store = Store::new();
        store.set("name".to_string(), "gokul".to_string());

        assert_eq!(store.get("name"), Some(&"gokul".to_string()));
    }

    #[test]
    fn overwrites_existing_key() {
        let mut store = Store::new();
        store.set("name".to_string(), "old".to_string());
        store.set("name".to_string(), "new".to_string());

        assert_eq!(store.get("name"), Some(&"new".to_string()));
    }

    #[test]
    fn gets_existing_value() {
        let mut store = Store::new();
        store.set("theme".to_string(), "dark".to_string());

        assert_eq!(store.get("theme"), Some(&"dark".to_string()));
    }

    #[test]
    fn deletes_existing_key() {
        let mut store = Store::new();
        store.set("session".to_string(), "abc123".to_string());

        assert!(store.delete("session"));
        assert_eq!(store.get("session"), None);
    }

    #[test]
    fn delete_missing_key_returns_false() {
        let mut store = Store::new();

        assert!(!store.delete("missing"));
    }
}
