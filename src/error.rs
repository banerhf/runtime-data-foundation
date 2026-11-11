use thiserror::Error;

#[derive(Debug, Error)]
pub enum FoundationError {
    #[error("system not found")]
    SystemNotFound,

    #[error("schema not found: {0}")]
    SchemaNotFound(u32),

    #[error("arena not found: {0}")]
    ArenaNotFound(u64),

    #[error("arena is not active")]
    ArenaNotActive,

    #[error("arena is busy")]
    ArenaBusy,

    #[error("invalid schema: {0}")]
    InvalidSchema(String),

    #[error("invalid data: {0}")]
    InvalidData(String),

    #[error("data expired")]
    DataExpired,

    #[error("generation mismatch")]
    GenerationMismatch,

    #[error("capacity exceeded")]
    CapacityExceeded,

    #[error("memory quota exceeded")]
    MemoryQuotaExceeded,

    #[error("corrupted record at offset {0}")]
    CorruptedRecord(u64),

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("codec error: {0}")]
    Codec(String),

    #[error("storage error: {0}")]
    Storage(String),
}

impl From<std::io::Error> for FoundationError {
    fn from(value: std::io::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, FoundationError>;