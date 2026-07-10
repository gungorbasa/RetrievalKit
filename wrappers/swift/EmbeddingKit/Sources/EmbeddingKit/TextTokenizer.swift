import Foundation

/// Tokenized text inputs expected by transformer-style embedding models.
public struct TokenizedText: Equatable, Sendable {
    /// Token IDs for the model vocabulary.
    public var inputIDs: [Int32]
    /// Attention mask aligned with `inputIDs`.
    public var attentionMask: [Int32]
    /// Optional token type IDs aligned with `inputIDs`.
    public var tokenTypeIDs: [Int32]?

    public init(
        inputIDs: [Int32],
        attentionMask: [Int32],
        tokenTypeIDs: [Int32]? = nil
    ) throws {
        guard !inputIDs.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }
        guard attentionMask.count == inputIDs.count else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "attention mask count \(attentionMask.count) does not match input ID count \(inputIDs.count)"
            )
        }
        if let tokenTypeIDs, tokenTypeIDs.count != inputIDs.count {
            throw EmbeddingKitError.unsupportedModelInterface(
                "token type ID count \(tokenTypeIDs.count) does not match input ID count \(inputIDs.count)"
            )
        }

        self.inputIDs = inputIDs
        self.attentionMask = attentionMask
        self.tokenTypeIDs = tokenTypeIDs
    }
}

/// Counts model tokens without truncating to the model input sequence length.
public protocol TextTokenCounter: Sendable {
    /// Returns the number of tokens the model tokenizer would consume, including special tokens.
    func countTokens(in text: String) throws -> Int
}

/// Provider-neutral tokenizer boundary used by model-backed embedders.
public protocol TextTokenizer: Sendable {
    /// Tokenizer identifier or revision used for diagnostics.
    var identifier: String { get }

    /// Tokenizes one text string.
    func tokenize(_ text: String) throws -> TokenizedText
    /// Tokenizes a batch of text strings.
    func tokenize(_ texts: [String]) throws -> [TokenizedText]
}

public extension TextTokenizer {
    func tokenize(_ texts: [String]) throws -> [TokenizedText] {
        guard !texts.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }
        return try texts.map { try tokenize($0) }
    }
}
