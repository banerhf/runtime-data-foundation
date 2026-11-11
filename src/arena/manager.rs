use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::{Condvar, Mutex};

use crate::error::{FoundationError, Result};
use crate::locator::{ArenaId, DataLocator, SystemId, TypeId};
use crate::memory::reservation::MemoryManager;
use crate::storage::{OverflowPolicy, StorageEngine};
use crate::storage::persistent_mmap::PersistentMmap;
use crate::storage::tmp_ring::TmpRingBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaStatus {
    Active,
    Closing,
    Closed,
    Failed,
}

pub struct ArenaRuntimeState {
    pub status: ArenaStatus,
    pub active_operations: usize,
    pub storage: Option<Arc<dyn StorageEngine>>,
}

pub struct ArenaEntry {
    pub id: ArenaId,
    pub state: Mutex<ArenaRuntimeState>,
    pub changed: Condvar,
}

impl ArenaEntry {
    pub fn new(id: ArenaId, storage: Arc<dyn StorageEngine>) -> Arc<Self> {
        Arc::new(Self {
            id,
            state: Mutex::new(ArenaRuntimeState {
                status: ArenaStatus::Active,
                active_operations: 0,
                storage: Some(storage),
            }),
            changed: Condvar::new(),
        })
    }

    pub fn begin(self: &Arc<Self>) -> Result<(Arc<dyn StorageEngine>, ArenaOperation)> {
        let mut state = self.state.lock();
        if state.status != ArenaStatus::Active {
            return Err(FoundationError::ArenaNotActive);
        }
        let storage = state.storage.as_ref().cloned().ok_or(FoundationError::ArenaNotActive)?;
        state.active_operations += 1;
        Ok((storage, ArenaOperation { entry: self.clone() }))
    }

    pub fn close(&self) -> Result<()> {
        let mut state = self.state.lock();
        match state.status {
            ArenaStatus::Closed => return Ok(()),
            ArenaStatus::Failed => return Err(FoundationError::ArenaNotActive),
            ArenaStatus::Active => state.status = ArenaStatus::Closing,
            ArenaStatus::Closing => {}
        }
        while state.active_operations > 0 {
            self.changed.wait(&mut state);
        }
        state.storage.take();
        state.status = ArenaStatus::Closed;
        self.changed.notify_all();
        Ok(())
    }

    pub fn status(&self) -> ArenaStatus {
        self.state.lock().status
    }
}

pub struct ArenaOperation {
    entry: Arc<ArenaEntry>,
}

impl Drop for ArenaOperation {
    fn drop(&mut self) {
        let mut state = self.entry.state.lock();
        state.active_operations = state.active_operations.saturating_sub(1);
        if state.active_operations == 0 {
            self.entry.changed.notify_all();
        }
    }
}

#[derive(Clone)]
pub struct ArenaHandle {
    entry: Arc<ArenaEntry>,
}

impl ArenaHandle {
    pub fn new(entry: Arc<ArenaEntry>) -> Self {
        Self { entry }
    }

    pub fn append(&self, type_id: TypeId, data: &[u8]) -> Result<DataLocator> {
        let (storage, _op) = self.entry.begin()?;
        storage.append(type_id, data)
    }

    pub fn read(&self, locator: &DataLocator) -> Result<Arc<Vec<u8>>> {
        let (storage, _op) = self.entry.begin()?;
        storage.read(locator)
    }

    pub fn flush(&self) -> Result<()> {
        let (storage, _op) = self.entry.begin()?;
        storage.flush()
    }

    pub fn arena_id(&self) -> ArenaId {
        self.entry.id
    }

    pub fn status(&self) -> ArenaStatus {
        self.entry.status()
    }
}

pub struct ArenaManager {
    system_id: SystemId,
    base_path: PathBuf,
    memory_manager: Arc<MemoryManager>,
    arenas: Mutex<HashMap<ArenaId, Arc<ArenaEntry>>>,
}

impl ArenaManager {
    pub fn new(system_id: SystemId, base_path: impl AsRef<Path>, memory_manager: Arc<MemoryManager>) -> Self {
        Self {
            system_id,
            base_path: base_path.as_ref().to_path_buf(),
            memory_manager,
            arenas: Mutex::new(HashMap::new()),
        }
    }

    pub fn create_tmp_arena(&self, arena_id: ArenaId, generation: u32, capacity: usize, overflow: OverflowPolicy) -> Result<ArenaHandle> {
        let mut arenas = self.arenas.lock();
        if arenas.contains_key(&arena_id) {
            return Err(FoundationError::Storage(format!("arena {} already exists", arena_id)));
        }
        let storage = Arc::new(TmpRingBuffer::new(self.system_id, arena_id, generation, capacity, overflow));
        let entry = ArenaEntry::new(arena_id, storage);
        arenas.insert(arena_id, entry.clone());
        Ok(ArenaHandle::new(entry))
    }

    pub fn create_persistent_arena(&self, arena_id: ArenaId, generation: u32, initial_capacity: usize, growth_size: usize) -> Result<ArenaHandle> {
        let mut arenas = self.arenas.lock();
        if arenas.contains_key(&arena_id) {
            return Err(FoundationError::Storage(format!("arena {} already exists", arena_id)));
        }
        let path = self.base_path.join(format!("arena_{}.rdf", arena_id));
        let storage = Arc::new(PersistentMmap::open(self.system_id, arena_id, generation, &path, initial_capacity, growth_size)?);
        let entry = ArenaEntry::new(arena_id, storage);
        arenas.insert(arena_id, entry.clone());
        Ok(ArenaHandle::new(entry))
    }

    pub fn open_persistent_arena(&self, arena_id: ArenaId, generation: u32, initial_capacity: usize, growth_size: usize) -> Result<ArenaHandle> {
        let mut arenas = self.arenas.lock();
        if let Some(entry) = arenas.get(&arena_id) {
            return Ok(ArenaHandle::new(entry.clone()));
        }
        let path = self.base_path.join(format!("arena_{}.rdf", arena_id));
        let storage = Arc::new(PersistentMmap::open(self.system_id, arena_id, generation, &path, initial_capacity, growth_size)?);
        let entry = ArenaEntry::new(arena_id, storage);
        arenas.insert(arena_id, entry.clone());
        Ok(ArenaHandle::new(entry))
    }

    pub fn close_arena(&self, arena_id: ArenaId) -> Result<()> {
        let entry = {
            let arenas = self.arenas.lock();
            arenas.get(&arena_id).cloned().ok_or(FoundationError::ArenaNotFound(arena_id))?
        };
        entry.close()
    }

    pub fn delete_arena(&self, arena_id: ArenaId) -> Result<()> {
        let entry = {
            let mut arenas = self.arenas.lock();
            arenas.remove(&arena_id).ok_or(FoundationError::ArenaNotFound(arena_id))?
        };
        entry.close()?;
        let path = self.base_path.join(format!("arena_{}.rdf", arena_id));
        if path.exists() { fs::remove_file(path)?; }
        let checkpoint_path = self.base_path.join(format!("arena_{}.checkpoint", arena_id));
        if checkpoint_path.exists() { fs::remove_file(checkpoint_path)?; }
        Ok(())
    }

    pub fn get_handle(&self, arena_id: ArenaId) -> Result<ArenaHandle> {
        let arenas = self.arenas.lock();
        let entry = arenas.get(&arena_id).cloned().ok_or(FoundationError::ArenaNotFound(arena_id))?;
        Ok(ArenaHandle::new(entry))
    }
}