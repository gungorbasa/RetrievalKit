import Foundation
import RetrievalKitFFI

public struct RetrievalKitRuntimeCapabilities: Codable, Equatable, Sendable {
    public let simsimd: String
    public let aarch64Dotprod: Bool

    enum CodingKeys: String, CodingKey {
        case simsimd
        case aarch64Dotprod = "aarch64_dotprod"
    }
}

public enum RetrievalKitRuntimeDiagnostics {
    public static func capabilities() throws -> RetrievalKitRuntimeCapabilities {
        guard let pointer = retrievalkit_runtime_capabilities_json() else {
            throw RuntimeDiagnosticsError.unavailable
        }
        defer { retrievalkit_string_free(pointer) }
        do {
            return try JSONDecoder().decode(
                RetrievalKitRuntimeCapabilities.self,
                from: Data(String(cString: pointer).utf8)
            )
        } catch {
            throw RuntimeDiagnosticsError.invalidJSON(String(describing: error))
        }
    }
}

public enum RuntimeDiagnosticsError: Error, Equatable, Sendable {
    case unavailable
    case invalidJSON(String)
}
