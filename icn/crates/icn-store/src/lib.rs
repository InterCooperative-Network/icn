//! ICN Store - Persistent key-value storage abstraction

use anyhow::Result;

/// Storage trait for pluggable backends
pub trait Store: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
}

/// Sled-based storage implementation
pub struct SledStore {
    db: sled::Db,
}

impl SledStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(SledStore { db })
    }

    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(SledStore { db })
    }
}

impl Store for SledStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get(key)?.map(|v| v.to_vec()))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.insert(key, value)?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.remove(key)?;
        Ok(())
    }
}
