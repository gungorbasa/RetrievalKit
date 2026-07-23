import Foundation

struct ForegroundExecutionGate {
    struct Configuration: Sendable {
        let maximumPolls: Int
        let pollIntervalNanoseconds: UInt64

        init(
            maximumPolls: Int = 1_200,
            pollIntervalNanoseconds: UInt64 = 25_000_000
        ) {
            precondition(maximumPolls > 0)
            self.maximumPolls = maximumPolls
            self.pollIntervalNanoseconds = pollIntervalNanoseconds
        }
    }

    @MainActor
    static func waitUntilActive(
        configuration: Configuration = Configuration(),
        isActive: @MainActor () -> Bool,
        sleep: @MainActor (UInt64) async throws -> Void = { nanoseconds in
            try await Task.sleep(nanoseconds: nanoseconds)
        }
    ) async -> Bool {
        if isActive() {
            return true
        }

        for _ in 0 ..< configuration.maximumPolls {
            do {
                try await sleep(configuration.pollIntervalNanoseconds)
            } catch {
                return false
            }
            if isActive() {
                return true
            }
        }
        return false
    }
}
