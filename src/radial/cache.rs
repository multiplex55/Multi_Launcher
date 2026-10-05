use super::model::RenderingQuality;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetCacheKey {
    pub identity: String,
    pub version: String,
    pub effective_style: u64,
    pub dpi_milli: u32,
    pub logical_width_milli: u32,
    pub logical_height_milli: u32,
    pub quality: RenderingQuality,
}

#[derive(Debug, PartialEq)]
pub enum CachedAsset<T> {
    Ready(Arc<T>),
    Unavailable(Arc<str>),
}

impl<T> Clone for CachedAsset<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Ready(value) => Self::Ready(Arc::clone(value)),
            Self::Unavailable(reason) => Self::Unavailable(Arc::clone(reason)),
        }
    }
}

struct Entry<T> {
    value: CachedAsset<T>,
    cost: usize,
    touched: u64,
}

/// Small deterministic LRU used only by the preparation service. Paint and
/// hit-testing receive `Arc` snapshots and never consult this cache.
pub struct PreparedAssetCache<T> {
    entries: BTreeMap<AssetCacheKey, Entry<T>>,
    max_entries: usize,
    max_cost: usize,
    cost: usize,
    clock: u64,
}

impl<T> PreparedAssetCache<T> {
    pub fn new(max_entries: usize, max_cost: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            max_entries: max_entries.max(1),
            max_cost: max_cost.max(1),
            cost: 0,
            clock: 0,
        }
    }

    pub fn get(&mut self, key: &AssetCacheKey) -> Option<CachedAsset<T>> {
        let entry = self.entries.get_mut(key)?;
        self.clock = self.clock.wrapping_add(1);
        entry.touched = self.clock;
        Some(entry.value.clone())
    }

    pub fn insert_ready(&mut self, key: AssetCacheKey, value: T, cost: usize) -> Arc<T> {
        let value = Arc::new(value);
        self.insert(key, CachedAsset::Ready(Arc::clone(&value)), cost.max(1));
        value
    }

    pub fn insert_unavailable(&mut self, key: AssetCacheKey, reason: impl Into<Arc<str>>) {
        self.insert(key, CachedAsset::Unavailable(reason.into()), 1);
    }

    fn insert(&mut self, key: AssetCacheKey, value: CachedAsset<T>, cost: usize) {
        if let Some(previous) = self.entries.remove(&key) {
            self.cost = self.cost.saturating_sub(previous.cost);
        }
        self.clock = self.clock.wrapping_add(1);
        self.cost = self.cost.saturating_add(cost);
        self.entries.insert(
            key,
            Entry {
                value,
                cost,
                touched: self.clock,
            },
        );
        while self.entries.len() > self.max_entries || self.cost > self.max_cost {
            let Some(evict) = self
                .entries
                .iter()
                .min_by_key(|(key, entry)| (entry.touched, *key))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&evict) {
                self.cost = self.cost.saturating_sub(removed.cost);
            }
        }
    }

    pub fn invalidate_identity(&mut self, identity: &str) {
        let keys = self
            .entries
            .keys()
            .filter(|key| key.identity == identity)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            if let Some(removed) = self.entries.remove(&key) {
                self.cost = self.cost.saturating_sub(removed.cost);
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.cost = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(identity: &str, version: &str, dpi: u32) -> AssetCacheKey {
        AssetCacheKey {
            identity: identity.into(),
            version: version.into(),
            effective_style: 1,
            dpi_milli: dpi,
            logical_width_milli: 32_000,
            logical_height_milli: 32_000,
            quality: RenderingQuality::Balanced,
        }
    }

    #[test]
    fn cache_identity_includes_version_and_dpi_and_hits_are_shared() {
        let mut cache = PreparedAssetCache::new(4, 64);
        let first = cache.insert_ready(key("a", "v1", 1000), vec![1u8], 1);
        assert!(
            matches!(cache.get(&key("a", "v1", 1000)), Some(CachedAsset::Ready(value)) if Arc::ptr_eq(&first, &value))
        );
        assert!(cache.get(&key("a", "v2", 1000)).is_none());
        assert!(cache.get(&key("a", "v1", 1500)).is_none());
    }

    #[test]
    fn eviction_negative_entries_and_identity_invalidation_are_bounded() {
        let mut cache = PreparedAssetCache::<Vec<u8>>::new(2, 2);
        cache.insert_unavailable(key("missing", "0", 1000), "not found");
        cache.insert_ready(key("a", "1", 1000), vec![1], 1);
        let _ = cache.get(&key("missing", "0", 1000));
        cache.insert_ready(key("b", "1", 1000), vec![2], 1);
        assert!(cache.get(&key("a", "1", 1000)).is_none());
        assert!(matches!(
            cache.get(&key("missing", "0", 1000)),
            Some(CachedAsset::Unavailable(_))
        ));
        cache.invalidate_identity("missing");
        assert_eq!(cache.len(), 1);
    }
}
