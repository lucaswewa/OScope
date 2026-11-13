//! The memo cache: steps' outputs, by a hash of their inputs and parameters
//! (ADR-0017).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rayon::prelude::*;
use serde::Serialize;

use crate::value::Value;

/// How the memo cache is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MemoStats {
    /// Steps' results held.
    pub entries: usize,
    /// The memory they hold, about, in bytes.
    pub bytes: usize,
    /// Steps given a result from it.
    pub hits: u64,
    /// Steps that looked, and ran.
    pub misses: u64,
}

/// Steps' outputs by their keys, least recently used first out past its
/// budget. Only values in memory are kept: holding GPU memory here would
/// take it from jobs' GPU budget.
pub(crate) struct Memo {
    budget: usize,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<u64, Entry>,
    stats: MemoStats,
    clock: u64,
}

struct Entry {
    values: Vec<Option<Value>>,
    bytes: usize,
    used: u64,
}

impl Memo {
    pub fn new(budget: usize) -> Memo {
        let inner = Mutex::new(Inner::default());
        Memo { budget, inner }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The outputs kept for `key`.
    pub fn get(&self, key: u64) -> Option<Vec<Option<Value>>> {
        let mut inner = self.lock();
        inner.clock += 1;
        let clock = inner.clock;
        let found = inner.entries.get_mut(&key).map(|entry| {
            entry.used = clock;
            entry.values.clone()
        });
        match found {
            Some(_) => inner.stats.hits += 1,
            None => inner.stats.misses += 1,
        }
        found
    }

    /// Keeps `values` for `key`, unless any is on the GPU or they are more
    /// than the budget.
    pub fn put(&self, key: u64, values: &[Option<Value>]) {
        if values.iter().flatten().any(Value::on_gpu) {
            return;
        }
        let bytes = values.iter().flatten().map(Value::bytes).sum();
        if bytes > self.budget {
            return;
        }
        let mut inner = self.lock();
        inner.clock += 1;
        let used = inner.clock;
        let values = values.to_vec();
        if let Some(old) = inner.entries.insert(
            key,
            Entry {
                values,
                bytes,
                used,
            },
        ) {
            inner.stats.bytes -= old.bytes;
        }
        inner.stats.bytes += bytes;
        while inner.stats.bytes > self.budget {
            let oldest = inner
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(&key, _)| key);
            let Some(entry) = oldest.and_then(|key| inner.entries.remove(&key)) else {
                break;
            };
            inner.stats.bytes -= entry.bytes;
        }
        inner.stats.entries = inner.entries.len();
    }

    pub fn stats(&self) -> MemoStats {
        self.lock().stats
    }
}

/// A key from its parts.
pub(crate) fn mix(parts: &[u64]) -> u64 {
    let mut hasher = DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

/// Hashes JSON with objects' members in the order of their names, so that
/// equal parameters give equal keys.
pub(crate) fn hash_json(value: &serde_json::Value, hasher: &mut impl Hasher) {
    use serde_json::Value as Json;
    match value {
        Json::Null => 0_u8.hash(hasher),
        Json::Bool(b) => (1_u8, b).hash(hasher),
        Json::Number(n) => (2_u8, n.to_string()).hash(hasher),
        Json::String(s) => (3_u8, s).hash(hasher),
        Json::Array(items) => {
            (4_u8, items.len()).hash(hasher);
            items.iter().for_each(|item| hash_json(item, hasher));
        }
        Json::Object(members) => {
            let mut sorted: Vec<_> = members.iter().collect();
            sorted.sort_by_key(|(name, _)| *name);
            (5_u8, sorted.len()).hash(hasher);
            for (name, member) in sorted {
                name.hash(hasher);
                hash_json(member, hasher);
            }
        }
    }
}

/// A hash of a value's content: a recipe input's part of the keys. Images
/// are hashed in parallel, in the current pool. An image on the GPU isn't
/// read back: it gets a key of its own, so it is never found.
pub(crate) fn content_key(value: &Value) -> u64 {
    let samples = |data: &[f32]| -> Vec<u64> {
        let chunks = data.par_chunks(1 << 16).map(|chunk| {
            let mut hasher = DefaultHasher::new();
            for pair in chunk.chunks(2) {
                let low = u64::from(pair[0].to_bits());
                let high = pair.get(1).map_or(0, |x| u64::from(x.to_bits()));
                hasher.write_u64(low | high << 32);
            }
            hasher.finish()
        });
        chunks.collect()
    };
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(value).hash(&mut hasher);
    match value {
        Value::Image(image) => (image.shape(), samples(image.data())).hash(&mut hasher),
        Value::Stack(images) => {
            for image in images.iter() {
                (image.shape(), samples(image.data())).hash(&mut hasher);
            }
        }
        Value::Mask(mask) => (mask.width(), mask.height(), mask.data()).hash(&mut hasher),
        Value::Table(table) => hash_json(
            &serde_json::to_value(&**table).unwrap_or_default(),
            &mut hasher,
        ),
        Value::Geometry(json) | Value::Json(json) | Value::Report(json) => {
            hash_json(json, &mut hasher)
        }
        #[cfg(feature = "cuda")]
        Value::DeviceImage(_) => {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .hash(&mut hasher);
        }
    }
    hasher.finish()
}
