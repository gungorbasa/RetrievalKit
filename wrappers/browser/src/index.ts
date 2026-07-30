export {
  GraphDatabase,
  GraphDatabaseBuilder,
  GraphRetrievalDatabase,
  GraphRetrievalDatabaseBuilder,
  GraphSelection,
  RetrievalDatabase,
  RetrievalDatabaseBuilder,
  RetrievalKitBrowser,
  type GraphQueryOperations,
  type RetrievalKitBrowserOptions,
  type RetrievalOperations
} from "./databases.js";
export {
  RetrievalKitCancelledError,
  RetrievalKitDimensionError,
  RetrievalKitError,
  RetrievalKitGraphError,
  RetrievalKitInputError,
  RetrievalKitLifecycleError,
  RetrievalKitPersistenceError,
  RetrievalKitQueryError,
  RetrievalKitStaleSelectionError,
  RetrievalKitWorkerError
} from "./errors.js";
export {
  browserSupportsWasmSimd128,
  createAdaptiveGeneratedWasmAdapter,
  createGeneratedWasmAdapter,
  type BrowserCapabilities,
  type GeneratedGraphDatabase,
  type GeneratedGraphRetrievalDatabase,
  type GeneratedRetrievalDatabase,
  type GeneratedWasmTierLoaders,
  type GeneratedWasmModule,
  type RetrievalKitWasmAdapter
} from "./adapter.js";
export type {
  CandidateProjection,
  ChunkNodeId,
  DocumentInput,
  Filter,
  FloatingPoint,
  GraphBuilderOptions,
  GraphMatch,
  GraphNodeId,
  GraphOnlyRecordInput,
  GraphQuery,
  GraphRecord,
  GraphRetrievalBuilderOptions,
  GraphRetrievalRecordInput,
  GraphSchema,
  GraphSelectionData,
  GraphSelectionReference,
  GraphTruncationReason,
  HybridSearch,
  HybridTrace,
  Metadata,
  MetadataValue,
  RecordNodeId,
  RecordValue,
  RelationshipSchema,
  RetrievalBuilderOptions,
  SearchControl,
  SearchQuery,
  SearchResult,
  TextSearch,
  TimestampMillis,
  VectorEncoding,
  VectorMetric,
  VectorSearch,
  VectorTrace
} from "./types.js";

export function timestampMillis(value: bigint | number): {
  readonly kind: "timestampMillis";
  readonly value: bigint;
} {
  return { kind: "timestampMillis", value: BigInt(value) };
}

export function floatingPoint(value: number): {
  readonly kind: "float";
  readonly value: number;
} {
  if (!Number.isFinite(value)) {
    throw new TypeError("Floating-point metadata must be finite.");
  }
  return { kind: "float", value };
}
