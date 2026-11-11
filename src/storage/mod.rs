pub mod tmp_ring;
pub mod persistent_mmap;

use std::sync::Arc;
use crate::error::Result;
use crate::locator::{ArenaId, DataLocator, SystemId, TypeId};

#[derive(Debug, Clone, Copy)]
pub enum OverflowPolicy {
    OverwriteOldest,
    RejectNew,
}

pub trait StorageEngine: Send + Sync {
    fn system_id(&self) -> SystemId;
    fn arena_id(&self) -> ArenaId;
    fn generation(&self) -> u32;

    fn append(&self, type_id: TypeId, data: &[u8]) -> Result<DataLocator>;
    fn read(&self, locator: &DataLocator) -> Result<Arc<Vec<u8>>>;
    fn flush(&self) -> Result<()>;
    fn used_bytes(&self) -> usize;
    fn capacity(&self) -> usize;
}

// Common Persistent constants
pub const ARENA_MAGIC: &[u8; 8] = b"RDFNDA01";
pub const RECORD_MAGIC: &[u8; 4] = b"RDFR";
pub const CHECKPOINT_MAGIC: &[u8; 8] = b"RDFNCP01";

pub const FORMAT_VERSION: u32 = 1;
pub const ARENA_HEADER_SIZE: usize = 32;
pub const RECORD_HEADER_SIZE: usize = 32;
pub const COMMIT_MARKER: u64 = 0x5244_464E_434F_4D4D;

#[derive(Debug, Clone)]
pub struct RecordHeader {
    pub type_id: TypeId,
    pub sequence: u64,
    pub payload_len: u32,
    pub checksum: u32,
}

pub fn checksum(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

pub fn encode_record_header(header: &RecordHeader) -> [u8; RECORD_HEADER_SIZE] {
    let mut output = [0u8; RECORD_HEADER_SIZE];
    output[0..4].copy_from_slice(RECORD_MAGIC);
    output[4..6].copy_from_slice(&1u16.to_le_bytes());
    output[8..12].copy_from_slice(&header.type_id.to_le_bytes());
    output[12..20].copy_from_slice(&header.sequence.to_le_bytes());
    output[20..24].copy_from_slice(&header.payload_len.to_le_bytes());
    output[24..28].copy_from_slice(&header.checksum.to_le_bytes());
    output
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub used: u64,
    pub next_sequence: u64,
}

impl Checkpoint {
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let mut bytes = Vec::with_capacity(28);
        bytes.extend_from_slice(CHECKPOINT_MAGIC);
        bytes.extend_from_slice(&self.used.to_le_bytes());
        bytes.extend_from_slice(&self.next_sequence.to_le_bytes());

        let crc = checksum(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());

        let temp = path.with_extension("checkpoint.tmp");
        {
            let mut file = std::fs::File::create(&temp)?;
            use std::io::Write;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path)?;
        if bytes.len() != 28 || &bytes[0..8] != CHECKPOINT_MAGIC {
            return Ok(None);
        }
        let stored_crc = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        if checksum(&bytes[0..24]) != stored_crc {
            return Ok(None);
        }
        Ok(Some(Self {
            used: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            next_sequence: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        }))
    }
}