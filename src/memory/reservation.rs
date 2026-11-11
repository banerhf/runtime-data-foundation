use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use crate::error::{FoundationError, Result};

pub struct MemoryManager {
    quota: usize,
    used: AtomicUsize,
}

impl MemoryManager {
    pub fn new(quota: usize) -> Arc<Self> {
        Arc::new(Self {
            quota,
            used: AtomicUsize::new(0),
        })
    }

    pub fn reserve(self: &Arc<Self>, size: usize) -> Result<MemoryReservation> {
        let mut current = self.used.load(Ordering::Acquire);
        loop {
            let next = current.checked_add(size).ok_or(FoundationError::MemoryQuotaExceeded)?;
            if next > self.quota {
                return Err(FoundationError::MemoryQuotaExceeded);
            }
            match self.used.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Ok(MemoryReservation { manager: self.clone(), size }),
                Err(actual) => current = actual,
            }
        }
    }

    pub fn used(&self) -> usize { self.used.load(Ordering::Acquire) }
    pub fn quota(&self) -> usize { self.quota }

    fn release(&self, size: usize) {
        self.used.fetch_sub(size, Ordering::AcqRel);
    }
}

pub struct MemoryReservation {
    manager: Arc<MemoryManager>,
    size: usize,
}

impl MemoryReservation {
    pub fn size(&self) -> usize { self.size }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.manager.release(self.size);
    }
}