import { createRequire } from "node:module";
import type {
  NativeCandidateProjection,
  NativeFilter,
  NativeGraphFileSizeReport,
  NativeGraphQuery,
  NativeGraphRecordInput,
  NativeGraphResult,
  NativeGraphSchema,
  NativeHybridHit,
  NativeSearchHit
} from "./native-types.js";

export interface NativeGraphSelection {
  readonly closed: boolean;
  close(): Promise<void>;
}
interface NativeSelectionConstructor {
  new (): NativeGraphSelection;
}
export interface NativeGraphHandle {
  readonly closed: boolean;
  addRecords(records: NativeGraphRecordInput[]): Promise<void>;
  build(): Promise<void>;
  load(kind: string, path: string): Promise<void>;
  query(query: NativeGraphQuery, selection: NativeGraphSelection): Promise<NativeGraphResult>;
  projectCandidates(
    selection: NativeGraphSelection,
    filter?: NativeFilter
  ): Promise<NativeCandidateProjection>;
  semanticSearch(
    embedding: Float32Array,
    topK: number,
    filter: NativeFilter | undefined,
    selection?: NativeGraphSelection
  ): Promise<NativeSearchHit[]>;
  hybridSearch(
    text: string,
    embedding: Float32Array | undefined,
    topK: number,
    filter: NativeFilter | undefined,
    alpha: number,
    vectorCandidates: number | undefined,
    keywordCandidates: number | undefined,
    selection?: NativeGraphSelection
  ): Promise<NativeHybridHit[]>;
  save(path: string): Promise<NativeGraphFileSizeReport>;
  close(): Promise<void>;
}
interface NativeGraphConstructor {
  new (
    kind: string,
    corpusId: string,
    schema: NativeGraphSchema,
    metric?: string,
    encoding?: string
  ): NativeGraphHandle;
  empty(): NativeGraphHandle;
}
interface NativeBinding {
  NativeGraphHandle: NativeGraphConstructor;
  NativeGraphSelection: NativeSelectionConstructor;
  validateGraph(kind: string, path: string): Promise<void>;
}
const require = createRequire(import.meta.url);
const aggregateKey = Symbol.for("retrievalkit.node.nativeAggregate");
const processState = globalThis as unknown as Record<
  symbol,
  "base" | "graph" | undefined
>;
if (processState[aggregateKey] === "base") {
  throw new Error(
    "RK_LIFECYCLE: cannot load the RetrievalKit graph native aggregate after the base aggregate in one process; import exactly one repository-local package"
  );
}
processState[aggregateKey] = "graph";
let exportBinding: NativeBinding;
try {
  exportBinding = require("../native/retrievalkit.node") as NativeBinding;
} catch (error) {
  delete processState[aggregateKey];
  throw error;
}
export const binding = exportBinding;
