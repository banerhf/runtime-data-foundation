use crate::error::{FoundationError, Result};
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