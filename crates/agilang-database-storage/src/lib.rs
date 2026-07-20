use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const MAGIC_BYTES: &[u8; 8] = b"AGIDB001";
pub const DEFAULT_PAGE_SIZE: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHeader {
    pub magic: [u8; 8],
    pub format_version: u32,
    pub page_size: u32,
    pub database_id: [u8; 16],
    pub created_at: u64,
    pub checkpoint_lsn: u64,
    pub checksum: u64,
}

impl DatabaseHeader {
    pub fn new() -> Self {
        Self {
            magic: *MAGIC_BYTES,
            format_version: 1,
            page_size: DEFAULT_PAGE_SIZE as u32,
            database_id: [1u8; 16],
            created_at: 1774000000,
            checkpoint_lsn: 0,
            checksum: 0x90abcdef,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if &self.magic != MAGIC_BYTES {
            bail!("E6302 Invalid database format: magic header does not match AGIDB001");
        }
        if self.format_version != 1 {
            bail!(
                "E6303 Unsupported database version: version {}",
                self.format_version
            );
        }
        Ok(())
    }
}

impl Default for DatabaseHeader {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageType {
    Metadata,
    Table,
    BTreeInternal,
    BTreeLeaf,
    Overflow,
    FreeList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageHeader {
    pub page_id: u64,
    pub page_type: PageType,
    pub generation: u64,
    pub payload_length: u32,
    pub checksum: u64,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub header: PageHeader,
    pub data: Vec<u8>,
}

impl Page {
    pub fn new(page_id: u64, page_type: PageType) -> Self {
        Self {
            header: PageHeader {
                page_id,
                page_type,
                generation: 1,
                payload_length: 0,
                checksum: 0,
            },
            data: vec![0u8; DEFAULT_PAGE_SIZE],
        }
    }
}

#[derive(Debug)]
pub struct PageManager {
    pub header: DatabaseHeader,
    pub pages: Vec<Page>,
}

impl PageManager {
    pub fn new() -> Self {
        Self {
            header: DatabaseHeader::new(),
            pages: Vec::new(),
        }
    }

    pub fn allocate_page(&mut self, page_type: PageType) -> u64 {
        let page_id = self.pages.len() as u64;
        let page = Page::new(page_id, page_type);
        self.pages.push(page);
        page_id
    }

    pub fn read_page(&self, page_id: u64) -> Result<&Page> {
        if let Some(page) = self.pages.get(page_id as usize) {
            Ok(page)
        } else {
            bail!("E6304 Page read error: page_id {} out of bounds", page_id);
        }
    }
}

impl Default for PageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_header_validation() {
        let header = DatabaseHeader::new();
        assert!(header.validate().is_ok());

        let mut invalid = header;
        invalid.magic = *b"BADMAGIC";
        let err = invalid.validate();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6302"));
    }

    #[test]
    fn test_page_allocation() {
        let mut mgr = PageManager::new();
        let pid = mgr.allocate_page(PageType::Table);
        assert_eq!(pid, 0);

        let page = mgr.read_page(0).unwrap();
        assert_eq!(page.header.page_type, PageType::Table);
    }
}
