pub mod error;
pub mod locator;
pub mod schema;
pub mod arena;
pub mod storage;
pub mod memory;
pub mod block;
pub mod codec;

// Re-export core types for convenient usage
pub use error::{FoundationError, Result};
pub use locator::{DataLocator, SystemId, TypeId, ArenaId};
pub use schema::registry::{Schema, SchemaRegistry, FieldSchema, FieldType};
pub use arena::manager::{ArenaManager, ArenaHandle, ArenaStatus};
pub use storage::{StorageEngine, OverflowPolicy};
pub use storage::tmp_ring::TmpRingBuffer;
pub use storage::persistent_mmap::PersistentMmap;
pub use memory::reservation::{MemoryManager, MemoryReservation};
pub use block::handle::DataBlockHandle;
pub use block::lease::DataBlockLease;

// ============================================================
// Codec helper functions
// ============================================================
use serde::{de::DeserializeOwned, Serialize};

pub fn encode_value(value: &serde_json::Value) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(value).map_err(|e| FoundationError::Codec(e.to_string()))
}

pub fn decode_value(data: &[u8]) -> Result<serde_json::Value> {
    rmp_serde::from_slice(data).map_err(|e| FoundationError::Codec(e.to_string()))
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(value).map_err(|e| FoundationError::Codec(e.to_string()))
}

pub fn decode<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
    rmp_serde::from_slice(data).map_err(|e| FoundationError::Codec(e.to_string()))
}