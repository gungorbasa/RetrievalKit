export type BrowserEmbeddingErrorCode =
  | "RK_EMBEDDING_INPUT"
  | "RK_EMBEDDING_UNAVAILABLE"
  | "RK_EMBEDDING_ARTIFACT"
  | "RK_EMBEDDING_CACHE"
  | "RK_EMBEDDING_RUNTIME"
  | "RK_EMBEDDING_OUTPUT"
  | "RK_EMBEDDING_CANCELLED"
  | "RK_EMBEDDING_CLOSED"
  | "RK_EMBEDDING_WORKER";

export class BrowserEmbeddingError extends Error {
  public readonly code: BrowserEmbeddingErrorCode;
  public override readonly cause?: unknown;

  public constructor(
    message: string,
    code: BrowserEmbeddingErrorCode,
    cause?: unknown
  ) {
    super(message);
    this.name = "BrowserEmbeddingError";
    this.code = code;
    this.cause = cause;
  }
}

export class EmbeddingInputError extends BrowserEmbeddingError {
  public constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_INPUT", cause);
    this.name = "EmbeddingInputError";
  }
}

export class EmbeddingUnavailableError extends BrowserEmbeddingError {
  public constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_UNAVAILABLE", cause);
    this.name = "EmbeddingUnavailableError";
  }
}

export class EmbeddingArtifactError extends BrowserEmbeddingError {
  public constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_ARTIFACT", cause);
    this.name = "EmbeddingArtifactError";
  }
}

export class EmbeddingCacheError extends BrowserEmbeddingError {
  public constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_CACHE", cause);
    this.name = "EmbeddingCacheError";
  }
}

export class EmbeddingRuntimeError extends BrowserEmbeddingError {
  public constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_RUNTIME", cause);
    this.name = "EmbeddingRuntimeError";
  }
}

export class EmbeddingOutputError extends BrowserEmbeddingError {
  public constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_OUTPUT", cause);
    this.name = "EmbeddingOutputError";
  }
}

export class EmbeddingCancelledError extends BrowserEmbeddingError {
  public constructor(message = "Embedding operation was cancelled.", cause?: unknown) {
    super(message, "RK_EMBEDDING_CANCELLED", cause);
    this.name = "EmbeddingCancelledError";
  }
}

export class EmbeddingClosedError extends BrowserEmbeddingError {
  public constructor(message = "The browser embedder is closed.", cause?: unknown) {
    super(message, "RK_EMBEDDING_CLOSED", cause);
    this.name = "EmbeddingClosedError";
  }
}

export class EmbeddingWorkerError extends BrowserEmbeddingError {
  public constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_WORKER", cause);
    this.name = "EmbeddingWorkerError";
  }
}

export interface SerializedEmbeddingError {
  readonly name: string;
  readonly message: string;
  readonly code: BrowserEmbeddingErrorCode;
}

const constructors: Readonly<Record<string, new (message: string) => BrowserEmbeddingError>> = {
  EmbeddingInputError,
  EmbeddingUnavailableError,
  EmbeddingArtifactError,
  EmbeddingCacheError,
  EmbeddingRuntimeError,
  EmbeddingOutputError,
  EmbeddingCancelledError,
  EmbeddingClosedError,
  EmbeddingWorkerError
};

export function serializeError(error: unknown): SerializedEmbeddingError {
  if (error instanceof BrowserEmbeddingError) {
    return { name: error.name, message: error.message, code: error.code };
  }
  return {
    name: "EmbeddingWorkerError",
    message: error instanceof Error ? error.message : String(error),
    code: "RK_EMBEDDING_WORKER"
  };
}

export function deserializeError(error: SerializedEmbeddingError): BrowserEmbeddingError {
  const Constructor = constructors[error.name] ?? EmbeddingWorkerError;
  return new Constructor(error.message);
}

export function cancelled(signal?: AbortSignal): void {
  if (signal?.aborted === true) {
    throw new EmbeddingCancelledError("Embedding operation was cancelled.", signal.reason);
  }
}
