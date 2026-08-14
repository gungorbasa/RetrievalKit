import Darwin
import CoreML
import Foundation
import Network
import RetrievalKitAppleE2EBenchmarkCore
import SwiftUI
import UIKit

@main
struct RetrievalKitAppleE2EIOSBenchApp: App {
    @State private var status = "Waiting to start"

    var body: some Scene {
        WindowGroup {
            VStack(spacing: 16) {
                Text("RetrievalKit Apple E2E")
                    .font(.title2)
                Text(status)
                    .font(.system(.body, design: .monospaced))
                    .multilineTextAlignment(.center)
                    .padding()
            }
            .task { await execute() }
        }
    }

    @MainActor
    private func execute() async {
        do {
            status = "Checking device state"
            let arguments = try DeviceArguments(CommandLine.arguments)
            let tracker = DeviceValidityTracker(
                networkDisabledAssertion: arguments.networkDisabled,
                poweredExecution: arguments.contractVersion == "apple-end-to-end-v2"
            )
            try await tracker.start()
            defer { tracker.stop() }
            try tracker.requireValidStart()

            let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            let assetRoot = documents.appendingPathComponent(arguments.assetRoot, isDirectory: true)
            let output = documents.appendingPathComponent(arguments.output)
            if arguments.workloadID.contains("100k") {
                try requireSafe100KStorage(index: assetRoot.appendingPathComponent("index"))
            }

            status = "Compiling verified local model"
            let modelSource = assetRoot.appendingPathComponent("model.mlpackage", isDirectory: true)
            let compiledModel = try await Task.detached(priority: .userInitiated) {
                try MLModel.compileModel(at: modelSource)
            }.value

            status = "Running \(arguments.mode.rawValue)"
            _ = try await AppleE2EBenchmark.run(RunConfiguration(
                contractVersion: arguments.contractVersion,
                queriesURL: assetRoot.appendingPathComponent("queries.json"),
                indexURL: assetRoot.appendingPathComponent("index", isDirectory: true),
                modelURL: compiledModel,
                tokenizerURL: assetRoot.appendingPathComponent("tokenizer.json"),
                outputURL: output,
                workloadID: arguments.workloadID,
                workloadClassification: arguments.workloadClassification,
                marketingEligible: arguments.workloadClassification != "stress",
                profileID: arguments.profileID,
                profileClassification: arguments.profileClassification,
                sessionID: arguments.sessionID,
                mode: arguments.mode,
                retrievalKitRevision: arguments.retrievalKitRevision,
                iphoneValidityProvider: { tracker.validity() },
                abortCheck: { tracker.abortReason() }
            ))
            status = "Complete: \(arguments.output)"
            print("BENCHMARK_COMPLETE \(arguments.output)")
            fflush(stdout)
            exit(0)
        } catch {
            status = "Failed: \(error)"
            fputs("BENCHMARK_FAILED \(error)\n", stderr)
            fflush(stderr)
            exit(2)
        }
    }

    private func requireSafe100KStorage(index: URL) throws {
        let resourceKeys: Set<URLResourceKey> = [.fileSizeKey, .isRegularFileKey]
        let enumerator = FileManager.default.enumerator(
            at: index,
            includingPropertiesForKeys: Array(resourceKeys)
        )
        var databaseBytes: Int64 = 0
        while let file = enumerator?.nextObject() as? URL {
            let values = try file.resourceValues(forKeys: resourceKeys)
            if values.isRegularFile == true {
                databaseBytes += Int64(values.fileSize ?? 0)
            }
        }
        let values = try index.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
        let required = databaseBytes * 3 + 1_073_741_824
        guard Int64(values.volumeAvailableCapacityForImportantUsage ?? 0) >= required else {
            throw AppleE2EBenchmarkError.invalidState("not_run_memory_safety: insufficient free storage")
        }
    }
}

private final class DeviceValidityTracker: @unchecked Sendable {
    private let lock = NSLock()
    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "dev.retrievalkit.apple-e2e.network")
    private let networkDisabledAssertion: Bool
    private let poweredExecution: Bool
    private var networkUnsatisfied = false
    private var networkDetails = "no path update"
    private var memoryWarning = false
    private var leftForeground = false
    private var observers: [NSObjectProtocol] = []
    private var thermalStart = ProcessInfo.processInfo.thermalState
    private var foregroundStart = false
    private var batteryPercent = -1
    private var charging = true
    private var batteryStateName = "unknown"
    private var lowPowerMode = true

    init(networkDisabledAssertion: Bool, poweredExecution: Bool) {
        self.networkDisabledAssertion = networkDisabledAssertion
        self.poweredExecution = poweredExecution
    }

    @MainActor
    func start() async throws {
        #if targetEnvironment(simulator)
        throw AppleE2EBenchmarkError.invalidState("physical iPhone required; simulator detected")
        #endif
        UIDevice.current.isBatteryMonitoringEnabled = true
        for _ in 0..<50 where UIApplication.shared.applicationState != .active {
            try await Task.sleep(for: .milliseconds(100))
        }
        thermalStart = ProcessInfo.processInfo.thermalState
        foregroundStart = UIApplication.shared.applicationState == .active
        batteryPercent = Int((UIDevice.current.batteryLevel * 100).rounded())
        batteryStateName = switch UIDevice.current.batteryState {
        case .unknown: "unknown"
        case .unplugged: "unplugged"
        case .charging: "charging"
        case .full: "full"
        @unknown default: "unknown"
        }
        charging = UIDevice.current.batteryState == .charging
        lowPowerMode = ProcessInfo.processInfo.isLowPowerModeEnabled
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.didReceiveMemoryWarningNotification,
            object: nil,
            queue: nil
        ) { [weak self] _ in self?.markMemoryWarning() })
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.didEnterBackgroundNotification,
            object: nil,
            queue: nil
        ) { [weak self] _ in self?.markBackground() })
        monitor.pathUpdateHandler = { [weak self] path in
            self?.setNetworkPath(path)
        }
        monitor.start(queue: queue)
        try await Task.sleep(for: .seconds(1))
    }

    func stop() {
        monitor.cancel()
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    func requireValidStart() throws {
        guard foregroundStart else { throw invalid("application is not foreground-active") }
        guard networkDisabledAssertion && snapshot().networkUnsatisfied else {
            throw invalid("network must be disabled and observed unavailable (\(snapshot().networkDetails))")
        }
        guard !lowPowerMode else { throw invalid("Low Power Mode is enabled") }
        guard (50...90).contains(batteryPercent) else { throw invalid("battery must be 50-90 percent") }
        if poweredExecution {
            guard batteryStateName == "charging" || batteryStateName == "full" else {
                throw invalid("powered execution requires charging or full battery state")
            }
        } else {
            guard !charging else {
                throw invalid("device must not be charging (state=\(batteryStateName), battery=\(batteryPercent)%)")
            }
        }
        guard thermalStart == .nominal else { throw invalid("thermal start must be nominal") }
    }

    func validity() -> IPhoneValidity? {
        let state = snapshot()
        return IPhoneValidity(
            physicalDevice: true,
            foregroundStart: foregroundStart,
            foregroundEnd: !state.leftForeground,
            networkDisabled: networkDisabledAssertion && state.networkUnsatisfied,
            lowPowerMode: lowPowerMode,
            batteryPercent: batteryPercent,
            batteryState: batteryStateName,
            charging: charging,
            thermalStart: thermalName(thermalStart),
            thermalEnd: thermalName(ProcessInfo.processInfo.thermalState),
            memoryWarning: state.memoryWarning
        )
    }

    func abortReason() -> String? {
        let state = snapshot()
        if state.memoryWarning { return "memory warning" }
        if state.leftForeground { return "application left foreground" }
        if !state.networkUnsatisfied { return "network became available" }
        switch ProcessInfo.processInfo.thermalState {
        case .serious, .critical: return "thermal state became serious or critical"
        default: return nil
        }
    }

    private func invalid(_ message: String) -> AppleE2EBenchmarkError {
        .invalidState("invalid iPhone preflight: \(message)")
    }

    private func markMemoryWarning() { lock.withLock { memoryWarning = true } }
    private func markBackground() { lock.withLock { leftForeground = true } }
    private func setNetworkPath(_ path: NWPath) {
        let interfaces: [(NWInterface.InterfaceType, String)] = [
            (.wifi, "wifi"), (.cellular, "cellular"), (.wiredEthernet, "wiredEthernet"),
            (.loopback, "loopback"), (.other, "other"),
        ]
        let active = interfaces.compactMap { path.usesInterfaceType($0.0) ? $0.1 : nil }
        lock.withLock {
            networkUnsatisfied = path.status == .unsatisfied
            networkDetails = "status=\(path.status), interfaces=\(active.joined(separator: ","))"
        }
    }
    private func snapshot() -> (
        networkUnsatisfied: Bool,
        networkDetails: String,
        memoryWarning: Bool,
        leftForeground: Bool
    ) {
        lock.withLock { (networkUnsatisfied, networkDetails, memoryWarning, leftForeground) }
    }

    private func thermalName(_ state: ProcessInfo.ThermalState) -> String {
        switch state {
        case .nominal: "nominal"
        case .fair: "fair"
        case .serious: "serious"
        case .critical: "critical"
        @unknown default: "unknown"
        }
    }
}

private struct DeviceArguments {
    let assetRoot: String
    let contractVersion: String
    let output: String
    let workloadID: String
    let workloadClassification: String
    let profileID: String
    let profileClassification: String
    let sessionID: String
    let mode: SearchMode
    let retrievalKitRevision: String
    let networkDisabled: Bool

    init(_ arguments: [String]) throws {
        var values: [String: String] = [:]
        var index = 1
        while index + 1 < arguments.count {
            let key = arguments[index]
            guard key.hasPrefix("--") else {
                throw AppleE2EBenchmarkError.invalidArgument("arguments must be --key value pairs")
            }
            values[String(key.dropFirst(2))] = arguments[index + 1]
            index += 2
        }
        func required(_ key: String) throws -> String {
            guard let value = values[key], !value.isEmpty else {
                throw AppleE2EBenchmarkError.invalidArgument("missing --\(key)")
            }
            return value
        }
        assetRoot = try required("asset-root")
        contractVersion = try required("contract-version")
        output = try required("output")
        workloadID = try required("workload-id")
        workloadClassification = try required("workload-classification")
        profileID = try required("profile-id")
        profileClassification = try required("profile-classification")
        sessionID = try required("session-id")
        retrievalKitRevision = try required("retrievalkit-revision")
        networkDisabled = try required("network-disabled") == "true"
        guard let parsedMode = SearchMode(rawValue: try required("mode")) else {
            throw AppleE2EBenchmarkError.invalidArgument("invalid --mode")
        }
        mode = parsedMode
    }
}
