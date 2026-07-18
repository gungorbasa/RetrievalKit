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
            if ProcessInfo.processInfo.arguments.contains("--phase4-graph-preflight") {
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
            "build_configuration": buildConfiguration,
            "physical_device": physical,
            "simulator": !physical,
            "device_model": device.model,
            "device_identifier": hardwareIdentifier(),
            "os_version": device.systemVersion,
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
            "network_disabled": true,
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
