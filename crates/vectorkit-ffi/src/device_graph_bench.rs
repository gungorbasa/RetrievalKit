use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    memory_evidence: Option<MemoryEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct MemoryEvidence {
    sample_interval_ms: u64,
    baseline_resident_bytes: Option<u64>,
    peak_resident_bytes: Option<u64>,
    peak_delta_bytes: Option<u64>,
    samples: Vec<MemorySample>,
}

#[derive(Serialize)]
struct MemorySample {
    offset_ns: u64,
    resident_bytes: u64,
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
                memory_evidence: None,
                error: Some("Phase 4b device query session panicked".to_owned()),
            }
        });
    json_to_c_string(
        &serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_owned()),
    )
}

/// Executes one Phase 4b lifecycle operation in the current fresh process.
/// The returned UTF-8 JSON string must be released with `vectorkit_string_free`.
///
/// # Safety
///
/// `config_json` must point to a valid null-terminated UTF-8 string that remains
/// alive for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn vectorkit_phase4_device_lifecycle_sample_json(
    config_json: *const c_char,
) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| unsafe { run_lifecycle(config_json) }))
        .unwrap_or_else(|_| failure("Phase 4b lifecycle sample panicked"));
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
    let sampler = MemorySampler::start();
    let result = vectorkit_phase4_bench::run_device_query_session_json(raw);
    let memory_evidence = sampler.stop();
    match result {
        Ok(report) => match serde_json::from_str(&report) {
            Ok(report) => DeviceResponse {
                ok: true,
                report: Some(report),
                capabilities: Some(RuntimeCapabilities::detect()),
                memory_evidence: Some(memory_evidence),
                error: None,
            },
            Err(error) => failure(&format!("invalid internal device report: {error}")),
        },
        Err(error) => failure(&error),
    }
}

unsafe fn run_lifecycle(config_json: *const c_char) -> DeviceResponse {
    if config_json.is_null() {
        return failure("Phase 4b lifecycle config cannot be null");
    }
    let raw = match unsafe { CStr::from_ptr(config_json) }.to_str() {
        Ok(value) => value,
        Err(_) => return failure("Phase 4b lifecycle config must be valid UTF-8"),
    };
    let sampler = MemorySampler::start();
    let result = vectorkit_phase4_bench::run_device_lifecycle_sample_json(raw);
    let memory_evidence = sampler.stop();
    match result {
        Ok(report) => match serde_json::from_str(&report) {
            Ok(report) => DeviceResponse {
                ok: true,
                report: Some(report),
                capabilities: Some(RuntimeCapabilities::detect()),
                memory_evidence: Some(memory_evidence),
                error: None,
            },
            Err(error) => failure(&format!("invalid internal lifecycle report: {error}")),
        },
        Err(error) => failure(&error),
    }
}

fn failure(message: &str) -> DeviceResponse {
    DeviceResponse {
        ok: false,
        report: None,
        capabilities: None,
        memory_evidence: None,
        error: Some(message.to_owned()),
    }
}

struct MemorySampler {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<MemorySample>>>,
    baseline: Option<u64>,
    started: Instant,
    thread: Option<thread::JoinHandle<()>>,
}

impl MemorySampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let baseline = ProcessMemorySnapshot::current().map(ProcessMemorySnapshot::resident_bytes);
        let started = Instant::now();
        let thread_stop = Arc::clone(&stop);
        let thread_samples = Arc::clone(&samples);
        let thread_started = started;
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                if let Some(snapshot) = ProcessMemorySnapshot::current() {
                    if let Ok(mut values) = thread_samples.lock() {
                        values.push(MemorySample {
                            offset_ns: u64::try_from(thread_started.elapsed().as_nanos())
                                .unwrap_or(u64::MAX),
                            resident_bytes: snapshot.resident_bytes(),
                        });
                    }
                }
                thread::sleep(Duration::from_millis(1));
            }
        });
        Self {
            stop,
            samples,
            baseline,
            started,
            thread: Some(thread),
        }
    }

    fn stop(mut self) -> MemoryEvidence {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some(snapshot) = ProcessMemorySnapshot::current() {
            if let Ok(mut values) = self.samples.lock() {
                values.push(MemorySample {
                    offset_ns: u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    resident_bytes: snapshot.resident_bytes(),
                });
            }
        }
        let samples = Arc::try_unwrap(self.samples)
            .ok()
            .and_then(|values| values.into_inner().ok())
            .unwrap_or_default();
        let peak = samples.iter().map(|sample| sample.resident_bytes).max();
        MemoryEvidence {
            sample_interval_ms: 1,
            baseline_resident_bytes: self.baseline,
            peak_resident_bytes: peak,
            peak_delta_bytes: peak
                .zip(self.baseline)
                .map(|(peak, baseline)| peak.saturating_sub(baseline)),
            samples,
        }
    }
}
