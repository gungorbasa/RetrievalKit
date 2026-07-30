#if canImport(CoreML)
import CryptoKit
import Foundation

enum SHA256Digest {
    static func hex(of data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    static func hex(ofFile url: URL) throws -> String {
        guard let stream = InputStream(url: url) else {
            throw EmbeddingKitError.artifactVerificationFailed(
                "cannot open \(url.lastPathComponent)"
            )
        }
        stream.open()
        defer { stream.close() }

        var hasher = SHA256()
        var buffer = [UInt8](repeating: 0, count: 1_048_576)
        while true {
            let count = stream.read(&buffer, maxLength: buffer.count)
            if count < 0 {
                throw stream.streamError ?? EmbeddingKitError.artifactVerificationFailed(
                    "cannot read \(url.lastPathComponent)"
                )
            }
            if count == 0 { break }
            hasher.update(data: Data(buffer[0..<count]))
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}
#endif
