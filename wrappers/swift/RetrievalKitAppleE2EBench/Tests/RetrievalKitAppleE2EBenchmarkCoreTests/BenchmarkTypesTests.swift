import Testing
@testable import RetrievalKitAppleE2EBenchmarkCore

@Test func nearestRankSummaryUsesContractDefinition() throws {
    let summary = try StageSummary.calculate(Array(1...100).map(UInt64.init))
    #expect(summary.count == 100)
    #expect(summary.p50NS == 50)
    #expect(summary.p95NS == 95)
    #expect(summary.p99NS == 99)
    #expect(summary.minimumNS == 1)
    #expect(summary.maximumNS == 100)
}

@Test func emptySummaryIsRejected() {
    #expect(throws: AppleE2EBenchmarkError.self) {
        _ = try StageSummary.calculate([])
    }
}
