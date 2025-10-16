use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

/// Simple thread-safe in-memory cache wrapper.
/// Provides a small API for get/set/delete so server logic doesn't manipulate the lock directly.
#[derive(Clone)]
pub struct Cache(Arc<Mutex<HashMap<String, Value>>>);

impl Cache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Cache(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Set a key to a JSON value.
    pub fn set(&self, key: String, value: Value) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(key, value);
    }

    /// Get a value by key. Returns a cloned Value if present.
    pub fn get(&self, key: &str) -> Option<Value> {
        let guard = self.0.lock().unwrap();
        guard.get(key).cloned()
    }

    /// Delete a key. Returns 1 if removed, 0 if not present.
    pub fn delete(&self, key: &str) -> usize {
        let mut guard = self.0.lock().unwrap();
        if guard.remove(key).is_some() { 1 } else { 0 }
    }
}
