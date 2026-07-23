import Foundation
import SwiftUI
import UIKit
import RetrievalKitGraphFFI

struct GraphBenchmarkView: View {
    @State private var report = "{}"

    var body: some View {
        ScrollView {
            Text(report)
                .font(.system(.footnote, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding()
        }
        .task {
            if isAutomatedBenchmarkLaunch {
                UIApplication.shared.isIdleTimerDisabled = true
                guard await ForegroundExecutionGate.waitUntilActive(
                    isActive: { UIApplication.shared.applicationState == .active }
                ) else {
                    report = foregroundTimeoutResponse()
                    FileHandle.standardOutput.write(Data("\(report)\n".utf8))
                    exit(2)
                }
            }
            report = await GraphHarnessPreflight.run()
            if ProcessInfo.processInfo.arguments.contains("--phase4-graph-preflight")
                || ProcessInfo.processInfo.arguments.contains("--phase4-query-session")
                || ProcessInfo.processInfo.arguments.contains("--phase4-lifecycle-sample")
                || ProcessInfo.processInfo.arguments.contains("--phase4-graph-free-regression") {
                FileHandle.standardOutput.write(Data("\(report)\n".utf8))
                exit(report.contains("\"ok\" : true") ? EXIT_SUCCESS : 2)
            }
        }
    }
}

private func foregroundTimeoutResponse() -> String {
    let value: [String: Any] = [
        "ok": false,
        "error": "automated benchmark did not reach foreground before timeout",
        "foreground": false,
        "foreground_wait_outside_measurement": true
    ]
    guard let data = try? JSONSerialization.data(
        withJSONObject: value,
        options: [.prettyPrinted, .sortedKeys]
    ) else {
        return "{\"ok\":false,\"error\":\"foreground timeout serialization failed\"}"
    }
    return String(decoding: data, as: UTF8.self)
}

private var isAutomatedBenchmarkLaunch: Bool {
    let arguments = ProcessInfo.processInfo.arguments
    return arguments.contains("--phase4-graph-preflight")
        || arguments.contains("--phase4-query-session")
        || arguments.contains("--phase4-lifecycle-sample")
        || arguments.contains("--phase4-graph-free-regression")
}

@MainActor
private enum GraphHarnessPreflight {
    private static var didRun = false

    static func run() async -> String {
        guard !didRun else {
            return encode(["ok": false, "error": "one scenario per fresh process is required"])
        }
        didRun = true
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("--phase4-graph-free-regression") {
            return await graphFreeRegression(arguments: arguments)
        }
        guard let workload = value(after: "--phase4-workload", in: arguments),
              ["10k-384d-v3", "25k-384d-v3", "50k-384d-v3", "100k-384d-v3-stress"].contains(workload),
              let encoding = value(after: "--phase4-encoding", in: arguments),
              ["f32", "i8"].contains(encoding) else {
            return encode(["ok": false, "error": "a frozen workload and f32/i8 encoding are required"])
        }
        if workload == "100k-384d-v3-stress",
           !arguments.contains("--phase4-100k-preflight-safe") {
            return encode([
                "ok": true,
                "status": arguments.contains("--phase4-100k-preflight-unsafe")
                    ? "not_run_memory_safety" : "not_run_preflight_required",
                "classification": "stress",
                "marketing_eligible": false,
                "workload_id": workload
            ])
        }

        let physical = isPhysicalDevice
        if arguments.contains("--physical-device-required") && !physical {
            return encode(["ok": false, "error": "simulator output cannot satisfy a physical-device run"])
        }
        let thermal = thermalStateName(ProcessInfo.processInfo.thermalState)
        if thermal == "serious" || thermal == "critical" {
            return encode(["ok": false, "error": "thermally invalid session", "thermal_state": thermal])
        }
        let device = UIDevice.current
        device.isBatteryMonitoringEnabled = true
        let supported = workload != "100k-384d-v3-stress"
        let batteryStart = Double(device.batteryLevel)
        func environment() -> [String: Any] { [
            "build_configuration": buildConfiguration,
            "physical_device": physical,
            "simulator": !physical,
            "device_identifier": hardwareIdentifier(),
            "hardware_model": sysctlString("hw.model"),
            "os_version": device.systemVersion,
            "os_build": ProcessInfo.processInfo.operatingSystemVersionString,
            "physical_memory_bytes": ProcessInfo.processInfo.physicalMemory,
            "process_id": Int(ProcessInfo.processInfo.processIdentifier),
            "one_scenario_per_fresh_process": true,
            "thermal_state_start": thermal,
            "thermal_state_end": thermalStateName(ProcessInfo.processInfo.thermalState),
            "power_state": powerState(device),
            "battery_level_start": batteryStart,
            "battery_level_end": Double(device.batteryLevel),
            "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
            "free_storage_bytes": freeStorageBytes(),
            "foreground": UIApplication.shared.applicationState == .active,
            "network_disabled": true
        ] }
        if arguments.contains("--phase4-query-session") {
            guard let sessionID = value(after: "--phase4-session", in: arguments) else {
                return encode(["ok": false, "error": "--phase4-session is required"])
            }
            let config: [String: Any] = [
                "workload_id": workload,
                "encoding": encoding,
                "session_id": sessionID
            ]
            guard let data = try? JSONSerialization.data(withJSONObject: config, options: [.sortedKeys]),
                  let configJSON = String(data: data, encoding: .utf8) else {
                return encode(["ok": false, "error": "device config serialization failed"])
            }
            guard let runnerResponse = await Task.detached(priority: .userInitiated, operation: {
                invokeDeviceQuery(configJSON)
            }).value else {
                return encode(["ok": false, "error": "device runner returned null"])
            }
            guard let responseData = runnerResponse.data(using: .utf8),
                  var response = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
                return encode(["ok": false, "error": "device runner returned malformed JSON"])
            }
            response["environment"] = environment()
            response["device_role"] = value(after: "--phase4-device-role", in: arguments) ?? "unregistered"
            return encode(response)
        }
        if arguments.contains("--phase4-lifecycle-sample") {
            guard let sampleID = value(after: "--phase4-sample", in: arguments),
                  let operation = value(after: "--phase4-operation", in: arguments) else {
                return encode([
                    "ok": false,
                    "error": "--phase4-sample and --phase4-operation are required"
                ])
            }
            let persisted = benchmarkSupportDirectory()
                .appendingPathComponent("persisted", isDirectory: true)
                .appendingPathComponent(workload, isDirectory: true)
                .appendingPathComponent(encoding, isDirectory: true)
            let directory = operation == "save"
                ? benchmarkSupportDirectory()
                    .appendingPathComponent("samples", isDirectory: true)
                    .appendingPathComponent(sampleID.replacingOccurrences(of: "/", with: "_"), isDirectory: true)
                : persisted
            let config: [String: Any] = [
                "workload_id": workload,
                "encoding": encoding,
                "sample_id": sampleID,
                "operation": operation,
                "directory": directory.path
            ]
            guard let data = try? JSONSerialization.data(withJSONObject: config, options: [.sortedKeys]),
                  let configJSON = String(data: data, encoding: .utf8) else {
                return encode(["ok": false, "error": "lifecycle config serialization failed"])
            }
            guard let runnerResponse = await Task.detached(priority: .userInitiated, operation: {
                invokeLifecycleSample(configJSON)
            }).value else {
                return encode(["ok": false, "error": "lifecycle runner returned null"])
            }
            guard let responseData = runnerResponse.data(using: .utf8),
                  var response = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
                return encode(["ok": false, "error": "lifecycle runner returned malformed JSON"])
            }
            response["environment"] = environment()
            response["device_role"] = value(after: "--phase4-device-role", in: arguments) ?? "unregistered"
            return encode(response)
        }
        return encode([
            "ok": true,
            "schema_version": 1,
            "artifact_type": "phase4_ios_graph_harness_preflight",
            "status": "ready",
            "workload_id": workload,
            "classification": supported ? "supported_product" : "stress",
            "marketing_eligible": false,
            "supported_v1_capacity_changed": false,
            "encoding": encoding,
            "graph_ffi_abi_version": Int(retrievalkit_graph_ffi_abi_version()),
            "environment": environment(),
            "query_warmups": 100,
            "query_samples": 1_000,
            "lifecycle_warmups": 3,
            "lifecycle_samples": 20,
            "rss_interval_ms": 1,
            "memory_repetitions": 5,
            "minimum_final_sessions": 3,
            "stages": [
                "seed_resolution", "traversal", "projection", "filter_intersection",
                "ranking", "hydration", "end_to_end_total"
            ]
        ])
    }

    private static func graphFreeRegression(arguments: [String]) async -> String {
        guard let encoding = value(after: "--phase4-encoding", in: arguments),
              ["f32", "i8"].contains(encoding),
              let sessionID = value(after: "--phase4-session", in: arguments),
              let product = value(after: "--phase4-product", in: arguments),
              product == "candidate" else {
            return encode([
                "ok": false,
                "error": "encoding, session, and candidate product are required"
            ])
        }
        let config: [String: Any] = [
            "encoding": encoding,
            "session_id": sessionID,
            "product": product
        ]
        let device = UIDevice.current
        device.isBatteryMonitoringEnabled = true
        let thermalStart = thermalStateName(ProcessInfo.processInfo.thermalState)
        let batteryStart = Double(device.batteryLevel)
        guard let data = try? JSONSerialization.data(withJSONObject: config, options: [.sortedKeys]),
              let configJSON = String(data: data, encoding: .utf8) else {
            return encode(["ok": false, "error": "graph-free config serialization failed"])
        }
        guard let runnerResponse = await Task.detached(priority: .userInitiated, operation: {
            invokeGraphFreeRegression(configJSON)
        }).value else {
            return encode(["ok": false, "error": "graph-free runner returned null"])
        }
        guard let responseData = runnerResponse.data(using: .utf8),
              var response = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
            return encode(["ok": false, "error": "graph-free runner returned malformed JSON"])
        }
        response["device_role"] = value(after: "--phase4-device-role", in: arguments) ?? "unregistered"
        response["environment"] = physicalEnvironment(
            thermalStart: thermalStart,
            batteryStart: batteryStart
        )
        return encode(response)
    }

    private static func physicalEnvironment(
        thermalStart: String,
        batteryStart: Double
    ) -> [String: Any] {
        let device = UIDevice.current
        device.isBatteryMonitoringEnabled = true
        return [
            "build_configuration": buildConfiguration,
            "physical_device": isPhysicalDevice,
            "simulator": !isPhysicalDevice,
            "device_identifier": hardwareIdentifier(),
            "hardware_model": sysctlString("hw.model"),
            "os_version": device.systemVersion,
            "os_build": ProcessInfo.processInfo.operatingSystemVersionString,
            "physical_memory_bytes": ProcessInfo.processInfo.physicalMemory,
            "process_id": Int(ProcessInfo.processInfo.processIdentifier),
            "one_scenario_per_fresh_process": true,
            "thermal_state_start": thermalStart,
            "thermal_state_end": thermalStateName(ProcessInfo.processInfo.thermalState),
            "power_state": powerState(device),
            "battery_level_start": batteryStart,
            "battery_level_end": Double(device.batteryLevel),
            "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
            "free_storage_bytes": freeStorageBytes(),
            "foreground": UIApplication.shared.applicationState == .active,
            "network_disabled": true
        ]
    }

    private static func value(after flag: String, in arguments: [String]) -> String? {
        guard let index = arguments.firstIndex(of: flag), arguments.indices.contains(index + 1) else {
            return nil
        }
        return arguments[index + 1]
    }

    private static func encode(_ value: [String: Any]) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys]) else {
            return "{\"ok\":false,\"error\":\"serialization failed\"}"
        }
        return String(decoding: data, as: UTF8.self)
    }
}

private func invokeDeviceQuery(_ configJSON: String) -> String? {
    invokeFFI(configJSON, retrievalkit_phase4_device_query_session_json)
}

private func invokeLifecycleSample(_ configJSON: String) -> String? {
    invokeFFI(configJSON, retrievalkit_phase4_device_lifecycle_sample_json)
}

private func invokeGraphFreeRegression(_ configJSON: String) -> String? {
    invokeFFI(configJSON, retrievalkit_phase4_graph_free_regression_json)
}

private func invokeFFI(
    _ configJSON: String,
    _ function: (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
) -> String? {
    let pointer = configJSON.withCString { function($0) }
    guard let pointer else {
        return nil
    }
    defer { retrievalkit_string_free(pointer) }
    return String(cString: pointer)
}

private var buildConfiguration: String {
    #if DEBUG
    "debug"
    #else
    "release"
    #endif
}

private var isPhysicalDevice: Bool {
    #if targetEnvironment(simulator)
    false
    #else
    true
    #endif
}

private func hardwareIdentifier() -> String {
    var systemInfo = utsname()
    uname(&systemInfo)
    return withUnsafePointer(to: &systemInfo.machine) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: 1) { String(cString: $0) }
    }
}

private func sysctlString(_ name: String) -> String {
    var size = 0
    guard sysctlbyname(name, nil, &size, nil, 0) == 0, size > 0 else {
        return "unknown"
    }
    var value = [CChar](repeating: 0, count: size)
    guard sysctlbyname(name, &value, &size, nil, 0) == 0 else {
        return "unknown"
    }
    let bytes = value.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    return String(decoding: bytes, as: UTF8.self)
}

private func thermalStateName(_ state: ProcessInfo.ThermalState) -> String {
    switch state {
    case .nominal: "nominal"
    case .fair: "fair"
    case .serious: "serious"
    case .critical: "critical"
    @unknown default: "unknown"
    }
}

@MainActor
private func powerState(_ device: UIDevice) -> String {
    switch device.batteryState {
    case .charging: "charging"
    case .full: "full"
    case .unplugged: "battery"
    case .unknown: "unknown"
    @unknown default: "unknown"
    }
}

private func freeStorageBytes() -> Int64 {
    let values = try? URL(fileURLWithPath: NSHomeDirectory()).resourceValues(
        forKeys: [.volumeAvailableCapacityForImportantUsageKey]
    )
    return values?.volumeAvailableCapacityForImportantUsage ?? -1
}

private func benchmarkSupportDirectory() -> URL {
    let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    return base.appendingPathComponent("phase4b", isDirectory: true)
}
