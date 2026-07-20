use agilang_database_storage::Page;
use anyhow::Result;
use std::collections::HashMap;

pub struct PageCache {
    pub max_pages: usize,
    pub cache: HashMap<u64, Page>,
    pub evictions: u64,
    pub hits: u64,
    pub misses: u64,
}

impl PageCache {
    pub fn new(max_pages: usize) -> Self {
        Self {
            max_pages,
            cache: HashMap::new(),
            evictions: 0,
            hits: 0,
            misses: 0,
        }
    }

    pub fn get_page(&mut self, page_id: u64) -> Option<&Page> {
        if self.cache.contains_key(&page_id) {
            self.hits += 1;
            self.cache.get(&page_id)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn put_page(&mut self, page: Page) -> Result<()> {
        let page_id = page.header.page_id;

        if self.cache.len() >= self.max_pages && !self.cache.contains_key(&page_id) {
            if let Some(&evict_id) = self.cache.keys().next() {
                self.cache.remove(&evict_id);
                self.evictions += 1;
            }
        }

        self.cache.insert(page_id, page);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_database_storage::PageType;

    #[test]
    fn test_page_cache_bounded_eviction() {
        let mut cache = PageCache::new(2);
        cache.put_page(Page::new(1, PageType::Table)).unwrap();
        cache.put_page(Page::new(2, PageType::Table)).unwrap();

        // 3rd page causes eviction of an existing page
        cache.put_page(Page::new(3, PageType::Table)).unwrap();
        assert_eq!(cache.cache.len(), 2);
        assert_eq!(cache.evictions, 1);
    }
}
