use std::collections::HashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use crate::error::{FoundationError, Result};
use crate::locator::TypeId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    String,
    I64,
    U64,
    F64,
    Bool,
    Object,
    Array,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub field_type: FieldType,
    pub nullable: bool,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub type_id: TypeId,
    pub name: String,
    pub version: u32,
    pub fields: Vec<FieldSchema>,
}

impl Schema {
    pub fn validate(&self, value: &serde_json::Value) -> Result<()> {
        let object = value.as_object().ok_or_else(|| {
            FoundationError::InvalidData("root JSON value must be an object".into())
        })?;

        for field in &self.fields {
            let Some(value) = object.get(&field.name) else {
                if field.required {
                    return Err(FoundationError::InvalidData(format!(
                        "missing required field: {}",
                        field.name
                    )));
                }
                continue;
            };

            if value.is_null() {
                if !field.nullable {
                    return Err(FoundationError::InvalidData(format!(
                        "field {} cannot be null",
                        field.name
                    )));
                }
                continue;
            }

            let valid = match field.field_type {
                FieldType::String => value.is_string(),
                FieldType::I64 => value.as_i64().is_some(),
                FieldType::U64 => value.as_u64().is_some(),
                FieldType::F64 => value.is_number(),
                FieldType::Bool => value.is_boolean(),
                FieldType::Object => value.is_object(),
                FieldType::Array => value.is_array(),
            };

            if !valid {
                return Err(FoundationError::InvalidData(format!(
                    "invalid type for field {}",
                    field.name
                )));
            }
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct SchemaRegistry {
    schemas: RwLock<HashMap<TypeId, Schema>>,
}

impl SchemaRegistry {
    pub fn register(&self, schema: Schema) -> Result<()> {
        let mut schemas = self.schemas.write();
        if schemas.contains_key(&schema.type_id) {
            return Err(FoundationError::InvalidSchema(format!(
                "type id {} already registered",
                schema.type_id
            )));
        }
        schemas.insert(schema.type_id, schema);
        Ok(())
    }

    pub fn get(&self, type_id: TypeId) -> Result<Schema> {
        self.schemas
            .read()
            .get(&type_id)
            .cloned()
            .ok_or(FoundationError::SchemaNotFound(type_id))
    }
}