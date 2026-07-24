//! Versioned, panic-safe C ABI for compiler-generated AGILANG programs.

#![allow(clippy::missing_safety_doc)]
#![allow(clippy::manual_c_str_literals)]

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    ffi::c_char,
    panic::{catch_unwind, AssertUnwindSafe},
    ptr, slice, str,
    sync::atomic::{AtomicU64, Ordering},
};

use agilang_runtime_accelerator::capabilities_json as accelerator_capabilities_json;
use agilang_runtime_compute::{capabilities as compute_capabilities, LinearModel, Tensor};
use agilang_runtime_core::{
    AgilangError, AgilangType, AgilangValue, ErrorCode, HandleId, HandleRegistry, RuntimeResult,
};
use agilang_runtime_crypto::{constant_time_eq, hmac_sha256, random_bytes, sha256};
use agilang_runtime_resource::capabilities_json as resource_capabilities_json;
use agilang_runtime_webrtc::capabilities_json as webrtc_capabilities_json;
use once_cell::sync::Lazy;
use parking_lot::RwLock;

pub const ABI_VERSION_MAJOR: u16 = 1;
pub const ABI_VERSION_MINOR: u16 = 3;
pub const ABI_VERSION_PATCH: u16 = 0;

static REGISTRY: Lazy<HandleRegistry> = Lazy::new(HandleRegistry::new);
static TENSOR_REGISTRY: Lazy<RwLock<HashMap<u64, Tensor>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
static MODEL_REGISTRY: Lazy<RwLock<HashMap<u64, LinearModel>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
static NEXT_COMPUTE_HANDLE: AtomicU64 = AtomicU64::new(1);

fn next_compute_handle() -> u64 {
    NEXT_COMPUTE_HANDLE.fetch_add(1, Ordering::Relaxed)
}

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
    TENSOR_REGISTRY.write().clear();
    MODEL_REGISTRY.write().clear();
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

// ---------------------------------------------------------------------------
// Native tensor and training ABI. Handles own their Rust allocations and must
// be released with agi_tensor_release / agi_model_release.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn agi_compute_capabilities_json(out: *mut AgiBuffer) -> u32 {
    ffi_status(|| {
        let bytes = serde_json::to_vec(&compute_capabilities())
            .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))?;
        write_buffer(out, bytes)
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_create_f64(
    shape: *const usize,
    rank: usize,
    data: *const f64,
    len: usize,
    out: *mut u64,
) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument(
                "output tensor handle is null",
            ));
        }
        let shape = input_usize_slice(shape, rank)?.to_vec();
        let data = input_f64_slice(data, len)?.to_vec();
        let tensor = Tensor::new(shape, data)?;
        let handle = next_compute_handle();
        TENSOR_REGISTRY.write().insert(handle, tensor);
        *out = handle;
        Ok(())
    })
}

unsafe fn input_f64_slice<'a>(data: *const f64, len: usize) -> RuntimeResult<&'a [f64]> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(AgilangError::invalid_argument("f64 input pointer is null"));
    }
    Ok(slice::from_raw_parts(data, len))
}

unsafe fn input_usize_slice<'a>(data: *const usize, len: usize) -> RuntimeResult<&'a [usize]> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(AgilangError::invalid_argument(
            "shape input pointer is null",
        ));
    }
    Ok(slice::from_raw_parts(data, len))
}

#[no_mangle]
pub extern "C" fn agi_tensor_release(handle: u64) -> u32 {
    ffi_status(|| {
        TENSOR_REGISTRY
            .write()
            .remove(&handle)
            .map(|_| ())
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_rank(handle: u64, out: *mut usize) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument(
                "rank output pointer is null",
            ));
        }
        *out = TENSOR_REGISTRY
            .read()
            .get(&handle)
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))?
            .rank();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_len(handle: u64, out: *mut usize) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument(
                "length output pointer is null",
            ));
        }
        *out = TENSOR_REGISTRY
            .read()
            .get(&handle)
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))?
            .len();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_shape(handle: u64, out: *mut usize, capacity: usize) -> u32 {
    ffi_status(|| {
        let registry = TENSOR_REGISTRY.read();
        let tensor = registry
            .get(&handle)
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))?;
        if capacity < tensor.rank() {
            return Err(AgilangError::invalid_argument(
                "shape output capacity is too small",
            ));
        }
        if tensor.rank() > 0 && out.is_null() {
            return Err(AgilangError::invalid_argument(
                "shape output pointer is null",
            ));
        }
        ptr::copy_nonoverlapping(tensor.shape().as_ptr(), out, tensor.rank());
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_read_f64(handle: u64, out: *mut f64, capacity: usize) -> u32 {
    ffi_status(|| {
        let registry = TENSOR_REGISTRY.read();
        let tensor = registry
            .get(&handle)
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))?;
        if capacity < tensor.len() {
            return Err(AgilangError::invalid_argument(
                "tensor output capacity is too small",
            ));
        }
        if !tensor.is_empty() && out.is_null() {
            return Err(AgilangError::invalid_argument(
                "tensor output pointer is null",
            ));
        }
        ptr::copy_nonoverlapping(tensor.data().as_ptr(), out, tensor.len());
        Ok(())
    })
}

fn store_tensor(tensor: Tensor, out: *mut u64) -> RuntimeResult<()> {
    if out.is_null() {
        return Err(AgilangError::invalid_argument(
            "output tensor handle is null",
        ));
    }
    let handle = next_compute_handle();
    TENSOR_REGISTRY.write().insert(handle, tensor);
    unsafe {
        *out = handle;
    }
    Ok(())
}

macro_rules! tensor_binary_abi {
    ($name:ident, $method:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(left: u64, right: u64, out: *mut u64) -> u32 {
            ffi_status(|| {
                let registry = TENSOR_REGISTRY.read();
                let left = registry
                    .get(&left)
                    .ok_or_else(|| AgilangError::invalid_argument("unknown left tensor handle"))?;
                let right = registry
                    .get(&right)
                    .ok_or_else(|| AgilangError::invalid_argument("unknown right tensor handle"))?;
                let result = left.$method(right)?;
                drop(registry);
                store_tensor(result, out)
            })
        }
    };
}

tensor_binary_abi!(agi_tensor_add, add);
tensor_binary_abi!(agi_tensor_sub, sub);
tensor_binary_abi!(agi_tensor_hadamard, hadamard);
tensor_binary_abi!(agi_tensor_matmul, matmul);

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_scale(handle: u64, factor: f64, out: *mut u64) -> u32 {
    ffi_status(|| {
        let registry = TENSOR_REGISTRY.read();
        let result = registry
            .get(&handle)
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))?
            .scale(factor)?;
        drop(registry);
        store_tensor(result, out)
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_relu(handle: u64, out: *mut u64) -> u32 {
    ffi_status(|| {
        let registry = TENSOR_REGISTRY.read();
        let result = registry
            .get(&handle)
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))?
            .relu();
        drop(registry);
        store_tensor(result, out)
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_softmax(handle: u64, out: *mut u64) -> u32 {
    ffi_status(|| {
        let registry = TENSOR_REGISTRY.read();
        let result = registry
            .get(&handle)
            .ok_or_else(|| AgilangError::invalid_argument("unknown tensor handle"))?
            .softmax()?;
        drop(registry);
        store_tensor(result, out)
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_tensor_mse(left: u64, right: u64, out: *mut f64) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument(
                "loss output pointer is null",
            ));
        }
        let registry = TENSOR_REGISTRY.read();
        let left = registry
            .get(&left)
            .ok_or_else(|| AgilangError::invalid_argument("unknown left tensor handle"))?;
        let right = registry
            .get(&right)
            .ok_or_else(|| AgilangError::invalid_argument("unknown right tensor handle"))?;
        *out = left.mean_squared_error(right)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_linear_model_create(feature_count: usize, out: *mut u64) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument(
                "output model handle is null",
            ));
        }
        let handle = next_compute_handle();
        MODEL_REGISTRY
            .write()
            .insert(handle, LinearModel::new(feature_count)?);
        *out = handle;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn agi_model_release(handle: u64) -> u32 {
    ffi_status(|| {
        MODEL_REGISTRY
            .write()
            .remove(&handle)
            .map(|_| ())
            .ok_or_else(|| AgilangError::invalid_argument("unknown model handle"))
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_linear_model_predict(
    model: u64,
    features: *const f64,
    len: usize,
    out: *mut f64,
) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument(
                "prediction output pointer is null",
            ));
        }
        let registry = MODEL_REGISTRY.read();
        *out = registry
            .get(&model)
            .ok_or_else(|| AgilangError::invalid_argument("unknown model handle"))?
            .predict(input_f64_slice(features, len)?)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_linear_model_train_batch(
    model: u64,
    features: u64,
    targets: *const f64,
    target_len: usize,
    learning_rate: f64,
    out_loss: *mut f64,
) -> u32 {
    ffi_status(|| {
        if out_loss.is_null() {
            return Err(AgilangError::invalid_argument(
                "loss output pointer is null",
            ));
        }
        let tensors = TENSOR_REGISTRY.read();
        let tensor = tensors
            .get(&features)
            .ok_or_else(|| AgilangError::invalid_argument("unknown feature tensor handle"))?;
        let mut models = MODEL_REGISTRY.write();
        let model = models
            .get_mut(&model)
            .ok_or_else(|| AgilangError::invalid_argument("unknown model handle"))?;
        *out_loss =
            model.train_batch(tensor, input_f64_slice(targets, target_len)?, learning_rate)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_accelerator_capabilities_json(out: *mut AgiBuffer) -> u32 {
    ffi_status(|| write_buffer(out, accelerator_capabilities_json()?))
}

#[no_mangle]
pub unsafe extern "C" fn agi_crypto_sha256(
    data: *const u8,
    len: usize,
    out: *mut u8,
    out_len: usize,
) -> u32 {
    ffi_status(|| {
        if out.is_null() || out_len < 32 {
            return Err(AgilangError::invalid_argument(
                "SHA-256 output buffer must contain at least 32 bytes",
            ));
        }
        let digest = sha256(input_bytes(data, len)?);
        ptr::copy_nonoverlapping(digest.as_ptr(), out, 32);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_crypto_hmac_sha256(
    key: *const u8,
    key_len: usize,
    data: *const u8,
    data_len: usize,
    out: *mut u8,
    out_len: usize,
) -> u32 {
    ffi_status(|| {
        if out.is_null() || out_len < 32 {
            return Err(AgilangError::invalid_argument(
                "HMAC output buffer must contain at least 32 bytes",
            ));
        }
        let digest = hmac_sha256(input_bytes(key, key_len)?, input_bytes(data, data_len)?);
        ptr::copy_nonoverlapping(digest.as_ptr(), out, 32);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_crypto_random(out: *mut u8, len: usize) -> u32 {
    ffi_status(|| {
        if len > 0 && out.is_null() {
            return Err(AgilangError::invalid_argument(
                "random output pointer is null",
            ));
        }
        let bytes = random_bytes(len)?;
        if len > 0 {
            ptr::copy_nonoverlapping(bytes.as_ptr(), out, len);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn agi_crypto_constant_time_eq(
    a: *const u8,
    a_len: usize,
    b: *const u8,
    b_len: usize,
    out: *mut bool,
) -> u32 {
    ffi_status(|| {
        if out.is_null() {
            return Err(AgilangError::invalid_argument(
                "comparison output pointer is null",
            ));
        }
        *out = constant_time_eq(input_bytes(a, a_len)?, input_bytes(b, b_len)?);
        Ok(())
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

/// Prints the given null-terminated string to standard output.
///
/// # Ownership Contract
/// - `agi_print` does not retain, modify, or free the provided pointer.
/// - The caller must provide a valid null-terminated string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn agi_print(msg: *const c_char) {
    if msg.is_null() {
        return;
    }

    let value = unsafe { std::ffi::CStr::from_ptr(msg) };

    match value.to_str() {
        Ok(text) => println!("{text}"),
        Err(_) => eprintln!("AGILANG runtime: invalid UTF-8 passed to agi_print"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn agi_resource_capabilities_json(out: *mut AgiBuffer) -> u32 {
    ffi_status(|| write_buffer(out, resource_capabilities_json()?))
}

#[no_mangle]
pub unsafe extern "C" fn agi_webrtc_capabilities_json(out: *mut AgiBuffer) -> u32 {
    ffi_status(|| write_buffer(out, webrtc_capabilities_json()?))
}

thread_local! {
    static HTTP_RESPONSE_BUFFER: RefCell<Option<std::ffi::CString>> = const { RefCell::new(None) };
}

fn set_http_response(s: String) -> *const c_char {
    let c_str = std::ffi::CString::new(s).unwrap_or_default();
    let ptr = c_str.as_ptr();
    HTTP_RESPONSE_BUFFER.with(|buf| *buf.borrow_mut() = Some(c_str));
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn agi_http_get_json(url_ptr: *const c_char) -> *const c_char {
    if url_ptr.is_null() {
        return set_http_response(String::new());
    }
    let url = match std::ffi::CStr::from_ptr(url_ptr).to_str() {
        Ok(s) => s,
        Err(_) => return set_http_response(String::new()),
    };

    let client = match agilang_runtime_http::HttpClient::new(agilang_runtime_http::TlsPolicy {
        require_https: false,
        allow_invalid_certificates: false,
    }) {
        Ok(c) => c,
        Err(_) => return set_http_response(String::new()),
    };

    let req = agilang_runtime_http::HttpRequest {
        method: "GET".to_string(),
        url: url.to_string(),
        headers: BTreeMap::new(),
        body: Vec::new(),
        timeout_ms: Some(10_000),
    };

    match agilang_runtime_async::block_on(client.execute(req)) {
        Ok(Ok(resp)) => {
            let body_str = String::from_utf8_lossy(&resp.body).into_owned();
            set_http_response(body_str)
        }
        Ok(Err(e)) => set_http_response(format!("{{\"error\": \"{}\"}}", e)),
        Err(e) => set_http_response(format!("{{\"error\": \"{}\"}}", e)),
    }
}

#[no_mangle]
pub unsafe extern "C" fn agi_http_get(url_ptr: *const c_char) -> *const c_char {
    agi_http_get_json(url_ptr)
}

#[no_mangle]
pub unsafe extern "C" fn agi_http_post(
    url_ptr: *const c_char,
    body_ptr: *const c_char,
) -> *const c_char {
    if url_ptr.is_null() {
        return set_http_response(String::new());
    }
    let url = match std::ffi::CStr::from_ptr(url_ptr).to_str() {
        Ok(s) => s,
        Err(_) => return set_http_response(String::new()),
    };
    let body = if body_ptr.is_null() {
        ""
    } else {
        match std::ffi::CStr::from_ptr(body_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return set_http_response(String::new()),
        }
    };

    let client = match agilang_runtime_http::HttpClient::new(agilang_runtime_http::TlsPolicy {
        require_https: false,
        allow_invalid_certificates: false,
    }) {
        Ok(c) => c,
        Err(_) => return set_http_response(String::new()),
    };

    let req = agilang_runtime_http::HttpRequest {
        method: "POST".to_string(),
        url: url.to_string(),
        headers: BTreeMap::new(),
        body: body.as_bytes().to_vec(),
        timeout_ms: Some(10_000),
    };

    match agilang_runtime_async::block_on(client.execute(req)) {
        Ok(Ok(resp)) => {
            let body_str = String::from_utf8_lossy(&resp.body).into_owned();
            set_http_response(body_str)
        }
        Ok(Err(e)) => set_http_response(format!("{{\"error\": \"{}\"}}", e)),
        Err(e) => set_http_response(format!("{{\"error\": \"{}\"}}", e)),
    }
}

#[no_mangle]
pub unsafe extern "C" fn agi_http_post_json(
    url_ptr: *const c_char,
    body_ptr: *const c_char,
) -> *const c_char {
    if url_ptr.is_null() {
        return set_http_response(String::new());
    }
    let url = match std::ffi::CStr::from_ptr(url_ptr).to_str() {
        Ok(s) => s,
        Err(_) => return set_http_response(String::new()),
    };
    let body = if body_ptr.is_null() {
        ""
    } else {
        match std::ffi::CStr::from_ptr(body_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return set_http_response(String::new()),
        }
    };

    let client = match agilang_runtime_http::HttpClient::new(agilang_runtime_http::TlsPolicy {
        require_https: false,
        allow_invalid_certificates: false,
    }) {
        Ok(c) => c,
        Err(_) => return set_http_response(String::new()),
    };

    let mut headers = BTreeMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let req = agilang_runtime_http::HttpRequest {
        method: "POST".to_string(),
        url: url.to_string(),
        headers,
        body: body.as_bytes().to_vec(),
        timeout_ms: Some(10_000),
    };

    match agilang_runtime_async::block_on(client.execute(req)) {
        Ok(Ok(resp)) => {
            let body_str = String::from_utf8_lossy(&resp.body).into_owned();
            set_http_response(body_str)
        }
        Ok(Err(e)) => set_http_response(format!("{{\"error\": \"{}\"}}", e)),
        Err(e) => set_http_response(format!("{{\"error\": \"{}\"}}", e)),
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

    #[test]
    fn native_tensor_abi_round_trip_and_release() {
        let _guard = TEST_MUTEX.lock().unwrap();
        agi_runtime_reset();
        let shape = [2usize, 2usize];
        let data = [1.0f64, 2.0, 3.0, 4.0];
        let mut tensor = 0u64;
        assert_eq!(
            unsafe {
                agi_tensor_create_f64(
                    shape.as_ptr(),
                    shape.len(),
                    data.as_ptr(),
                    data.len(),
                    &mut tensor,
                )
            },
            0
        );
        assert_ne!(tensor, 0);
        let mut len = 0usize;
        assert_eq!(unsafe { agi_tensor_len(tensor, &mut len) }, 0);
        assert_eq!(len, 4);
        let mut output = [0.0f64; 4];
        assert_eq!(
            unsafe { agi_tensor_read_f64(tensor, output.as_mut_ptr(), output.len()) },
            0
        );
        assert_eq!(output, data);
        assert_eq!(agi_tensor_release(tensor), 0);
        assert_ne!(agi_tensor_release(tensor), 0);
    }

    #[test]
    fn native_linear_model_training_reduces_loss_through_abi() {
        let _guard = TEST_MUTEX.lock().unwrap();
        agi_runtime_reset();
        let shape = [4usize, 1usize];
        let features = [1.0f64, 2.0, 3.0, 4.0];
        let targets = [3.0f64, 5.0, 7.0, 9.0];
        let mut tensor = 0u64;
        let mut model = 0u64;
        assert_eq!(
            unsafe {
                agi_tensor_create_f64(
                    shape.as_ptr(),
                    shape.len(),
                    features.as_ptr(),
                    features.len(),
                    &mut tensor,
                )
            },
            0
        );
        assert_eq!(unsafe { agi_linear_model_create(1, &mut model) }, 0);
        let mut first = 0.0;
        assert_eq!(
            unsafe {
                agi_linear_model_train_batch(
                    model,
                    tensor,
                    targets.as_ptr(),
                    targets.len(),
                    0.01,
                    &mut first,
                )
            },
            0
        );
        let mut last = first;
        for _ in 0..1000 {
            assert_eq!(
                unsafe {
                    agi_linear_model_train_batch(
                        model,
                        tensor,
                        targets.as_ptr(),
                        targets.len(),
                        0.01,
                        &mut last,
                    )
                },
                0
            );
        }
        assert!(last < first);
        assert_eq!(agi_model_release(model), 0);
        assert_eq!(agi_tensor_release(tensor), 0);
    }
}
