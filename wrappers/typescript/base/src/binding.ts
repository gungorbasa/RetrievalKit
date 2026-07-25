import { createRequire } from "node:module";

import type {
  NativeDocumentInput,
  NativeFileSizeReport,
  NativeFilter,
  NativeHybridHit,
  NativeSearchHit
} from "./native-types.js";

export interface NativeRetrievalHandle {
  readonly closed: boolean;
  addDocuments(documents: NativeDocumentInput[]): Promise<number[][]>;
  build(): Promise<void>;
  load(path: string): Promise<void>;
  semanticSearch(
    embedding: Float32Array,
    topK: number,
    filter?: NativeFilter
  ): Promise<NativeSearchHit[]>;
  hybridSearch(
    text: string,
    embedding: Float32Array | undefined,
    topK: number,
    filter: NativeFilter | undefined,
    alpha: number,
    vectorCandidates?: number,
    keywordCandidates?: number
  ): Promise<NativeHybridHit[]>;
  save(path: string): Promise<NativeFileSizeReport>;
  close(): Promise<void>;
}

interface NativeRetrievalConstructor {
  new (corpusId: string, metric: string, encoding: string): NativeRetrievalHandle;
  empty(): NativeRetrievalHandle;
}

interface NativeBinding {
  NativeRetrievalHandle: NativeRetrievalConstructor;
  validateRetrieval(path: string): Promise<void>;
}

const require = createRequire(import.meta.url);
const aggregateKey = Symbol.for("retrievalkit.node.nativeAggregate");
const processState = globalThis as unknown as Record<
  symbol,
  "base" | "graph" | undefined
>;
if (processState[aggregateKey] === "graph") {
  throw new Error(
    "RK_LIFECYCLE: cannot load the RetrievalKit base native aggregate after the graph aggregate in one process; import exactly one repository-local package"
  );
}
processState[aggregateKey] = "base";
let exportBinding: NativeBinding;
try {
  exportBinding = require("../native/retrievalkit.node") as NativeBinding;
} catch (error) {
  delete processState[aggregateKey];
  throw error;
}
export const binding = exportBinding;
