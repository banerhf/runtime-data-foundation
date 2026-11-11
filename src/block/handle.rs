use std::sync::Arc;
use crate::arena::manager::ArenaHandle;
use crate::error::Result;
use crate::locator::DataLocator;
use crate::memory::reservation::MemoryReservation;
use super::lease::DataBlockLease;

#[derive(Clone)]
pub struct DataBlockHandle {
    locator: DataLocator,
    arena_handle: ArenaHandle,
}

impl DataBlockHandle {
    pub fn new(locator: DataLocator, arena_handle: ArenaHandle) -> Self {
        Self {
            locator,
            arena_handle,
        }
    }

    pub fn locator(&self) -> &DataLocator {
        &self.locator
    }

    pub fn acquire_lease(&self, reservation: MemoryReservation) -> Result<DataBlockLease<Vec<u8>>> {
        let raw_bytes = self.arena_handle.read(&self.locator)?;
        Ok(DataBlockLease::new(raw_bytes, reservation))
    }

    pub fn flush(&self) -> Result<()> {
        self.arena_handle.flush()
    }
}