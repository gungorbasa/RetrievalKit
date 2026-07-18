use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde::Serialize;

use crate::bench::{ProcessMemorySnapshot, RuntimeCapabilities};
use crate::json_to_c_string;

#[derive(Serialize)]
struct DeviceResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<RuntimeCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_memory: Option<ProcessMemorySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Executes one complete Phase 4b graph query session in the current process.
/// The returned UTF-8 JSON string must be released with `vectorkit_string_free`.
///
/// # Safety
///
/// `config_json` must point to a valid null-terminated UTF-8 string that remains
/// alive for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn vectorkit_phase4_device_query_session_json(
    config_json: *const c_char,
) -> *mut c_char {
    let response =
        catch_unwind(AssertUnwindSafe(|| unsafe { run(config_json) })).unwrap_or_else(|_| {
            DeviceResponse {
                ok: false,
                report: None,
                capabilities: None,
                baseline_memory: None,
                error: Some("Phase 4b device query session panicked".to_owned()),
            }
        });
    json_to_c_string(
        &serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_owned()),
    )
}

unsafe fn run(config_json: *const c_char) -> DeviceResponse {
    if config_json.is_null() {
        return failure("Phase 4b device config cannot be null");
    }
    let raw = match unsafe { CStr::from_ptr(config_json) }.to_str() {
        Ok(value) => value,
        Err(_) => return failure("Phase 4b device config must be valid UTF-8"),
    };
    match vectorkit_phase4_bench::run_device_query_session_json(raw) {
        Ok(report) => match serde_json::from_str(&report) {
            Ok(report) => DeviceResponse {
                ok: true,
                report: Some(report),
                capabilities: Some(RuntimeCapabilities::detect()),
                baseline_memory: ProcessMemorySnapshot::current(),
                error: None,
            },
            Err(error) => failure(&format!("invalid internal device report: {error}")),
        },
        Err(error) => failure(&error),
    }
}

fn failure(message: &str) -> DeviceResponse {
    DeviceResponse {
        ok: false,
        report: None,
        capabilities: None,
        baseline_memory: None,
        error: Some(message.to_owned()),
    }
}
