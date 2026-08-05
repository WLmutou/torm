use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// 简单的 LRU 缓存实现
pub struct SimpleLruCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Clone + Hash + Eq, V: Clone> SimpleLruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        if self.map.contains_key(key) {
            // Move to end (most recently used)
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
                self.order.push_back(key.clone());
            }
            self.map.get(key).cloned()
        } else {
            None
        }
    }

    pub fn put(&mut self, key: K, value: V) {
        // Remove existing if present
        if self.map.contains_key(&key) {
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
        } 
        // Evict oldest if at capacity
        else if self.map.len() >= self.capacity {
            if let Some(old_key) = self.order.pop_front() {
                self.map.remove(&old_key);
            }
        }

        self.map.insert(key.clone(), value);
        self.order.push_back(key);
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.order.retain(|k| k != key);
        self.map.remove(key)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn resize(&mut self, new_capacity: usize) {
        self.capacity = new_capacity;
        while self.map.len() > new_capacity {
            if let Some(old_key) = self.order.pop_front() {
                self.map.remove(&old_key);
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.order.iter().filter_map(|key| {
            self.map.get(key).map(|value| (key, value))
        })
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.order.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.order.iter().filter_map(|key| self.map.get(key))
    }
}

impl<K: Clone + Hash + Eq, V: Clone> Default for SimpleLruCache<K, V> {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache_basic() {
        let mut cache = SimpleLruCache::new(2);
        
        cache.put(1, "one");
        cache.put(2, "two");
        
        assert_eq!(cache.get(&1), Some("one"));
        assert_eq!(cache.get(&2), Some("two"));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache = SimpleLruCache::new(2);
        
        cache.put(1, "one");
        cache.put(2, "two");
        cache.put(3, "three"); // Should evict key 1
        
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some("two"));
        assert_eq!(cache.get(&3), Some("three"));
    }

    #[test]
    fn test_lru_cache_lru_behavior() {
        let mut cache = SimpleLruCache::new(2);
        
        cache.put(1, "one");
        cache.put(2, "two");
        cache.get(&1); // Make key 1 recently used
        cache.put(3, "three"); // Should evict key 2
        
        assert_eq!(cache.get(&1), Some("one"));
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&3), Some("three"));
    }

    #[test]
    fn test_lru_cache_remove() {
        let mut cache = SimpleLruCache::new(10);
        
        cache.put(1, "one");
        cache.put(2, "two");
        
        assert_eq!(cache.remove(&1), Some("one"));
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_lru_cache_clear() {
        let mut cache = SimpleLruCache::new(10);
        
        cache.put(1, "one");
        cache.put(2, "two");
        cache.clear();
        
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_lru_cache_resize() {
        let mut cache = SimpleLruCache::new(5);
        
        cache.put(1, "one");
        cache.put(2, "two");
        cache.put(3, "three");
        
        cache.resize(2);
        
        assert_eq!(cache.len(), 2); // Should have evicted some
    }

    #[test]
    fn test_lru_cache_iteration() {
        let mut cache = SimpleLruCache::new(10);
        
        cache.put(1, "one");
        cache.put(2, "two");
        cache.put(3, "three");
        
        let keys: Vec<_> = cache.keys().cloned().collect();
        assert_eq!(keys, vec![1, 2, 3]);
        
        let values: Vec<&str> = values_iter(&cache).copied().collect();
        assert!(values.contains(&"one"));
        assert!(values.contains(&"two"));
        assert!(values.contains(&"three"));
    }

    #[test]
    fn test_lru_cache_update_existing() {
        let mut cache = SimpleLruCache::new(3);
        
        cache.put(1, "one");
        cache.put(2, "two");
        cache.put(3, "three");
        cache.put(1, "one_updated"); // Update existing
        
        assert_eq!(cache.get(&1), Some("one_updated"));
        assert_eq!(cache.len(), 3);
    }

    fn values_iter<'a, K: Clone + Hash + Eq, V: Clone>(
        cache: &'a SimpleLruCache<K, V>
    ) -> impl Iterator<Item = &'a V> {
        cache.values()
    }
}
