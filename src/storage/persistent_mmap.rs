use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use memmap2::{MmapMut, MmapOptions};
use parking_lot::Mutex;
use crate::error::{FoundationError, Result};
use crate::locator::{ArenaId, DataLocator, SystemId, TypeId};
use super::{ARENA_HEADER_SIZE, ARENA_MAGIC, COMMIT_MARKER, FORMAT_VERSION, RECORD_HEADER_SIZE, RECORD_MAGIC, RecordHeader, Checkpoint, StorageEngine};

struct PersistentState {
    file: File,
    mmap: MmapMut,
    capacity: usize,
    used: usize,
    next_sequence: u64,
}

pub struct PersistentMmap {
    system_id: SystemId,
    arena_id: ArenaId,
    generation: u32,
    path: PathBuf,
    checkpoint_path: PathBuf,
    growth_size: usize,
    state: Mutex<PersistentState>,
}

impl PersistentMmap {
    pub fn open(
        system_id: SystemId,
        arena_id: ArenaId,
        generation: u32,
        path: impl AsRef<Path>,
        initial_capacity: usize,
        growth_size: usize,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let checkpoint_path = path.with_extension("checkpoint");

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let original_len = file.metadata()?.len() as usize;
        let is_new = original_len == 0;
        let capacity = if is_new {
            initial_capacity.max(4096).max(ARENA_HEADER_SIZE + RECORD_HEADER_SIZE + 8)
        } else {
            original_len
        };

        if is_new {
            file.set_len(capacity as u64)?;
        }

        let mut mmap = unsafe { MmapOptions::new().len(capacity).map_mut(&file)? };

        if is_new {
            mmap[0..8].copy_from_slice(ARENA_MAGIC);
            mmap[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
            mmap[12..16].copy_from_slice(&system_id.to_le_bytes());
            mmap[16..20].copy_from_slice(&arena_id.to_le_bytes()[0..4]);
            mmap[20..24].copy_from_slice(&generation.to_le_bytes());
            mmap[24..32].copy_from_slice(&0u64.to_le_bytes());
            mmap.flush()?;
            file.sync_all()?;
        } else {
            if capacity < ARENA_HEADER_SIZE || &mmap[0..8] != ARENA_MAGIC {
                return Err(FoundationError::Storage("invalid arena header".into()));
            }
            let version = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
            let stored_system = u32::from_le_bytes(mmap[12..16].try_into().unwrap());
            let stored_arena = u32::from_le_bytes(mmap[16..20].try_into().unwrap());
            let stored_generation = u32::from_le_bytes(mmap[20..24].try_into().unwrap());

            if version != FORMAT_VERSION || stored_system != system_id || stored_arena != arena_id as u32 || stored_generation != generation {
                return Err(FoundationError::Storage("arena identity or format mismatch".into()));
            }
        }

        let _checkpoint = Checkpoint::load(&checkpoint_path)?;
        let (used, next_sequence) = Self::recover_records(&mmap)?;

        if used < capacity {
            mmap[used..capacity].fill(0);
            mmap.flush()?;
            file.sync_data()?;
        }

        Ok(Self {
            system_id,
            arena_id,
            generation,
            path,
            checkpoint_path,
            growth_size: growth_size.max(4096),
            state: Mutex::new(PersistentState {
                file,
                mmap,
                capacity,
                used,
                next_sequence,
            }),
        })
    }

    fn recover_records(mmap: &MmapMut) -> Result<(usize, u64)> {
        let capacity = mmap.len();
        let mut offset = ARENA_HEADER_SIZE;
        let mut next_sequence = 1u64;

        while offset + RECORD_HEADER_SIZE <= capacity {
            let header = &mmap[offset..offset + RECORD_HEADER_SIZE];
            if header.iter().all(|b| *b == 0) { break; }
            if &header[0..4] != RECORD_MAGIC { break; }

            let version = u16::from_le_bytes(header[4..6].try_into().unwrap());
            if version != 1 { break; }

            let payload_len = u32::from_le_bytes(header[20..24].try_into().unwrap()) as usize;
            let stored_checksum = u32::from_le_bytes(header[24..28].try_into().unwrap());
            let sequence = u64::from_le_bytes(header[12..20].try_into().unwrap());

            let total = RECORD_HEADER_SIZE
                .checked_add(payload_len)
                .and_then(|v| v.checked_add(8))
                .ok_or(FoundationError::CorruptedRecord(offset as u64))?;

            if offset.checked_add(total).filter(|end| *end <= capacity).is_none() {
                break;
            }

            let payload_start = offset + RECORD_HEADER_SIZE;
            let payload_end = payload_start + payload_len;
            let payload = &mmap[payload_start..payload_end];

            let marker = u64::from_le_bytes(mmap[payload_end..payload_end + 8].try_into().unwrap());
            if marker != COMMIT_MARKER { break; }
            if super::checksum(payload) != stored_checksum { break; }

            if sequence < next_sequence {
                return Err(FoundationError::CorruptedRecord(offset as u64));
            }

            next_sequence = sequence + 1;
            offset += total;
        }

        Ok((offset, next_sequence))
    }

    fn grow(state: &mut PersistentState, required: usize, growth: usize) -> Result<()> {
        while state.capacity < required {
            state.capacity = state.capacity.checked_add(growth).ok_or(FoundationError::CapacityExceeded)?;
        }
        state.file.set_len(state.capacity as u64)?;
        state.mmap = unsafe { MmapOptions::new().len(state.capacity).map_mut(&state.file)? };
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<()> {
        let state = self.state.lock();
        state.mmap.flush()?;
        state.file.sync_data()?;
        Checkpoint { used: state.used as u64, next_sequence: state.next_sequence }.save(&self.checkpoint_path)?;
        Ok(())
    }
}

impl StorageEngine for PersistentMmap {
    fn system_id(&self) -> SystemId { self.system_id }
    fn arena_id(&self) -> ArenaId { self.arena_id }
    fn generation(&self) -> u32 { self.generation }

    fn append(&self, type_id: TypeId, data: &[u8]) -> Result<DataLocator> {
        if data.len() > u32::MAX as usize {
            return Err(FoundationError::CapacityExceeded);
        }

        let mut state = self.state.lock();
        let total = RECORD_HEADER_SIZE
            .checked_add(data.len())
            .and_then(|v| v.checked_add(8))
            .ok_or(FoundationError::CapacityExceeded)?;

        let end = state.used.checked_add(total).ok_or(FoundationError::CapacityExceeded)?;
        if end > state.capacity {
            Self::grow(&mut state, end, self.growth_size)?;
        }

        let offset = state.used;
        let sequence = state.next_sequence;
        let header = super::encode_record_header(&super::RecordHeader {
            type_id,
            sequence,
            payload_len: data.len() as u32,
            checksum: super::checksum(data),
        });

        let payload_start = offset + RECORD_HEADER_SIZE;
        let payload_end = payload_start + data.len();
        let marker_end = payload_end + 8;

        state.mmap[offset..offset + RECORD_HEADER_SIZE].copy_from_slice(&header);
        state.mmap[payload_start..payload_end].copy_from_slice(data);
        state.mmap.flush()?;
        state.file.sync_data()?;

        state.mmap[payload_end..marker_end].copy_from_slice(&COMMIT_MARKER.to_le_bytes());
        state.mmap.flush()?;
        state.file.sync_data()?;

        state.used = end;
        state.next_sequence += 1;

        Ok(DataLocator {
            system_id: self.system_id,
            type_id,
            arena_id: self.arena_id,
            generation: self.generation,
            offset: offset as u64,
            length: data.len() as u32,
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
        let offset = locator.offset as usize;
        let payload_start = offset.checked_add(RECORD_HEADER_SIZE).ok_or(FoundationError::DataExpired)?;
        let payload_end = payload_start.checked_add(locator.length as usize).ok_or(FoundationError::DataExpired)?;
        let record_end = payload_end.checked_add(8).ok_or(FoundationError::DataExpired)?;

        if offset < ARENA_HEADER_SIZE || record_end > state.used {
            return Err(FoundationError::DataExpired);
        }

        let header = &state.mmap[offset..offset + RECORD_HEADER_SIZE];
        if &header[0..4] != RECORD_MAGIC {
            return Err(FoundationError::DataExpired);
        }

        let type_id = u32::from_le_bytes(header[8..12].try_into().unwrap());
        let sequence = u64::from_le_bytes(header[12..20].try_into().unwrap());
        let length = u32::from_le_bytes(header[20..24].try_into().unwrap());
        let stored_checksum = u32::from_le_bytes(header[24..28].try_into().unwrap());
        let marker = u64::from_le_bytes(state.mmap[payload_end..record_end].try_into().unwrap());

        if type_id != locator.type_id || sequence != locator.sequence || length != locator.length || marker != COMMIT_MARKER {
            return Err(FoundationError::DataExpired);
        }

        let payload = state.mmap[payload_start..payload_end].to_vec();
        if super::checksum(&payload) != stored_checksum {
            return Err(FoundationError::CorruptedRecord(offset as u64));
        }

        Ok(Arc::new(payload))
    }

    fn flush(&self) -> Result<()> { self.checkpoint() }
    fn used_bytes(&self) -> usize { self.state.lock().used }
    fn capacity(&self) -> usize { self.state.lock().capacity }
}