use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::{AgilangError, ErrorCode, RuntimeResult};

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgilangType {
    Invalid = 0,
    Null = 1,
    Bool = 2,
    Int = 3,
    UInt = 4,
    Float = 5,
    String = 6,
    Bytes = 7,
    Array = 8,
    Map = 9,
    Error = 10,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum AgilangValue {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(Arc<str>),
    Bytes(Bytes),
    Array(Arc<Vec<AgilangValue>>),
    Map(Arc<BTreeMap<String, AgilangValue>>),
    Error(Arc<AgilangError>),
}

impl AgilangValue {
    pub const fn kind(&self) -> AgilangType {
        match self {
            Self::Null => AgilangType::Null,
            Self::Bool(_) => AgilangType::Bool,
            Self::Int(_) => AgilangType::Int,
            Self::UInt(_) => AgilangType::UInt,
            Self::Float(_) => AgilangType::Float,
            Self::String(_) => AgilangType::String,
            Self::Bytes(_) => AgilangType::Bytes,
            Self::Array(_) => AgilangType::Array,
            Self::Map(_) => AgilangType::Map,
            Self::Error(_) => AgilangType::Error,
        }
    }

    pub fn as_bool(&self) -> RuntimeResult<bool> {
        match self {
            Self::Bool(v) => Ok(*v),
            _ => Err(self.mismatch("bool")),
        }
    }
    pub fn as_i64(&self) -> RuntimeResult<i64> {
        match self {
            Self::Int(v) => Ok(*v),
            _ => Err(self.mismatch("int")),
        }
    }
    pub fn as_u64(&self) -> RuntimeResult<u64> {
        match self {
            Self::UInt(v) => Ok(*v),
            _ => Err(self.mismatch("uint")),
        }
    }
    pub fn as_f64(&self) -> RuntimeResult<f64> {
        match self {
            Self::Float(v) => Ok(*v),
            _ => Err(self.mismatch("float")),
        }
    }
    pub fn as_str(&self) -> RuntimeResult<&str> {
        match self {
            Self::String(v) => Ok(v),
            _ => Err(self.mismatch("string")),
        }
    }
    pub fn as_bytes(&self) -> RuntimeResult<&[u8]> {
        match self {
            Self::Bytes(v) => Ok(v),
            _ => Err(self.mismatch("bytes")),
        }
    }
    pub fn as_array(&self) -> RuntimeResult<&[AgilangValue]> {
        match self {
            Self::Array(v) => Ok(v),
            _ => Err(self.mismatch("array")),
        }
    }
    pub fn as_map(&self) -> RuntimeResult<&BTreeMap<String, AgilangValue>> {
        match self {
            Self::Map(v) => Ok(v),
            _ => Err(self.mismatch("map")),
        }
    }

    fn mismatch(&self, expected: &str) -> AgilangError {
        AgilangError::type_mismatch(expected, format!("{:?}", self.kind()))
    }

    pub fn array(values: Vec<AgilangValue>) -> Self {
        Self::Array(Arc::new(values))
    }
    pub fn map(values: BTreeMap<String, AgilangValue>) -> Self {
        Self::Map(Arc::new(values))
    }

    pub fn array_push(&self, value: AgilangValue) -> RuntimeResult<Self> {
        let mut values = self.as_array()?.to_vec();
        values.push(value);
        Ok(Self::array(values))
    }

    pub fn array_get(&self, index: usize) -> RuntimeResult<Self> {
        let values = self.as_array()?;
        values
            .get(index)
            .cloned()
            .ok_or_else(|| AgilangError::out_of_bounds(index, values.len()))
    }

    pub fn map_insert(&self, key: String, value: AgilangValue) -> RuntimeResult<Self> {
        let mut values = self.as_map()?.clone();
        values.insert(key, value);
        Ok(Self::map(values))
    }

    pub fn map_get(&self, key: &str) -> RuntimeResult<Self> {
        self.as_map()?.get(key).cloned().ok_or_else(|| {
            AgilangError::new(ErrorCode::NotFound, "map key was not found").with_context("key", key)
        })
    }

    pub fn len(&self) -> RuntimeResult<usize> {
        match self {
            Self::String(v) => Ok(v.chars().count()),
            Self::Bytes(v) => Ok(v.len()),
            Self::Array(v) => Ok(v.len()),
            Self::Map(v) => Ok(v.len()),
            _ => Err(self.mismatch("string, bytes, array, or map")),
        }
    }

    pub fn is_empty(&self) -> RuntimeResult<bool> {
        match self {
            Self::String(v) => Ok(v.is_empty()),
            Self::Bytes(v) => Ok(v.is_empty()),
            Self::Array(v) => Ok(v.is_empty()),
            Self::Map(v) => Ok(v.is_empty()),
            _ => Err(self.mismatch("string, bytes, array, or map")),
        }
    }

    pub fn to_json(&self) -> RuntimeResult<String> {
        serde_json::to_string(self)
            .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))
    }

    pub fn from_json(text: &str) -> RuntimeResult<Self> {
        serde_json::from_str(text)
            .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))
    }
}

impl From<bool> for AgilangValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i64> for AgilangValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<u64> for AgilangValue {
    fn from(v: u64) -> Self {
        Self::UInt(v)
    }
}
impl From<f64> for AgilangValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}
impl From<String> for AgilangValue {
    fn from(v: String) -> Self {
        Self::String(Arc::from(v))
    }
}
impl From<&str> for AgilangValue {
    fn from(v: &str) -> Self {
        Self::String(Arc::from(v))
    }
}
impl From<Vec<u8>> for AgilangValue {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(Bytes::from(v))
    }
}
