import Foundation

/// BERT-compatible WordPiece tokenizer backed by a Hugging Face `tokenizer.json`.
public struct BertWordPieceTokenizer: TextTokenizer {
    public let identifier: String
    public let sequenceLength: Int

    private let vocab: [String: Int32]
    private let lowercase: Bool
    private let cleanText: Bool
    private let handleChineseChars: Bool
    private let stripAccents: Bool
    private let continuingSubwordPrefix: String
    private let maxInputCharsPerWord: Int
    private let clsTokenID: Int32
    private let sepTokenID: Int32
    private let padTokenID: Int32
    private let unknownTokenID: Int32
    private let includeTokenTypeIDs: Bool

    /// Loads a tokenizer from a directory containing Hugging Face `tokenizer.json`.
    public init(
        tokenizerDirectory: URL,
        sequenceLength: Int,
        includeTokenTypeIDs: Bool = true
    ) throws {
        try self.init(
            tokenizerJSON: tokenizerDirectory.appendingPathComponent("tokenizer.json"),
            sequenceLength: sequenceLength,
            includeTokenTypeIDs: includeTokenTypeIDs
        )
    }

    /// Loads a tokenizer from a Hugging Face `tokenizer.json` file.
    public init(
        tokenizerJSON: URL,
        sequenceLength: Int,
        includeTokenTypeIDs: Bool = true
    ) throws {
        guard sequenceLength >= 2 else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "BERT tokenizers require sequenceLength of at least 2"
            )
        }

        let data = try Data(contentsOf: tokenizerJSON)
        let tokenizer = try JSONDecoder().decode(HuggingFaceTokenizer.self, from: data)
        guard tokenizer.model.type == "WordPiece" else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "unsupported tokenizer model '\(tokenizer.model.type)'"
            )
        }
        if let normalizer = tokenizer.normalizer, normalizer.type != "BertNormalizer" {
            throw EmbeddingKitError.unsupportedModelInterface(
                "unsupported normalizer '\(normalizer.type)'"
            )
        }
        if let preTokenizer = tokenizer.preTokenizer, preTokenizer.type != "BertPreTokenizer" {
            throw EmbeddingKitError.unsupportedModelInterface(
                "unsupported pre-tokenizer '\(preTokenizer.type)'"
            )
        }
        if let postProcessor = tokenizer.postProcessor, postProcessor.type != "TemplateProcessing" {
            throw EmbeddingKitError.unsupportedModelInterface(
                "unsupported post-processor '\(postProcessor.type)'"
            )
        }

        let specialTokens = tokenizer.postProcessor?.specialTokens ?? [:]
        self.identifier = tokenizerJSON.deletingLastPathComponent().lastPathComponent
        self.sequenceLength = sequenceLength
        self.vocab = tokenizer.model.vocab
        self.lowercase = tokenizer.normalizer?.lowercase ?? true
        self.cleanText = tokenizer.normalizer?.cleanText ?? true
        self.handleChineseChars = tokenizer.normalizer?.handleChineseChars ?? true
        self.stripAccents = tokenizer.normalizer?.stripAccents ?? self.lowercase
        self.continuingSubwordPrefix = tokenizer.model.continuingSubwordPrefix ?? "##"
        self.maxInputCharsPerWord = tokenizer.model.maxInputCharsPerWord ?? 100
        self.clsTokenID = try Self.specialTokenID(
            "[CLS]",
            vocab: tokenizer.model.vocab,
            specialTokens: specialTokens
        )
        self.sepTokenID = try Self.specialTokenID(
            "[SEP]",
            vocab: tokenizer.model.vocab,
            specialTokens: specialTokens
        )
        self.padTokenID = try Self.specialTokenID(
            "[PAD]",
            vocab: tokenizer.model.vocab,
            specialTokens: specialTokens
        )
        self.unknownTokenID = try Self.specialTokenID(
            tokenizer.model.unknownToken,
            vocab: tokenizer.model.vocab,
            specialTokens: specialTokens
        )
        self.includeTokenTypeIDs = includeTokenTypeIDs
    }

    public func tokenize(_ text: String) throws -> TokenizedText {
        guard !text.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        var ids = [clsTokenID]
        let tokenBudget = sequenceLength - 2
        for token in wordPieceTokens(for: text).prefix(tokenBudget) {
            ids.append(vocab[token] ?? unknownTokenID)
        }
        ids.append(sepTokenID)

        var attentionMask = Array(repeating: Int32(1), count: ids.count)
        if ids.count < sequenceLength {
            let paddingCount = sequenceLength - ids.count
            ids.append(contentsOf: Array(repeating: padTokenID, count: paddingCount))
            attentionMask.append(contentsOf: Array(repeating: Int32(0), count: paddingCount))
        }

        let tokenTypeIDs = includeTokenTypeIDs
            ? Array(repeating: Int32(0), count: sequenceLength)
            : nil
        return try TokenizedText(
            inputIDs: ids,
            attentionMask: attentionMask,
            tokenTypeIDs: tokenTypeIDs
        )
    }

    private func wordPieceTokens(for text: String) -> [String] {
        var pieces: [String] = []
        for token in basicTokens(for: text) {
            pieces.append(contentsOf: wordPieceTokens(forBasicToken: token))
        }
        return pieces
    }

    private func basicTokens(for text: String) -> [String] {
        var normalized = cleanText ? clean(text) : text
        if handleChineseChars {
            normalized = addSpacesAroundChineseChars(normalized)
        }

        return normalized
            .split(whereSeparator: isWhitespace)
            .flatMap { splitOnPunctuation(normalizeToken(String($0))) }
            .filter { !$0.isEmpty }
    }

    private func normalizeToken(_ token: String) -> String {
        var result = lowercase ? token.lowercased() : token
        if stripAccents {
            result = result.folding(
                options: .diacriticInsensitive,
                locale: Locale(identifier: "en_US_POSIX")
            )
        }
        return result
    }

    private func wordPieceTokens(forBasicToken token: String) -> [String] {
        let characters = Array(token).map(String.init)
        guard characters.count <= maxInputCharsPerWord else {
            return [unknownTokenString]
        }

        var subTokens: [String] = []
        var start = 0
        while start < characters.count {
            var end = characters.count
            var current: String?

            while start < end {
                var candidate = characters[start..<end].joined()
                if start > 0 {
                    candidate = continuingSubwordPrefix + candidate
                }
                if vocab[candidate] != nil {
                    current = candidate
                    break
                }
                end -= 1
            }

            guard let current else {
                return [unknownTokenString]
            }
            subTokens.append(current)
            start = end
        }
        return subTokens
    }

    private var unknownTokenString: String {
        vocab.first { $0.value == unknownTokenID }?.key ?? "[UNK]"
    }

    private static func specialTokenID(
        _ token: String,
        vocab: [String: Int32],
        specialTokens: [String: HuggingFaceSpecialToken]
    ) throws -> Int32 {
        if let id = specialTokens[token]?.ids.first {
            return id
        }
        if let id = vocab[token] {
            return id
        }
        throw EmbeddingKitError.unsupportedModelInterface("missing special token '\(token)'")
    }
}

private struct HuggingFaceTokenizer: Decodable {
    var normalizer: HuggingFaceBertNormalizer?
    var preTokenizer: HuggingFaceTypeConfig?
    var postProcessor: HuggingFacePostProcessor?
    var model: HuggingFaceWordPieceModel

    enum CodingKeys: String, CodingKey {
        case normalizer
        case preTokenizer = "pre_tokenizer"
        case postProcessor = "post_processor"
        case model
    }
}

private struct HuggingFaceTypeConfig: Decodable {
    var type: String
}

private struct HuggingFaceBertNormalizer: Decodable {
    var type: String
    var cleanText: Bool?
    var handleChineseChars: Bool?
    var stripAccents: Bool?
    var lowercase: Bool?

    enum CodingKeys: String, CodingKey {
        case type
        case cleanText = "clean_text"
        case handleChineseChars = "handle_chinese_chars"
        case stripAccents = "strip_accents"
        case lowercase
    }
}

private struct HuggingFacePostProcessor: Decodable {
    var type: String
    var specialTokens: [String: HuggingFaceSpecialToken]?

    enum CodingKeys: String, CodingKey {
        case type
        case specialTokens = "special_tokens"
    }
}

private struct HuggingFaceSpecialToken: Decodable {
    var ids: [Int32]
}

private struct HuggingFaceWordPieceModel: Decodable {
    var type: String
    var unknownToken: String
    var continuingSubwordPrefix: String?
    var maxInputCharsPerWord: Int?
    var vocab: [String: Int32]

    enum CodingKeys: String, CodingKey {
        case type
        case unknownToken = "unk_token"
        case continuingSubwordPrefix = "continuing_subword_prefix"
        case maxInputCharsPerWord = "max_input_chars_per_word"
        case vocab
    }
}

private func clean(_ text: String) -> String {
    var result = ""
    result.reserveCapacity(text.count)
    for scalar in text.unicodeScalars {
        if scalar.value == 0 || scalar.value == 0xfffd || isControl(scalar) {
            continue
        }
        result.append(isWhitespace(scalar) ? " " : String(scalar))
    }
    return result
}

private func addSpacesAroundChineseChars(_ text: String) -> String {
    var result = ""
    result.reserveCapacity(text.count)
    for scalar in text.unicodeScalars {
        if isChineseChar(scalar) {
            result.append(" ")
            result.append(String(scalar))
            result.append(" ")
        } else {
            result.append(String(scalar))
        }
    }
    return result
}

private func splitOnPunctuation(_ token: String) -> [String] {
    var tokens: [String] = []
    var current = ""
    for scalar in token.unicodeScalars {
        if isPunctuation(scalar) {
            if !current.isEmpty {
                tokens.append(current)
                current.removeAll(keepingCapacity: true)
            }
            tokens.append(String(scalar))
        } else {
            current.append(String(scalar))
        }
    }
    if !current.isEmpty {
        tokens.append(current)
    }
    return tokens
}

private func isWhitespace(_ character: Character) -> Bool {
    character.unicodeScalars.allSatisfy(isWhitespace)
}

private func isWhitespace(_ scalar: UnicodeScalar) -> Bool {
    scalar.properties.isWhitespace
}

private func isControl(_ scalar: UnicodeScalar) -> Bool {
    if scalar == "\t" || scalar == "\n" || scalar == "\r" {
        return false
    }
    return scalar.properties.generalCategory == .control
}

private func isPunctuation(_ scalar: UnicodeScalar) -> Bool {
    if (33...47).contains(scalar.value)
        || (58...64).contains(scalar.value)
        || (91...96).contains(scalar.value)
        || (123...126).contains(scalar.value) {
        return true
    }

    switch scalar.properties.generalCategory {
    case .connectorPunctuation,
         .dashPunctuation,
         .openPunctuation,
         .closePunctuation,
         .initialPunctuation,
         .finalPunctuation,
         .otherPunctuation:
        return true
    default:
        return false
    }
}

private func isChineseChar(_ scalar: UnicodeScalar) -> Bool {
    let value = scalar.value
    return (0x4e00...0x9fff).contains(value)
        || (0x3400...0x4dbf).contains(value)
        || (0x20000...0x2a6df).contains(value)
        || (0x2a700...0x2b73f).contains(value)
        || (0x2b740...0x2b81f).contains(value)
        || (0x2b820...0x2ceaf).contains(value)
        || (0xf900...0xfaff).contains(value)
        || (0x2f800...0x2fa1f).contains(value)
}
