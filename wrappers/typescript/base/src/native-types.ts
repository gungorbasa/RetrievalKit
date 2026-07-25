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

export interface NativeFilter {
  kind: string;
  field?: string;
  value?: NativeMetadataValue;
  values?: NativeMetadataValue[];
  lower?: NativeMetadataValue;
  upper?: NativeMetadataValue;
  children?: NativeFilter[];
}

export interface NativeDocumentInput {
  id: string;
  text: string;
  metadata: NativeMetadataEntry[];
  embedding: Float32Array;
}

export interface NativeSearchHit {
  documentId: string;
  text: string;
  metadata: NativeMetadataEntry[];
  score: number;
  vectorScore: number;
}

export interface NativeHybridTrace {
  alpha: number;
  vectorRank?: number;
  keywordRank?: number;
  normalizedVectorScore?: number;
  normalizedKeywordScore?: number;
  matchedTerms: string[];
}

export interface NativeHybridHit {
  documentId: string;
  text: string;
  metadata: NativeMetadataEntry[];
  score: number;
  vectorScore?: number;
  keywordScore?: number;
  trace: NativeHybridTrace;
}

export interface NativeFileSizeReport {
  manifestBytes: number;
  vectorsBytes: number;
  chunksBytes: number;
  recordsBytes: number;
  bm25Bytes: number;
  tombstonesBytes: number;
  totalBytes: number;
}
