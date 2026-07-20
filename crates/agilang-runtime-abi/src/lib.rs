//! Versioned, panic-safe C ABI for compiler-generated AGILANG programs.

#![allow(clippy::missing_safety_doc)]
#![allow(clippy::manual_c_str_literals)]

use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::c_char,
    panic::{catch_unwind, AssertUnwindSafe},
    ptr, slice, str,
};

use agilang_runtime_core::{
    AgilangError, AgilangType, AgilangValue, ErrorCode, HandleId, HandleRegistry, RuntimeResult,
};
use once_cell::sync::Lazy;

pub const ABI_VERSION_MAJOR: u16 = 1;
pub const ABI_VERSION_MINOR: u16 = 0;
pub const ABI_VERSION_PATCH: u16 = 0;

static REGISTRY: Lazy<HandleRegistry> = Lazy::new(HandleRegistry::new);

thread_local! {
    static LAST_ERROR: RefCell<Option<AgilangError>> = const { RefCell::new(None) };
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AgiBuffer {
    pub data: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl AgiBuffer {
    fn empty() -> Self {
        Self {
            data: ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }
    fn from_vec(mut value: Vec<u8>) -> Self {
        let out = Self {
            data: value.as_mut_ptr(),
            len: value.len(),
            capacity: value.capacity(),
        };
        std::mem::forget(value);
        out
    }
}

fn set_error(error: AgilangError) -> u32 {
    let code = error.code as u32;
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(error));
    code
}

fn clear_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
}

fn status(result: RuntimeResult<()>) -> u32 {
    match result {
        Ok(()) => {
            clear_error();
            ErrorCode::Ok as u32
        }
        Err(e) => set_error(e),
    }
}

fn ffi_status<F>(f: F) -> u32
where
    F: FnOnce() -> RuntimeResult<()>,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => status(result),
        Err(_) => set_error(AgilangError::new(
            ErrorCode::Panic,
            "native runtime panic was contained at the ABI boundary",
        )),
    }
}

fn ffi_handle<F>(f: F) -> u64
where
    F: FnOnce() -> RuntimeResult<HandleId>,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(id)) => {
            clear_error();
            id.0
        }
        Ok(Err(e)) => {
            set_error(e);
            0
        }
        Err(_) => {
            set_error(AgilangError::new(
                ErrorCode::Panic,
                "native runtime panic was contained at the ABI boundary",
            ));
            0
        }
    }
}

unsafe fn input_bytes<'a>(data: *const u8, len: usize) -> RuntimeResult<&'a [u8]> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(AgilangError::invalid_argument("input pointer is null"));
    }
    Ok(slice::from_raw_parts(data, len))
}

unsafe fn input_text<'a>(data: *const u8, len: usize) -> RuntimeResult<&'a str> {
    str::from_utf8(input_bytes(data, len)?)
        .map_err(|e| AgilangError::new(ErrorCode::Utf8, e.to_string()))
}

unsafe fn write_handle(out: *mut u64, value: AgilangValue) -> RuntimeResult<()> {
    if out.is_null() {
        return Err(AgilangError::invalid_argument(
            "output handle pointer is null",
        ));
    }
    *out = REGISTRY.insert_value(value).0;
    Ok(())
}

unsafe fn write_buffer(out: *mut AgiBuffer, bytes: Vec<u8>) -> RuntimeResult<()> {
    if out.is_null() {
        return Err(AgilangError::invalid_argument(
            "output buffer pointer is null",
        ));
    }
    *out = AgiBuffer::from_vec(bytes);
    Ok(())
}

#[no_mangle]
pub extern "C" fn agi_runtime_abi_version() -> u32 {
    ((ABI_VERSION_MAJOR as u32) << 16)
        | ((ABI_VERSION_MINOR as u32) << 8)
        | ABI_VERSION_PATCH as u32
}

#[no_mangle]
pub extern "C" fn agi_runtime_live_handles() -> usize {
    REGISTRY.len()
}

#[no_mangle]
pub extern "C" fn agi_runtime_reset() {
    REGISTRY.clear();
    clear_error();
}

#[no_mangle]
pub extern "C" fn agi_runtime_last_error_code() -> u32 {
    LAST_ERROR.with(|slot| slot.borrow().as_ref().map(|e| e.code as u32).unwrap_or(0))
}

#[no_mangle]
pub unsafe extern "C" fn agi_runtime_last_error_json(out: *mut AgiBuffer) -> u32 {
    ffi_status(|| {
        let json = LAST_ERROR
            .with(|slot| serde_json::to_vec(&*slot.borrow()))
            .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))?;
        write_buffer(out, json)
    })
}

#[no_mangle]
pub extern "C" fn agi_value_null() -> u64 {
    REGISTRY.insert_value(AgilangValue::Null).0
}
#[no_mangle]
pub extern "C" fn agi_value_bool(value: bool) -> u64 {
    REGISTRY.insert_value(AgilangValue::Bool(value)).0
}
#[no_mangle]
pub extern "C" fn agi_value_int(value: i64) -> u64 {
    REGISTRY.insert_value(AgilangValue::Int(value)).0
}
#[no_mangle]
pub extern "C" fn agi_value_uint(value: u64) -> u64 {
    REGISTRY.insert_value(AgilangValue::UInt(value)).0
}
#[no_mangle]
pub extern "C" fn agi_value_float(value: f64) -> u64 {
    REGISTRY.insert_value(AgilangValue::Float(value)).0
}
#[no_mangle]
pub extern "C" fn agi_value_array() -> u64 {
    REGISTRY.insert_value(AgilangValue::array(Vec::new())).0
}
#[no_mangle]
pub extern "C" fn agi_value_map() -> u64 {
    REGISTRY.insert_value(AgilangValue::map(BTreeMap::new())).0
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_string(data: *const u8, len: usize, out: *mut u64) -> u32 {
    ffi_status(|| write_handle(out, AgilangValue::from(input_text(data, len)?)))
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_bytes(data: *const u8, len: usize, out: *mut u64) -> u32 {
    ffi_status(|| write_handle(out, AgilangValue::from(input_bytes(data, len)?.to_vec())))
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_from_json(data: *const u8, len: usize, out: *mut u64) -> u32 {
    ffi_status(|| write_handle(out, AgilangValue::from_json(input_text(data, len)?)?))
}

#[no_mangle]
pub extern "C" fn agi_value_kind(handle: u64) -> u32 {
    REGISTRY
        .get_value(HandleId(handle))
        .map(|v| v.kind() as u32)
        .unwrap_or(AgilangType::Invalid as u32)
}

macro_rules! reader {
    ($name:ident, $ty:ty, $method:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(handle: u64, out: *mut $ty) -> u32 {
            ffi_status(|| {
                if out.is_null() {
                    return Err(AgilangError::invalid_argument("output pointer is null"));
                }
                *out = REGISTRY.get_value(HandleId(handle))?.$method()?;
                Ok(())
            })
        }
    };
}
reader!(agi_value_read_bool, bool, as_bool);
reader!(agi_value_read_int, i64, as_i64);
reader!(agi_value_read_uint, u64, as_u64);
reader!(agi_value_read_float, f64, as_f64);

#[no_mangle]
pub unsafe extern "C" fn agi_value_read_string(handle: u64, out: *mut AgiBuffer) -> u32 {
    ffi_status(|| {
        write_buffer(
            out,
            REGISTRY
                .get_value(HandleId(handle))?
                .as_str()?
                .as_bytes()
                .to_vec(),
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_read_bytes(handle: u64, out: *mut AgiBuffer) -> u32 {
    ffi_status(|| {
        write_buffer(
            out,
            REGISTRY.get_value(HandleId(handle))?.as_bytes()?.to_vec(),
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_len(handle: u64, out: *mut usize) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument("output pointer is null"));
        }
        *out = REGISTRY.get_value(HandleId(handle))?.len()?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_is_empty(handle: u64, out: *mut bool) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument("output pointer is null"));
        }
        *out = REGISTRY.get_value(HandleId(handle))?.is_empty()?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_array_push(array: u64, value: u64, out: *mut u64) -> u32 {
    ffi_status(|| {
        let next = REGISTRY
            .get_value(HandleId(array))?
            .array_push(REGISTRY.get_value(HandleId(value))?)?;
        write_handle(out, next)
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_array_get(array: u64, index: usize, out: *mut u64) -> u32 {
    ffi_status(|| write_handle(out, REGISTRY.get_value(HandleId(array))?.array_get(index)?))
}

#[no_mangle]
pub unsafe extern "C" fn agi_map_insert(
    map: u64,
    key: *const u8,
    key_len: usize,
    value: u64,
    out: *mut u64,
) -> u32 {
    ffi_status(|| {
        let next = REGISTRY.get_value(HandleId(map))?.map_insert(
            input_text(key, key_len)?.to_owned(),
            REGISTRY.get_value(HandleId(value))?,
        )?;
        write_handle(out, next)
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_map_get(
    map: u64,
    key: *const u8,
    key_len: usize,
    out: *mut u64,
) -> u32 {
    ffi_status(|| {
        write_handle(
            out,
            REGISTRY
                .get_value(HandleId(map))?
                .map_get(input_text(key, key_len)?)?,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_equal(left: u64, right: u64, out: *mut bool) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument("output pointer is null"));
        }
        *out = REGISTRY.get_value(HandleId(left))? == REGISTRY.get_value(HandleId(right))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_value_to_json(handle: u64, out: *mut AgiBuffer) -> u32 {
    ffi_status(|| {
        write_buffer(
            out,
            REGISTRY
                .get_value(HandleId(handle))?
                .to_json()?
                .into_bytes(),
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_platform_info_json(out: *mut AgiBuffer) -> u32 {
    ffi_status(|| {
        let value = serde_json::to_vec(&agilang_runtime_platform::platform_info()?)
            .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))?;
        write_buffer(out, value)
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_buffer_free(buffer: *mut AgiBuffer) {
    if buffer.is_null() {
        return;
    }
    let buffer = &mut *buffer;
    if !buffer.data.is_null() {
        drop(Vec::from_raw_parts(
            buffer.data,
            buffer.len,
            buffer.capacity,
        ));
    }
    *buffer = AgiBuffer::empty();
}

#[no_mangle]
pub extern "C" fn agi_handle_clone(handle: u64) -> u64 {
    ffi_handle(|| REGISTRY.clone_handle(HandleId(handle)))
}
#[no_mangle]
pub extern "C" fn agi_handle_release(handle: u64) -> u32 {
    ffi_status(|| REGISTRY.release(HandleId(handle)))
}

// Reserved symbol used by loaders to verify that the dynamic library is AGILANG-compatible.
#[no_mangle]
pub extern "C" fn agi_runtime_identity() -> *const c_char {
    b"AGILANG-NATIVE-RUNTIME\0".as_ptr().cast()
}

#[no_mangle]
pub unsafe extern "C" fn agi_print(msg: *const c_char) {
    if !msg.is_null() {
        if let Ok(s) = std::ffi::CStr::from_ptr(msg).to_str() {
            println!("{}", s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    static TEST_MUTEX: Lazy<std::sync::Mutex<()>> = Lazy::new(|| std::sync::Mutex::new(()));

    #[test]
    fn collection_round_trip_and_stale_handle_rejection() {
        let _guard = TEST_MUTEX.lock().unwrap();
        agi_runtime_reset();
        let array = agi_value_array();
        let number = agi_value_int(1990);
        let mut next = 0;
        assert_eq!(unsafe { agi_array_push(array, number, &mut next) }, 0);
        let mut item = 0;
        assert_eq!(unsafe { agi_array_get(next, 0, &mut item) }, 0);
        let mut value = 0;
        assert_eq!(unsafe { agi_value_read_int(item, &mut value) }, 0);
        assert_eq!(value, 1990);
        assert_eq!(agi_handle_release(item), 0);
        assert_ne!(agi_handle_release(item), 0);
    }

    #[test]
    fn is_empty_abi_test() {
        let _guard = TEST_MUTEX.lock().unwrap();
        agi_runtime_reset();
        let array = agi_value_array();

        let mut empty = false;
        assert_eq!(
            unsafe { agi_value_is_empty(array, &mut empty) },
            ErrorCode::Ok as u32
        );
        assert!(empty);

        let number = agi_value_int(1990);
        let mut updated = 0;
        assert_eq!(
            unsafe { agi_array_push(array, number, &mut updated) },
            ErrorCode::Ok as u32
        );

        assert_eq!(
            unsafe { agi_value_is_empty(updated, &mut empty) },
            ErrorCode::Ok as u32
        );
        assert!(!empty);
    }
}
