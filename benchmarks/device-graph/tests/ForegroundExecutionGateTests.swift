import Foundation

@main
struct ForegroundExecutionGateTests {
    static func main() async {
        await passesImmediatelyWithoutSleeping()
        await waitsUntilActive()
        await failsClosedAtTheBound()
        await failsClosedWhenSleepIsCancelled()
    }

    @MainActor
    private static func passesImmediatelyWithoutSleeping() async {
        var sleeps = 0
        let ready = await ForegroundExecutionGate.waitUntilActive(
            configuration: .init(maximumPolls: 3, pollIntervalNanoseconds: 1_000_000),
            isActive: { true },
            sleep: { _ in sleeps += 1 }
        )
        precondition(ready)
        precondition(sleeps == 0)
    }

    @MainActor
    private static func waitsUntilActive() async {
        var active = false
        var sleeps = 0
        let ready = await ForegroundExecutionGate.waitUntilActive(
            configuration: .init(maximumPolls: 3, pollIntervalNanoseconds: 1_000_000),
            isActive: { active },
            sleep: { _ in
                sleeps += 1
                if sleeps == 2 {
                    active = true
                }
            }
        )
        precondition(ready)
        precondition(sleeps == 2)
    }

    @MainActor
    private static func failsClosedAtTheBound() async {
        var sleeps = 0
        let ready = await ForegroundExecutionGate.waitUntilActive(
            configuration: .init(maximumPolls: 3, pollIntervalNanoseconds: 1_000_000),
            isActive: { false },
            sleep: { _ in sleeps += 1 }
        )
        precondition(!ready)
        precondition(sleeps == 3)
    }

    @MainActor
    private static func failsClosedWhenSleepIsCancelled() async {
        enum Cancelled: Error {
            case requested
        }

        let ready = await ForegroundExecutionGate.waitUntilActive(
            configuration: .init(maximumPolls: 3, pollIntervalNanoseconds: 1_000_000),
            isActive: { false },
            sleep: { _ in throw Cancelled.requested }
        )
        precondition(!ready)
    }
}
