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

export type Filter =
  | { readonly kind: "equals"; readonly field: string; readonly value: MetadataValue }
  | { readonly kind: "notEquals"; readonly field: string; readonly value: MetadataValue }
  | { readonly kind: "in"; readonly field: string; readonly values: readonly MetadataValue[] }
  | {
      readonly kind: "range";
      readonly field: string;
      readonly lower?: MetadataValue;
      readonly upper?: MetadataValue;
    }
  | { readonly kind: "exists"; readonly field: string }
  | { readonly kind: "all"; readonly filters: readonly Filter[] }
  | { readonly kind: "any"; readonly filters: readonly Filter[] };

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

export interface SearchControl {
  readonly signal?: AbortSignal;
  /**
   * Starting another request with the same key rejects and cancels the previous
   * request. This is useful for type-ahead search.
   */
  readonly supersedeKey?: string;
}

export interface VectorSearch {
  readonly mode: "vector";
  readonly embedding: Float32Array;
  readonly limit?: number;
  readonly where?: Filter;
  readonly within?: GraphSelectionReference;
}

export interface TextSearch {
  readonly mode: "text";
  readonly text: string;
  readonly limit?: number;
  readonly where?: Filter;
  readonly keywordCandidates?: number;
  readonly within?: GraphSelectionReference;
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
  readonly within?: GraphSelectionReference;
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
  readonly metadata: Metadata;
  readonly score: number;
  readonly vectorScore?: number;
  readonly keywordScore?: number;
  readonly trace: VectorTrace | HybridTrace;
}

export interface RecordValueMap {
  readonly [field: string]: RecordValue;
}
export type RecordValue =
  | null
  | string
  | boolean
  | bigint
  | number
  | readonly RecordValue[]
  | RecordValueMap;

export interface RecordNodeSchema {
  readonly recordType: string;
  readonly nodeType: string;
  readonly queryableFields?: readonly (readonly string[])[];
}

export interface RelationshipSchema {
  readonly relationship: string;
  readonly sourceNodeType: string;
  readonly targetNodeType: string;
  readonly sourceField: readonly string[];
  readonly cardinality: "one" | "optionalOne" | "many";
  readonly missingTarget?: "error" | "omitEdge";
  readonly duplicateReferences?: "error" | "deduplicate";
  readonly allowSelfEdge?: boolean;
  readonly inverseRelationship?: string;
}

export interface GraphSchema {
  readonly recordNodes: readonly RecordNodeSchema[];
  readonly relationships?: readonly RelationshipSchema[];
  readonly chunkNodes?: {
    readonly nodeType: string;
    readonly ownsRelationship: string;
    readonly inverseRelationship?: string;
  };
}

export interface GraphRecord {
  readonly id: string;
  readonly type: string;
  readonly fields?: Readonly<Record<string, RecordValue>>;
  readonly content?: string;
  readonly metadata?: Metadata;
}

export interface GraphOnlyRecordInput extends GraphRecord {
  readonly retrieval?: never;
}

export interface GraphRetrievalRecordInput extends GraphRecord {
  readonly retrieval?:
    | { readonly kind: "content"; readonly embedding: Float32Array }
    | {
        readonly kind: "documents";
        readonly documents: readonly DocumentInput[];
      };
}

export interface GraphBuilderOptions {
  readonly corpusId: string;
  readonly schema: GraphSchema;
}

export interface GraphRetrievalBuilderOptions extends GraphBuilderOptions {
  readonly metric?: VectorMetric;
  readonly encoding?: VectorEncoding;
}

export interface RecordNodeId {
  readonly kind: "record";
  readonly nodeType: string;
  readonly recordId: string;
}

export interface ChunkNodeId {
  readonly kind: "chunk";
  readonly nodeType: string;
  readonly recordId: string;
  readonly chunkKey: string;
}

export type GraphNodeId = RecordNodeId | ChunkNodeId;
export type GraphScalar = string | bigint | boolean;
export type GraphSeed =
  | { readonly kind: "nodes"; readonly nodes: readonly GraphNodeId[] }
  | {
      readonly kind: "equals";
      readonly nodeType: string;
      readonly field: readonly string[];
      readonly values: readonly GraphScalar[];
    };

export interface GraphQuery {
  readonly seed: GraphSeed;
  readonly traverse?: readonly {
    readonly relationship: string;
    readonly direction?: "outgoing" | "incoming";
    readonly minHops?: number;
    readonly maxHops?: number;
  }[];
  readonly limits?: {
    readonly maxHops?: number;
    readonly maxVisited?: number;
    readonly maxResults?: number;
    readonly maxWorkingBytes?: number;
  };
}

export interface GraphPathEdge {
  readonly relationship: string;
  readonly source: GraphNodeId;
  readonly target: GraphNodeId;
  readonly occurrenceOrdinal: number;
  readonly provenance: {
    readonly schemaRuleIndex: number;
    readonly sourceRecordId: string;
    readonly sourceField?: readonly string[];
    readonly derivedInverse: boolean;
    readonly builtIn: boolean;
  };
}

export interface GraphMatch {
  readonly node: GraphNodeId;
  readonly depth: number;
  readonly path: readonly GraphPathEdge[];
}

export type GraphTruncationReason =
  | "maxHops"
  | "maxVisited"
  | "maxResults"
  | "maxWorkingBytes";

export interface GraphSelectionData {
  readonly matches: readonly GraphMatch[];
  readonly truncated?: GraphTruncationReason;
  readonly trace: {
    readonly seedCount: number;
    readonly visitedStates: number;
    readonly traversedEdges: number;
    readonly resultCount: number;
    readonly diagnostics: number;
  };
}

export interface CandidateProjection {
  readonly candidates: readonly {
    readonly recordId: string;
    readonly chunkKey: string;
  }[];
  readonly sourceNodes: number;
  readonly projectedChunksBeforeFilter: number;
  readonly projectedChunksAfterFilter: number;
}

/**
 * Deliberately opaque. Only GraphSelection instances returned by this package
 * satisfy this interface at runtime.
 */
export interface GraphSelectionReference {
  readonly matches: readonly GraphMatch[];
  readonly truncated: GraphTruncationReason | undefined;
  readonly trace: GraphSelectionData["trace"];
  readonly closed: boolean;
  close(): Promise<void>;
}
