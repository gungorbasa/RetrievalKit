export interface NativeMetadataValue {
  kind: string;
  stringValue?: string;
  integerValue?: string;
  numberValue?: number;
  booleanValue?: boolean;
}
export interface NativeMetadataEntry {
  field: string;
  value: NativeMetadataValue;
}
export interface NativeRecordValue {
  kind: string;
  stringValue?: string;
  integerValue?: string;
  numberValue?: number;
  booleanValue?: boolean;
  listValue?: NativeRecordValue[];
  mapValue?: NativeRecordField[];
}
export interface NativeRecordField {
  field: string;
  value: NativeRecordValue;
}
export interface NativeFilter {
  kind: string;
  field?: string;
  value?: NativeMetadataValue;
  values?: NativeMetadataValue[];
  lower?: NativeMetadataValue;
  upper?: NativeMetadataValue;
  children?: NativeFilter[];
}
export interface NativeRecordNodeSchema {
  recordType: string;
  nodeType: string;
  queryableFields: string[][];
}
export interface NativeRelationshipSchema {
  relationshipType: string;
  sourceNodeType: string;
  targetNodeType: string;
  sourceField: string[];
  cardinality: string;
  missingTarget: string;
  duplicateReferences: string;
  allowSelfEdge: boolean;
  inverseRelationship?: string;
}
export interface NativeChunkNodeSchema {
  nodeType: string;
  ownsRelationship: string;
  inverseRelationship?: string;
}
export interface NativeGraphSchema {
  recordNodes: NativeRecordNodeSchema[];
  relationships: NativeRelationshipSchema[];
  chunkNodes?: NativeChunkNodeSchema;
}
export interface NativeGraphDocumentInput {
  id: string;
  text: string;
  metadata: NativeMetadataEntry[];
  embedding: Float32Array;
}
export interface NativeGraphRecordInput {
  id: string;
  recordType: string;
  fields: NativeRecordField[];
  content?: string;
  metadata: NativeMetadataEntry[];
  embedding?: Float32Array;
  documents: NativeGraphDocumentInput[];
}
export interface NativeNodeId {
  nodeType: string;
  sourceKind: string;
  recordId: string;
  chunkKey?: string;
}
export interface NativeGraphScalar {
  kind: string;
  stringValue?: string;
  integerValue?: string;
  booleanValue?: boolean;
}
export interface NativeGraphSeed {
  kind: string;
  nodes?: NativeNodeId[];
  nodeType?: string;
  field?: string[];
  values?: NativeGraphScalar[];
}
export interface NativeTraverse {
  relationship: string;
  direction: string;
  minHops: number;
  maxHops: number;
}
export interface NativeGraphLimits {
  maxHops: number;
  maxVisited: number;
  maxResults: number;
  maxWorkingBytes: number;
}
export interface NativeGraphQuery {
  seed: NativeGraphSeed;
  steps: NativeTraverse[];
  limits?: NativeGraphLimits;
}
export interface NativeGraphEdgeProvenance {
  schemaRuleIndex: number;
  sourceRecordId: string;
  sourceField?: string[];
  derivedInverse: boolean;
  builtIn: boolean;
}
export interface NativeGraphPathEdge {
  relationship: string;
  source: NativeNodeId;
  target: NativeNodeId;
  occurrenceOrdinal: number;
  provenance: NativeGraphEdgeProvenance;
}
export interface NativeGraphMatch {
  node: NativeNodeId;
  depth: number;
  path: NativeGraphPathEdge[];
}
export interface NativeGraphResult {
  matches: NativeGraphMatch[];
  truncated?: "maxHops" | "maxVisited" | "maxResults" | "maxWorkingBytes";
  trace: {
    seedCount: number;
    visitedStates: number;
    traversedEdges: number;
    resultCount: number;
    diagnostics: number;
  };
}
export interface NativeCandidateProjection {
  candidates: { recordId: string; chunkKey: string }[];
  sourceNodes: number;
  projectedChunksBeforeFilter: number;
  projectedChunksAfterFilter: number;
}
export interface NativeSearchHit {
  documentId: string;
  text: string;
  metadata: NativeMetadataEntry[];
  score: number;
  vectorScore: number;
}
export interface NativeKeywordHit {
  documentId: string;
  text: string;
  metadata: NativeMetadataEntry[];
  score: number;
  matchedTerms: string[];
}
export interface NativeHybridHit {
  documentId: string;
  text: string;
  metadata: NativeMetadataEntry[];
  score: number;
  vectorScore?: number;
  keywordScore?: number;
  trace: {
    alpha: number;
    vectorRank?: number;
    keywordRank?: number;
    normalizedVectorScore?: number;
    normalizedKeywordScore?: number;
    matchedTerms: string[];
  };
}
export interface NativeGraphFileSizeReport {
  corpusBytes: number;
  schemaBytes: number;
  graphBytes: number;
  totalBytes: number;
}
