use std::sync::atomic::{AtomicU8, Ordering};

pub struct AbdadaTable {
    entries: Box<[AtomicU8]>,
}

impl AbdadaTable {
    pub fn new() -> Self {
        AbdadaTable {
            entries: (0..65536).map(|_| AtomicU8::new(0)).collect(),
        }
    }

    pub fn is_searching(&self, hash: u64) -> bool {
        let entry = &self.entries[(hash % self.entries.len() as u64) as usize];
        entry.load(Ordering::Acquire) > 0
    }

    pub fn enter(&self, hash: u64) -> Option<AbdadaGuard> {
        let entry = &self.entries[(hash % self.entries.len() as u64) as usize];
        entry.fetch_add(1, Ordering::Acquire);
        Some(AbdadaGuard { entry })
    }
}

pub struct AbdadaGuard<'a> {
    entry: &'a AtomicU8,
}

impl Drop for AbdadaGuard<'_> {
    fn drop(&mut self) {
        self.entry.fetch_sub(1, Ordering::Release);
    }
}
