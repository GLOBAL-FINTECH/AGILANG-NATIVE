//! Core value, error, ownership, and handle kernel for the AGILANG native runtime.
mod error;
mod handle;
mod value;
pub use error::{AgilangError, ErrorCode, RuntimeResult};
pub use handle::{HandleId, HandleRegistry, RuntimeObject};
pub use value::{AgilangType, AgilangValue};
