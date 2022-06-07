use std::sync::atomic::{AtomicU64, Ordering};

pub struct AbdadaTable {
    entries: Box<[AtomicU64]>,
}

impl AbdadaTable {
    pub fn new() -> Self {
        AbdadaTable {
            entries: (0..4096).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    pub fn is_searching(&self, hash: u64) -> bool {
        let entry = &self.entries[(hash % self.entries.len() as u64) as usize];
        let v = entry.load(Ordering::Acquire);
        v & !0xFF == hash & !0xFF && v & 0xFF > 0
    }

    pub fn enter(&self, hash: u64) -> Option<AbdadaGuard> {
        let entry = &self.entries[(hash % self.entries.len() as u64) as usize];
        let hash_upper = hash & !0xFF;
        entry
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                let count = current & 0xFF;
                if current & !0xFF == hash_upper && count < 0xFF {
                    Some(current + 1)
                } else if count == 0 {
                    Some(hash_upper + 1)
                } else {
                    None
                }
            })
            .ok()
            .map(|_| AbdadaGuard { entry })
    }
}

pub struct AbdadaGuard<'a> {
    entry: &'a AtomicU64,
}

impl Drop for AbdadaGuard<'_> {
    fn drop(&mut self) {
        self.entry.fetch_sub(1, Ordering::AcqRel);
    }
}
