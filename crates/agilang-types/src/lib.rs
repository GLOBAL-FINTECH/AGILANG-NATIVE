use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Type {
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    Bool,
    String,
    Bytes,
    Void,
    Never,
    Unknown,
    Error,
    Function(FunctionType),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FunctionType {
    pub params: Vec<Type>,
    pub ret: Box<Type>,
}

impl Type {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(name: &str) -> Self {
        match name {
            "i32" => Self::I32,
            "i64" => Self::I64,
            "u32" => Self::U32,
            "u64" => Self::U64,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "bool" => Self::Bool,
            "string" => Self::String,
            "bytes" => Self::Bytes,
            "void" => Self::Void,
            _ => Self::Unknown,
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::I32 | Self::I64 | Self::U32 | Self::U64 | Self::F32 | Self::F64
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Self::I32 | Self::I64 | Self::U32 | Self::U64)
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn is_compatible(&self, other: &Self) -> bool {
        if self == &Self::Error || other == &Self::Error {
            return true;
        }
        if self.is_integer() && other.is_integer() {
            return true;
        }
        if self.is_float() && other.is_float() {
            return true;
        }
        self == other
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I32 => write!(f, "i32"),
            Self::I64 => write!(f, "i64"),
            Self::U32 => write!(f, "u32"),
            Self::U64 => write!(f, "u64"),
            Self::F32 => write!(f, "f32"),
            Self::F64 => write!(f, "f64"),
            Self::Bool => write!(f, "bool"),
            Self::String => write!(f, "string"),
            Self::Bytes => write!(f, "bytes"),
            Self::Void => write!(f, "void"),
            Self::Never => write!(f, "never"),
            Self::Unknown => write!(f, "unknown"),
            Self::Error => write!(f, "error"),
            Self::Function(func) => {
                write!(f, "(")?;
                for (i, p) in func.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", func.ret)
            }
        }
    }
}
