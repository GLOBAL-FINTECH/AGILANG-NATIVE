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
    List(Box<Type>),
    Vector,
    Matrix,
    Complex,
    Optional(Box<Type>),
    Void,
    Never,
    Unknown,
    Error,
    Struct(String),
    Enum(String),
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
        let normalized = name.trim();

        if let Some(inner) = Self::generic_argument(normalized, "array")
            .or_else(|| Self::generic_argument(normalized, "list"))
        {
            return Self::List(Box::new(Self::from_str(inner)));
        }

        if let Some(inner) = normalized.strip_suffix("[]") {
            return Self::List(Box::new(Self::from_str(inner)));
        }

        if let Some(inner) = Self::generic_argument(normalized, "optional") {
            return Self::Optional(Box::new(Self::from_str(inner)));
        }

        match normalized {
            "i32" => Self::I32,
            "i64" => Self::I64,
            "int" => Self::I64,
            "u32" => Self::U32,
            "u64" => Self::U64,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "bool" => Self::Bool,
            "string" => Self::String,
            "bytes" => Self::Bytes,
            "array" | "list" => Self::List(Box::new(Self::Unknown)),
            "vector" => Self::Vector,
            "matrix" => Self::Matrix,
            "complex" => Self::Complex,
            "void" => Self::Void,
            "never" => Self::Never,
            _ => Self::Unknown,
        }
    }

    fn generic_argument<'a>(name: &'a str, constructor: &str) -> Option<&'a str> {
        let prefix = format!("{constructor}<");
        let inner = name.strip_prefix(&prefix)?.strip_suffix('>')?.trim();
        if inner.is_empty() {
            None
        } else {
            Some(inner)
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

    pub fn is_array(&self) -> bool {
        matches!(self, Self::List(_))
    }

    pub fn array_element(&self) -> Option<&Type> {
        match self {
            Self::List(inner) => Some(inner.as_ref()),
            _ => None,
        }
    }

    pub fn is_compatible(&self, other: &Self) -> bool {
        if self == &Self::Error || other == &Self::Error {
            return true;
        }
        if self == &Self::Unknown || other == &Self::Unknown {
            return true;
        }
        if self.is_integer() && other.is_integer() {
            return true;
        }
        if self.is_float() && other.is_float() {
            return true;
        }
        match (self, other) {
            (Self::List(left), Self::List(right)) => {
                left.as_ref() == &Self::Unknown
                    || right.as_ref() == &Self::Unknown
                    || left.is_compatible(right)
            }
            (Self::Optional(left), Self::Optional(right)) => {
                left.as_ref() == &Self::Unknown
                    || right.as_ref() == &Self::Unknown
                    || left.is_compatible(right)
            }
            (Self::Struct(left), Self::Struct(right)) => left == right,
            (Self::Enum(left), Self::Enum(right)) => left == right,
            _ => self == other,
        }
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
            Self::List(inner) => write!(f, "array<{}>", inner),
            Self::Vector => write!(f, "vector"),
            Self::Matrix => write!(f, "matrix"),
            Self::Complex => write!(f, "complex"),
            Self::Optional(inner) => write!(f, "optional<{}>", inner),
            Self::Void => write!(f, "void"),
            Self::Never => write!(f, "never"),
            Self::Unknown => write!(f, "unknown"),
            Self::Error => write!(f, "error"),
            Self::Struct(name) => write!(f, "{name}"),
            Self::Enum(name) => write!(f, "{name}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native_array_aliases() {
        assert_eq!(Type::from_str("array"), Type::List(Box::new(Type::Unknown)));
        assert_eq!(Type::from_str("list"), Type::List(Box::new(Type::Unknown)));
        assert_eq!(Type::from_str("array<f64>"), Type::List(Box::new(Type::F64)));
        assert_eq!(Type::from_str("list<i64>"), Type::List(Box::new(Type::I64)));
        assert_eq!(Type::from_str("f64[]"), Type::List(Box::new(Type::F64)));
    }

    #[test]
    fn parses_nested_native_arrays() {
        assert_eq!(
            Type::from_str("array<array<f64>>"),
            Type::List(Box::new(Type::List(Box::new(Type::F64))))
        );
    }

    #[test]
    fn displays_arrays_using_agilang_native_syntax() {
        let ty = Type::List(Box::new(Type::I64));
        assert_eq!(ty.to_string(), "array<i64>");
    }

    #[test]
    fn unknown_arrays_accept_inferred_element_types() {
        let untyped = Type::List(Box::new(Type::Unknown));
        let typed = Type::List(Box::new(Type::F64));
        assert!(untyped.is_compatible(&typed));
    }
}
