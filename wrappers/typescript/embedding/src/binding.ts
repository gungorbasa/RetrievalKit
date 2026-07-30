import { createRequire } from "node:module";

import type {
  NativeLoadOptions,
  NativeModelInfo,
  NativePrefetchOptions
} from "./native-types.js";

export interface NativeOnnxEmbedder {
  readonly closed: boolean;
  initialize(options: NativeLoadOptions): Promise<void>;
  embed(text: string): Promise<Float32Array>;
  embedBatch(texts: string[]): Promise<Float32Array[]>;
  modelInfo(): NativeModelInfo;
  close(): Promise<void>;
}

interface NativeOnnxEmbedderConstructor {
  new (): NativeOnnxEmbedder;
}

interface NativeBinding {
  NativeOnnxEmbedder: NativeOnnxEmbedderConstructor;
  prefetchModel(options: NativePrefetchOptions): Promise<void>;
  _verifyPackageRuntime(path: string): Promise<void>;
}

const require = createRequire(import.meta.url);
export const binding = require(
  "../native/retrievalkit-embedding.node"
) as NativeBinding;
