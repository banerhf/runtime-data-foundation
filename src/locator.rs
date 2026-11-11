use serde::{Deserialize, Serialize};

pub type SystemId = u32;
pub type TypeId = u32;
pub type ArenaId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DataLocator {
    pub system_id: SystemId,
    pub type_id: TypeId,
    pub arena_id: ArenaId,
    pub generation: u32,
    pub offset: u64,
    pub length: u32,
    pub sequence: u64,
}