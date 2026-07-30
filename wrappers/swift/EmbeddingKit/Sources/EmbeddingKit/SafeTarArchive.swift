#if canImport(CoreML)
import Foundation

struct CoreMLArchiveManifest: Codable, Equatable, Sendable {
    struct File: Codable, Equatable, Sendable {
        let path: String
        let size: Int64
        let sha256: String
    }

    let schemaVersion: Int
    let artifactID: String
    let modelPath: String
    let tokenizerPath: String
    let canonicalTreeSHA256: String
    let files: [File]
}

enum SafeTarArchive {
    static let manifestPath = "archive-manifest-v1.json"
    private static let blockSize = 512

    private struct Entry {
        let path: String
        let type: UInt8
        let size: Int
        let payloadOffset: Int
    }

    static func extract(
        archiveURL: URL,
        to destination: URL,
        expectedArtifactID: String,
        expectedManifestSHA256: String
    ) throws -> CoreMLArchiveManifest {
        let data = try Data(contentsOf: archiveURL, options: .mappedIfSafe)
        let entries = try parse(data)
        guard let manifestEntry = entries.first(where: { $0.path == manifestPath }) else {
            throw EmbeddingKitError.unsafeArchive("missing \(manifestPath)")
        }
        let manifestData = data.subdata(
            in: manifestEntry.payloadOffset..<(manifestEntry.payloadOffset + manifestEntry.size)
        )
        guard SHA256Digest.hex(of: manifestData) == expectedManifestSHA256 else {
            throw EmbeddingKitError.artifactVerificationFailed("archive manifest hash mismatch")
        }

        let manifest: CoreMLArchiveManifest
        do {
            manifest = try JSONDecoder().decode(CoreMLArchiveManifest.self, from: manifestData)
        } catch {
            throw EmbeddingKitError.artifactVerificationFailed(
                "invalid archive manifest: \(error.localizedDescription)"
            )
        }
        guard manifest.schemaVersion == 1, manifest.artifactID == expectedArtifactID else {
            throw EmbeddingKitError.artifactVerificationFailed(
                "archive manifest identity mismatch"
            )
        }
        let canonicalTree = manifest.files.sorted { $0.path < $1.path }.map {
            "\($0.path)\0\($0.size)\0\($0.sha256)\n"
        }.joined()
        guard manifest.canonicalTreeSHA256 == SHA256Digest.hex(of: Data(canonicalTree.utf8)) else {
            throw EmbeddingKitError.artifactVerificationFailed(
                "archive canonical tree hash mismatch"
            )
        }
        try validateRelativePath(manifest.modelPath)
        try validateRelativePath(manifest.tokenizerPath)

        let listed = try Dictionary(
            manifest.files.map { file -> (String, CoreMLArchiveManifest.File) in
                try validateRelativePath(file.path)
                guard file.path != manifestPath, file.size >= 0,
                      file.sha256.count == 64,
                      file.sha256.allSatisfy({ $0.isHexDigit && !$0.isUppercase }) else {
                    throw EmbeddingKitError.artifactVerificationFailed(
                        "invalid file record for \(file.path)"
                    )
                }
                return (file.path, file)
            },
            uniquingKeysWith: { _, _ in
                throw EmbeddingKitError.unsafeArchive("duplicate manifest file record")
            }
        )
        let actualPaths = Set(entries.map(\.path))
        let expectedPaths = Set(listed.keys).union([manifestPath])
        guard actualPaths == expectedPaths else {
            let unexpected = actualPaths.subtracting(expectedPaths).sorted()
            let missing = expectedPaths.subtracting(actualPaths).sorted()
            throw EmbeddingKitError.unsafeArchive(
                "entry set mismatch; unexpected=\(unexpected), missing=\(missing)"
            )
        }
        guard listed.keys.contains(where: {
            $0 == manifest.modelPath || $0.hasPrefix(manifest.modelPath + "/")
        }) else {
            throw EmbeddingKitError.artifactVerificationFailed("modelPath is absent")
        }
        guard listed.keys.contains(manifest.tokenizerPath + "/tokenizer.json") else {
            throw EmbeddingKitError.artifactVerificationFailed("tokenizer.json is absent")
        }

        try FileManager.default.createDirectory(
            at: destination,
            withIntermediateDirectories: true
        )
        for entry in entries where entry.path != manifestPath {
            guard let expected = listed[entry.path], Int64(entry.size) == expected.size else {
                throw EmbeddingKitError.artifactVerificationFailed(
                    "size mismatch for \(entry.path)"
                )
            }
            let payload = data.subdata(
                in: entry.payloadOffset..<(entry.payloadOffset + entry.size)
            )
            guard SHA256Digest.hex(of: payload) == expected.sha256 else {
                throw EmbeddingKitError.artifactVerificationFailed(
                    "SHA-256 mismatch for \(entry.path)"
                )
            }
            let output = destination.appendingPathComponent(entry.path)
            try FileManager.default.createDirectory(
                at: output.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try payload.write(to: output, options: .atomic)
        }
        try manifestData.write(
            to: destination.appendingPathComponent(manifestPath),
            options: .atomic
        )
        return manifest
    }

    private static func parse(_ data: Data) throws -> [Entry] {
        var entries: [Entry] = []
        var paths = Set<String>()
        var offset = 0
        var foundTerminator = false

        while offset + blockSize <= data.count {
            let header = data[offset..<(offset + blockSize)]
            if header.allSatisfy({ $0 == 0 }) {
                guard offset + blockSize * 2 <= data.count,
                      data[(offset + blockSize)..<(offset + blockSize * 2)]
                        .allSatisfy({ $0 == 0 }) else {
                    throw EmbeddingKitError.unsafeArchive("tar has only one zero terminator")
                }
                foundTerminator = true
                let trailing = offset + blockSize * 2
                guard trailing == data.count
                    || data[trailing..<data.count].allSatisfy({ $0 == 0 }) else {
                    throw EmbeddingKitError.unsafeArchive("non-zero data after tar terminator")
                }
                break
            }
            guard ascii(header, 257, 6).hasPrefix("ustar") else {
                throw EmbeddingKitError.unsafeArchive("archive is not POSIX ustar")
            }
            let expectedChecksum = try parseOctal(
                ascii(header, 148, 8),
                field: "checksum"
            )
            let actualChecksum = header.enumerated().reduce(0) { partial, item in
                partial + ((148..<156).contains(item.offset) ? 32 : Int(item.element))
            }
            guard expectedChecksum == actualChecksum else {
                throw EmbeddingKitError.unsafeArchive("invalid tar header checksum")
            }
            let name = ascii(header, 0, 100)
            let prefix = ascii(header, 345, 155)
            let path = prefix.isEmpty ? name : "\(prefix)/\(name)"
            try validateRelativePath(path)
            guard paths.insert(path).inserted else {
                throw EmbeddingKitError.unsafeArchive("duplicate entry \(path)")
            }

            let type = header[header.index(header.startIndex, offsetBy: 156)]
            guard type == 0 || type == 48 else {
                throw EmbeddingKitError.unsafeArchive(
                    "non-regular or linked entry \(path)"
                )
            }
            let size = try parseOctal(ascii(header, 124, 12), field: "size")
            let payloadOffset = offset + blockSize
            guard size <= data.count - payloadOffset else {
                throw EmbeddingKitError.unsafeArchive("truncated entry \(path)")
            }
            entries.append(
                Entry(path: path, type: type, size: size, payloadOffset: payloadOffset)
            )
            let paddedSize = ((size + blockSize - 1) / blockSize) * blockSize
            offset = payloadOffset + paddedSize
        }
        guard foundTerminator else {
            throw EmbeddingKitError.unsafeArchive("missing tar terminator")
        }
        return entries
    }

    private static func ascii(_ bytes: Data.SubSequence, _ offset: Int, _ count: Int) -> String {
        let start = bytes.index(bytes.startIndex, offsetBy: offset)
        let end = bytes.index(start, offsetBy: count)
        return String(bytes: bytes[start..<end].prefix { $0 != 0 }, encoding: .utf8)?
            .trimmingCharacters(in: .whitespaces) ?? ""
    }

    private static func parseOctal(_ value: String, field: String) throws -> Int {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let result = Int(trimmed, radix: 8), result >= 0 else {
            throw EmbeddingKitError.unsafeArchive("invalid tar \(field)")
        }
        return result
    }

    private static func validateRelativePath(_ path: String) throws {
        guard !path.isEmpty, !path.hasPrefix("/"), !path.contains("\\"),
              !path.unicodeScalars.contains(where: { $0.value == 0 }) else {
            throw EmbeddingKitError.unsafeArchive("invalid path \(path)")
        }
        let components = path.split(separator: "/", omittingEmptySubsequences: false)
        guard components.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }) else {
            throw EmbeddingKitError.unsafeArchive("path traversal in \(path)")
        }
    }
}
#endif
