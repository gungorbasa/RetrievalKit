import { Tokenizer } from "@huggingface/tokenizers";

import { MAX_INPUT_TOKENS } from "./constants.js";
import { EmbeddingArtifactError, EmbeddingInputError } from "./errors.js";

const CLS_TOKEN_ID = 101;
const SEP_TOKEN_ID = 102;
const PAD_TOKEN_ID = 0;
const CONTENT_TOKEN_LIMIT = MAX_INPUT_TOKENS - 2;

export interface TokenizedBatch {
  readonly inputIds: BigInt64Array;
  readonly attentionMask: BigInt64Array;
  readonly tokenTypeIds: BigInt64Array;
  readonly batchSize: number;
  readonly sequenceLength: number;
}

export class PinnedMiniLmTokenizer {
  readonly #tokenizer: Tokenizer;

  public constructor(tokenizerJson: Uint8Array, tokenizerConfigJson: Uint8Array) {
    try {
      const tokenizer = JSON.parse(new TextDecoder().decode(tokenizerJson)) as object;
      const config = JSON.parse(new TextDecoder().decode(tokenizerConfigJson)) as object;
      this.#tokenizer = new Tokenizer(tokenizer, config);
    } catch (error) {
      throw new EmbeddingArtifactError("The pinned tokenizer files are invalid.", error);
    }
    if (
      this.#tokenizer.token_to_id("[CLS]") !== CLS_TOKEN_ID ||
      this.#tokenizer.token_to_id("[SEP]") !== SEP_TOKEN_ID ||
      this.#tokenizer.token_to_id("[PAD]") !== PAD_TOKEN_ID
    ) {
      throw new EmbeddingArtifactError("The pinned tokenizer special-token IDs changed.");
    }
  }

  public tokenize(texts: readonly string[]): TokenizedBatch {
    if (texts.length === 0) throw new EmbeddingInputError("Embedding batch cannot be empty.");
    const rows = texts.map((text) => {
      if (text.trim().length === 0) {
        throw new EmbeddingInputError("Embedding text cannot be empty.");
      }
      const content = this.#tokenizer.encode(text, {
        add_special_tokens: false,
        return_token_type_ids: true
      }).ids.slice(0, CONTENT_TOKEN_LIMIT);
      return [CLS_TOKEN_ID, ...content, SEP_TOKEN_ID];
    });
    const sequenceLength = Math.max(...rows.map((row) => row.length));
    const elementCount = rows.length * sequenceLength;
    const inputIds = new BigInt64Array(elementCount);
    inputIds.fill(BigInt(PAD_TOKEN_ID));
    const attentionMask = new BigInt64Array(elementCount);
    const tokenTypeIds = new BigInt64Array(elementCount);
    rows.forEach((row, rowIndex) => {
      const offset = rowIndex * sequenceLength;
      row.forEach((id, column) => {
        inputIds[offset + column] = BigInt(id);
        attentionMask[offset + column] = 1n;
      });
    });
    return {
      inputIds,
      attentionMask,
      tokenTypeIds,
      batchSize: rows.length,
      sequenceLength
    };
  }
}
