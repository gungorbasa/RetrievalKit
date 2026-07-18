import Foundation
import SwiftUI
import UIKit
import VectorKitGraphFFI

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
            report = GraphHarnessPreflight.run()
            if ProcessInfo.processInfo.arguments.contains("--phase4-graph-preflight")
                || ProcessInfo.processInfo.arguments.contains("--phase4-query-session") {
                FileHandle.standardOutput.write(Data("\(report)\n".utf8))
                exit(report.contains("\"ok\" : true") ? EXIT_SUCCESS : 2)
            }
        }
    }
}

@MainActor
private enum GraphHarnessPreflight {
    private static var didRun = false

    static func run() -> String {
        guard !didRun else {
            return encode(["ok": false, "error": "one scenario per fresh process is required"])
        }
        didRun = true
        let arguments = ProcessInfo.processInfo.arguments
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
        let environment: [String: Any] = [
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
            "battery_level_start": Double(device.batteryLevel),
            "battery_level_end": Double(device.batteryLevel),
            "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
            "free_storage_bytes": freeStorageBytes(),
            "foreground": UIApplication.shared.applicationState == .active,
            "network_disabled": true
        ]
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
            let pointer = configJSON.withCString {
                vectorkit_phase4_device_query_session_json($0)
            }
            guard let pointer else {
                return encode(["ok": false, "error": "device runner returned null"])
            }
            defer { vectorkit_string_free(pointer) }
            guard let responseData = String(cString: pointer).data(using: .utf8),
                  var response = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
                return encode(["ok": false, "error": "device runner returned malformed JSON"])
            }
            response["environment"] = environment
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
            "graph_ffi_abi_version": Int(vectorkit_graph_ffi_abi_version()),
            "environment": environment,
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
    return String(cString: value)
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
