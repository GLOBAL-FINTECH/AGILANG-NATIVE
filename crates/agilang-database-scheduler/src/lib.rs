use anyhow::Result;
use std::collections::HashSet;

pub struct ShardedScheduler {
    pub active_shards: HashSet<usize>,
}

impl ShardedScheduler {
    pub fn new() -> Self {
        Self {
            active_shards: HashSet::new(),
        }
    }

    pub fn acquire_shard_lock(&mut self, shard_id: usize) -> Result<bool> {
        if self.active_shards.contains(&shard_id) {
            Ok(false)
        } else {
            self.active_shards.insert(shard_id);
            Ok(true)
        }
    }

    pub fn release_shard_lock(&mut self, shard_id: usize) {
        self.active_shards.remove(&shard_id);
    }
}

impl Default for ShardedScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharded_scheduler_lock_isolation() {
        let mut scheduler = ShardedScheduler::new();

        assert!(scheduler.acquire_shard_lock(1).unwrap());
        assert!(!scheduler.acquire_shard_lock(1).unwrap()); // Re-acquire fails
        assert!(scheduler.acquire_shard_lock(2).unwrap()); // Different shard succeeds

        scheduler.release_shard_lock(1);
        assert!(scheduler.acquire_shard_lock(1).unwrap());
    }
}
