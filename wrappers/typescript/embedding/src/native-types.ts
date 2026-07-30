export interface NativeLoadOptions {
  cacheDirectory?: string;
  localOnly?: boolean;
  runtimeLibraryPath?: string;
  verifyPackageRuntime?: boolean;
}

export interface NativePrefetchOptions {
  cacheDirectory?: string;
  localOnly?: boolean;
}

export interface NativeModelInfo {
  readonly identifier: string;
  readonly dimension: number;
  readonly maxInputTokens: number;
  readonly normalized: boolean;
  readonly precision: string;
  readonly sourceRevision: string;
  readonly runtime: string;
  readonly runtimeVersion: string;
}
