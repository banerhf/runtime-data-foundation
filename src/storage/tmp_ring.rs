use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::Mutex;
use crate::error::{FoundationError, Result};
use crate::locator::{ArenaId, DataLocator, SystemId, TypeId};
use super::{OverflowPolicy, StorageEngine};

struct TmpRecord {
    sequence: u64,
    type_id: TypeId,
    data: Arc<Vec<u8>>,
}

struct TmpState {
    records: VecDeque<TmpRecord>,
    used: usize,
    next_sequence: u64,
}

pub struct TmpRingBuffer {
    system_id: SystemId,
    arena_id: ArenaId,
    generation: u32,
    capacity: usize,
    overflow: OverflowPolicy,
    state: Mutex<TmpState>,
}

impl TmpRingBuffer {
    pub fn new(
        system_id: SystemId,
        arena_id: ArenaId,
        generation: u32,
        capacity: usize,
        overflow: OverflowPolicy,
    ) -> Self {
        Self {
            system_id,
            arena_id,
            generation,
            capacity,
            overflow,
            state: Mutex::new(TmpState {
                records: VecDeque::new(),
                used: 0,
                next_sequence: 1,
            }),
        }
    }
}

impl StorageEngine for TmpRingBuffer {
    fn system_id(&self) -> SystemId { self.system_id }
    fn arena_id(&self) -> ArenaId { self.arena_id }
    fn generation(&self) -> u32 { self.generation }

    fn append(&self, type_id: TypeId, data: &[u8]) -> Result<DataLocator> {
        if data.len() > self.capacity || data.len() > u32::MAX as usize {
            return Err(FoundationError::CapacityExceeded);
        }

        let mut state = self.state.lock();

        while state.used + data.len() > self.capacity {
            match self.overflow {
                OverflowPolicy::RejectNew => return Err(FoundationError::CapacityExceeded),
                OverflowPolicy::OverwriteOldest => {
                    let old = state.records.pop_front().ok_or(FoundationError::CapacityExceeded)?;
                    state.used -= old.data.len();
                }
            }
        }

        let sequence = state.next_sequence;
        state.next_sequence += 1;

        let bytes = Arc::new(data.to_vec());
        let length = bytes.len();
        state.used += length;

        state.records.push_back(TmpRecord {
            sequence,
            type_id,
            data: bytes,
        });

        Ok(DataLocator {
            system_id: self.system_id,
            type_id,
            arena_id: self.arena_id,
            generation: self.generation,
            offset: 0,
            length: length as u32,
            sequence,
        })
    }

    fn read(&self, locator: &DataLocator) -> Result<Arc<Vec<u8>>> {
        if locator.system_id != self.system_id || locator.arena_id != self.arena_id {
            return Err(FoundationError::ArenaNotFound(locator.arena_id));
        }
        if locator.generation != self.generation {
            return Err(FoundationError::GenerationMismatch);
        }

        let state = self.state.lock();
        let record = state
            .records
            .iter()
            .find(|r| r.sequence == locator.sequence && r.type_id == locator.type_id)
            .ok_or(FoundationError::DataExpired)?;

        Ok(record.data.clone())
    }

    fn flush(&self) -> Result<()> { Ok(()) }
    fn used_bytes(&self) -> usize { self.state.lock().used }
    fn capacity(&self) -> usize { self.capacity }
}