import { binding, type NativeRetrievalHandle } from "./binding.js";
import type {
  NativeFilter,
  NativeHybridHit,
  NativeMetadataEntry,
  NativeMetadataValue,
  NativeSearchHit
} from "./native-types.js";

export type VectorMetric = "cosine" | "dotProduct";
export type VectorEncoding = "f32" | "f16" | "bf16" | "i8";

export interface TimestampMillis {
  readonly kind: "timestampMillis";
  readonly value: bigint;
}

export interface FloatingPoint {
  readonly kind: "float";
  readonly value: number;
}

export type MetadataValue =
  | string
  | boolean
  | bigint
  | number
  | TimestampMillis
  | FloatingPoint;
export type Metadata = Readonly<Record<string, MetadataValue>>;

export function timestampMillis(value: bigint | number): TimestampMillis {
  return { kind: "timestampMillis", value: safeBigInt(value, "timestamp") };
}

export function floatingPoint(value: number): FloatingPoint {
  if (!Number.isFinite(value)) {
    throw new RetrievalKitInputError(
      "Floating-point metadata must be finite.",
      "RK_INVALID_INPUT"
    );
  }
  return { kind: "float", value };
}

export interface EqualsFilter {
  readonly kind: "equals";
  readonly field: string;
  readonly value: MetadataValue;
}
export interface NotEqualsFilter {
  readonly kind: "notEquals";
  readonly field: string;
  readonly value: MetadataValue;
}
export interface InFilter {
  readonly kind: "in";
  readonly field: string;
  readonly values: readonly MetadataValue[];
}
export interface RangeFilter {
  readonly kind: "range";
  readonly field: string;
  readonly lower?: MetadataValue;
  readonly upper?: MetadataValue;
}
export interface ExistsFilter {
  readonly kind: "exists";
  readonly field: string;
}
export interface AllFilter {
  readonly kind: "all";
  readonly filters: readonly Filter[];
}
export interface AnyFilter {
  readonly kind: "any";
  readonly filters: readonly Filter[];
}
export type Filter =
  | EqualsFilter
  | NotEqualsFilter
  | InFilter
  | RangeFilter
  | ExistsFilter
  | AllFilter
  | AnyFilter;

export interface DocumentInput {
  readonly id: string;
  readonly text: string;
  readonly embedding: Float32Array;
  readonly metadata?: Metadata;
}

export interface RetrievalBuilderOptions {
  readonly corpusId: string;
  readonly metric?: VectorMetric;
  readonly encoding?: VectorEncoding;
}

export interface VectorSearch {
  readonly mode: "vector";
  readonly embedding: Float32Array;
  readonly limit?: number;
  readonly where?: Filter;
}

export interface TextSearch {
  readonly mode: "text";
  readonly text: string;
  readonly limit?: number;
  readonly where?: Filter;
  readonly keywordCandidates?: number;
}

export interface HybridSearch {
  readonly mode: "hybrid";
  readonly text: string;
  readonly embedding?: Float32Array;
  readonly alpha?: number;
  readonly limit?: number;
  readonly where?: Filter;
  readonly vectorCandidates?: number;
  readonly keywordCandidates?: number;
}

export type SearchQuery = VectorSearch | TextSearch | HybridSearch;

export interface VectorTrace {
  readonly kind: "vector";
  readonly vectorScore: number;
}

export interface HybridTrace {
  readonly kind: "hybrid";
  readonly alpha: number;
  readonly vectorRank?: number;
  readonly keywordRank?: number;
  readonly normalizedVectorScore?: number;
  readonly normalizedKeywordScore?: number;
  readonly matchedTerms: readonly string[];
}

export interface SearchResult {
  readonly documentId: string;
  readonly text: string;
  readonly metadata: Readonly<Record<string, MetadataValue>>;
  readonly score: number;
  readonly vectorScore?: number;
  readonly keywordScore?: number;
  readonly trace: VectorTrace | HybridTrace;
}

export interface FileSizeReport {
  readonly manifestBytes: number;
  readonly vectorsBytes: number;
  readonly chunksBytes: number;
  readonly recordsBytes: number;
  readonly bm25Bytes: number;
  readonly tombstonesBytes: number;
  readonly totalBytes: number;
}

export class RetrievalKitError extends Error {
  public constructor(
    message: string,
    public readonly code: string,
    options?: ErrorOptions
  ) {
    super(message, options);
    this.name = new.target.name;
  }
}
export class RetrievalKitInputError extends RetrievalKitError {}
export class RetrievalKitDimensionError extends RetrievalKitError {}
export class RetrievalKitLifecycleError extends RetrievalKitError {}
export class RetrievalKitPersistenceError extends RetrievalKitError {}
export class RetrievalKitQueryError extends RetrievalKitError {}

export class RetrievalDatabaseBuilder {
  readonly #native: NativeRetrievalHandle;
  #consumed = false;
  #transferred = false;
  #closing?: Promise<void>;

  public constructor(options: RetrievalBuilderOptions) {
    this.#native = new binding.NativeRetrievalHandle(
      options.corpusId,
      options.metric ?? "cosine",
      options.encoding ?? "i8"
    );
  }

  public async add(documents: Iterable<DocumentInput>): Promise<void> {
    this.#requireActive();
    const values = [...documents].map(toNativeDocument);
    if (values.length === 0) return;
    await nativeCall(() => this.#native.addDocuments(values));
  }

  public async build(): Promise<RetrievalDatabase> {
    this.#requireActive();
    try {
      await nativeCall(() => this.#native.build());
      this.#consumed = true;
      this.#transferred = true;
      return createRetrievalDatabase(this.#native);
    } catch (error) {
      this.#consumed = true;
      throw error;
    }
  }

  public close(): Promise<void> {
    if (this.#transferred) return Promise.resolve();
    this.#consumed = true;
    this.#closing ??= nativeCall(() => this.#native.close());
    return this.#closing;
  }

  public [Symbol.dispose](): void {
    void this.close();
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }

  #requireActive(): void {
    if (this.#consumed) {
      throw new RetrievalKitLifecycleError(
        "RetrievalDatabaseBuilder has already been consumed; create a new builder.",
        "RK_LIFECYCLE"
      );
    }
  }
}

const retrievalNatives = new WeakMap<RetrievalDatabase, NativeRetrievalHandle>();

export abstract class RetrievalDatabase {
  #closing?: Promise<void>;

  protected constructor() {}

  public static async load(path: string): Promise<RetrievalDatabase> {
    const native = binding.NativeRetrievalHandle.empty();
    await nativeCall(() => native.load(path));
    return createRetrievalDatabase(native);
  }

  public static async validate(path: string): Promise<void> {
    await nativeCall(() => binding.validateRetrieval(path));
  }

  public get closed(): boolean {
    return nativeFor(this).closed;
  }

  public async search(query: SearchQuery): Promise<SearchResult[]> {
    const limit = positiveInteger(query.limit ?? 10, "limit");
    const where = query.where === undefined ? undefined : toNativeFilter(query.where);
    switch (query.mode) {
      case "vector": {
        const hits = await nativeCall(() =>
          nativeFor(this).semanticSearch(query.embedding, limit, where)
        );
        return hits.map(fromNativeVectorHit);
      }
      case "text": {
        const hits = await nativeCall(() =>
          nativeFor(this).hybridSearch(
            query.text,
            undefined,
            limit,
            where,
            0,
            0,
            optionalPositiveInteger(query.keywordCandidates, "keywordCandidates")
          )
        );
        return hits.map(fromNativeHybridHit);
      }
      case "hybrid": {
        const alpha = query.alpha ?? 0.6;
        const hits = await nativeCall(() =>
          nativeFor(this).hybridSearch(
            query.text,
            query.embedding,
            limit,
            where,
            alpha,
            optionalPositiveInteger(query.vectorCandidates, "vectorCandidates"),
            optionalPositiveInteger(query.keywordCandidates, "keywordCandidates")
          )
        );
        return hits.map(fromNativeHybridHit);
      }
    }
  }

  public async save(path: string): Promise<FileSizeReport> {
    return nativeCall(() => nativeFor(this).save(path));
  }

  public close(): Promise<void> {
    this.#closing ??= nativeCall(() => nativeFor(this).close());
    return this.#closing;
  }

  public [Symbol.dispose](): void {
    void this.close();
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}

class RetrievalDatabaseImpl extends RetrievalDatabase {
  public constructor(native: NativeRetrievalHandle) {
    super();
    retrievalNatives.set(this, native);
  }
}

function createRetrievalDatabase(native: NativeRetrievalHandle): RetrievalDatabase {
  return new RetrievalDatabaseImpl(native);
}

function nativeFor(database: RetrievalDatabase): NativeRetrievalHandle {
  const native = retrievalNatives.get(database);
  if (native === undefined) {
    throw new RetrievalKitLifecycleError(
      "RetrievalDatabase native ownership is unavailable; create it with a builder or load().",
      "RK_LIFECYCLE"
    );
  }
  return native;
}

function toNativeDocument(document: DocumentInput) {
  if (!(document.embedding instanceof Float32Array)) {
    throw new RetrievalKitInputError(
      `Document '${document.id}' embedding must be a Float32Array.`,
      "RK_INVALID_INPUT"
    );
  }
  return {
    id: document.id,
    text: document.text,
    metadata: toNativeMetadata(document.metadata),
    embedding: document.embedding
  };
}

function toNativeMetadata(metadata: Metadata | undefined): NativeMetadataEntry[] {
  return Object.entries(metadata ?? {}).map(([field, value]) => ({
    field,
    value: toNativeMetadataValue(value)
  }));
}

function toNativeMetadataValue(value: MetadataValue): NativeMetadataValue {
  if (typeof value === "string") return { kind: "string", stringValue: value };
  if (typeof value === "boolean") return { kind: "boolean", booleanValue: value };
  if (typeof value === "bigint") return { kind: "integer", integerValue: value.toString() };
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new RetrievalKitInputError("Metadata numbers must be finite.", "RK_INVALID_INPUT");
    }
    if (Number.isSafeInteger(value)) {
      return { kind: "integer", integerValue: BigInt(value).toString() };
    }
    return { kind: "float", numberValue: value };
  }
  if (value.kind === "timestampMillis") {
    return { kind: "timestamp", integerValue: value.value.toString() };
  }
  return { kind: "float", numberValue: value.value };
}

function fromNativeMetadata(entries: NativeMetadataEntry[]): Record<string, MetadataValue> {
  return Object.fromEntries(entries.map(({ field, value }) => [field, fromNativeValue(value)]));
}

function fromNativeValue(value: NativeMetadataValue): MetadataValue {
  switch (value.kind) {
    case "string":
      return value.stringValue ?? "";
    case "boolean":
      return value.booleanValue ?? false;
    case "integer":
      return BigInt(value.integerValue ?? "0");
    case "timestamp":
      return timestampMillis(BigInt(value.integerValue ?? "0"));
    case "float":
      return floatingPoint(value.numberValue ?? 0);
    default:
      throw new RetrievalKitError(
        `Native addon returned unknown metadata kind '${value.kind}'.`,
        "RK_NATIVE_CONTRACT"
      );
  }
}

function toNativeFilter(filter: Filter): NativeFilter {
  switch (filter.kind) {
    case "equals":
    case "notEquals":
      return {
        kind: filter.kind,
        field: filter.field,
        value: toNativeMetadataValue(filter.value)
      };
    case "in":
      return {
        kind: "in",
        field: filter.field,
        values: filter.values.map(toNativeMetadataValue)
      };
    case "range":
      return {
        kind: "range",
        field: filter.field,
        ...(filter.lower === undefined
          ? {}
          : { lower: toNativeMetadataValue(filter.lower) }),
        ...(filter.upper === undefined
          ? {}
          : { upper: toNativeMetadataValue(filter.upper) })
      };
    case "exists":
      return { kind: "exists", field: filter.field };
    case "all":
    case "any":
      return { kind: filter.kind, children: filter.filters.map(toNativeFilter) };
  }
}

function fromNativeVectorHit(hit: NativeSearchHit): SearchResult {
  return {
    documentId: hit.documentId,
    text: hit.text,
    metadata: fromNativeMetadata(hit.metadata),
    score: hit.score,
    vectorScore: hit.vectorScore,
    trace: { kind: "vector", vectorScore: hit.vectorScore }
  };
}

function fromNativeHybridHit(hit: NativeHybridHit): SearchResult {
  return {
    documentId: hit.documentId,
    text: hit.text,
    metadata: fromNativeMetadata(hit.metadata),
    score: hit.score,
    ...(hit.vectorScore === undefined ? {} : { vectorScore: hit.vectorScore }),
    ...(hit.keywordScore === undefined ? {} : { keywordScore: hit.keywordScore }),
    trace: {
      kind: "hybrid",
      alpha: hit.trace.alpha,
      ...(hit.trace.vectorRank === undefined ? {} : { vectorRank: hit.trace.vectorRank }),
      ...(hit.trace.keywordRank === undefined ? {} : { keywordRank: hit.trace.keywordRank }),
      ...(hit.trace.normalizedVectorScore === undefined
        ? {}
        : { normalizedVectorScore: hit.trace.normalizedVectorScore }),
      ...(hit.trace.normalizedKeywordScore === undefined
        ? {}
        : { normalizedKeywordScore: hit.trace.normalizedKeywordScore }),
      matchedTerms: hit.trace.matchedTerms
    }
  };
}

function positiveInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value <= 0 || value > 0xffff_ffff) {
    throw new RetrievalKitInputError(
      `Invalid ${name} ${String(value)}; expected an integer from 1 through 4294967295.`,
      "RK_INVALID_INPUT"
    );
  }
  return value;
}

function optionalPositiveInteger(value: number | undefined, name: string): number | undefined {
  return value === undefined ? undefined : positiveInteger(value, name);
}

function safeBigInt(value: bigint | number, name: string): bigint {
  if (typeof value === "bigint") return value;
  if (!Number.isSafeInteger(value)) {
    throw new RetrievalKitInputError(
      `${name} must be a bigint or safe integer; got ${String(value)}.`,
      "RK_INVALID_INPUT"
    );
  }
  return BigInt(value);
}

async function nativeCall<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw normalizeError(error);
  }
}

function normalizeError(error: unknown): RetrievalKitError {
  if (error instanceof RetrievalKitError) return error;
  const cause = error instanceof Error ? error : new Error(String(error));
  const match = /^(RK_[A-Z_]+):\s*(.*)$/s.exec(cause.message);
  const code = match?.[1] ?? "RK_NATIVE";
  const message = match?.[2] ?? cause.message;
  const Constructor =
    code === "RK_DIMENSION"
      ? RetrievalKitDimensionError
      : code === "RK_CLOSED" || code === "RK_LIFECYCLE"
        ? RetrievalKitLifecycleError
        : code === "RK_PERSISTENCE"
          ? RetrievalKitPersistenceError
          : code === "RK_INVALID_QUERY"
            ? RetrievalKitQueryError
            : code === "RK_INVALID_INPUT" || code === "RK_MISSING_EMBEDDING"
              ? RetrievalKitInputError
              : RetrievalKitError;
  return new Constructor(message, code, { cause });
}
