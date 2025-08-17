use hashbrown::HashMap;
use std::collections::VecDeque;
use std::hash::Hash;

/// A simple LRU cache backed by `hashbrown::HashMap`.
#[derive(Default)]
pub struct LruCache<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    capacity: usize,
}

impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    /// Create a new cache limited to `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    /// Retrieve a value from the cache, updating its recency.
    pub fn get(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.map.get(key).cloned() {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.clone());
            Some(value)
        } else {
            None
        }
    }

    /// Insert a value into the cache.
    pub fn put(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            self.map.insert(key.clone(), value);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key);
            return;
        }

        if self.map.len() == self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.map.insert(key.clone(), value);
        self.order.push_back(key);
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}
