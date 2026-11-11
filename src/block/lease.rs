use std::sync::Arc;
use crate::memory::reservation::MemoryReservation;

pub struct DataBlockLease<T> {
    data: Arc<T>,
    _reservation: MemoryReservation,
}

impl<T> DataBlockLease<T> {
    pub fn new(data: Arc<T>, reservation: MemoryReservation) -> Self {
        Self {
            data,
            _reservation: reservation,
        }
    }

    pub fn data(&self) -> &Arc<T> {
        &self.data
    }

    pub fn into_inner(self) -> (Arc<T>, MemoryReservation) {
        (self.data, self._reservation)
    }

    pub fn memory_size(&self) -> usize {
        self._reservation.size()
    }
}