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
export class RetrievalKitGraphError extends RetrievalKitError {}
export class RetrievalKitStaleSelectionError extends RetrievalKitGraphError {}
export class RetrievalKitCancelledError extends RetrievalKitError {}
export class RetrievalKitWorkerError extends RetrievalKitError {}

export interface SerializedError {
  readonly name: string;
  readonly message: string;
  readonly code: string;
}

const errorsByName: Readonly<Record<string, typeof RetrievalKitError>> = {
  RetrievalKitInputError,
  RetrievalKitDimensionError,
  RetrievalKitLifecycleError,
  RetrievalKitPersistenceError,
  RetrievalKitQueryError,
  RetrievalKitGraphError,
  RetrievalKitStaleSelectionError,
  RetrievalKitCancelledError,
  RetrievalKitWorkerError
};

export function serializeError(error: unknown): SerializedError {
  if (error instanceof RetrievalKitError) {
    return { name: error.name, message: error.message, code: error.code };
  }
  if (error instanceof Error) {
    return { name: error.name, message: error.message, code: "RK_WORKER" };
  }
  return { name: "RetrievalKitWorkerError", message: String(error), code: "RK_WORKER" };
}

export function deserializeError(error: SerializedError): RetrievalKitError {
  const Constructor = errorsByName[error.name] ?? RetrievalKitWorkerError;
  return new Constructor(error.message, error.code);
}
