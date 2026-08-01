use anyhow::{bail, Result};
use std::collections::BTreeMap;

pub struct BTreeIndex {
    pub name: String,
    pub map: BTreeMap<i64, u64>,
}

impl BTreeIndex {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            map: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, key: i64, page_id: u64) -> Result<()> {
        if self.map.contains_key(&key) {
            bail!(
                "E6308 Duplicate primary key `{}` in index `{}`",
                key,
                self.name
            );
        }
        self.map.insert(key, page_id);
        Ok(())
    }

    pub fn get(&self, key: i64) -> Option<u64> {
        self.map.get(&key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btree_index_insert_and_duplicate_rejection() {
        let mut idx = BTreeIndex::new("users_pk");
        assert!(idx.insert(1, 10).is_ok());
        assert_eq!(idx.get(1), Some(10));

        let err = idx.insert(1, 20);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6308"));
    }
}
