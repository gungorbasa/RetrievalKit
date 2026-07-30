import {
  ARTIFACT_MANIFEST_SHA256,
  ARTIFACT_REVISION,
  CACHE_SCHEMA,
  type ArtifactSpec
} from "./constants.js";
import {
  cancelled,
  EmbeddingArtifactError,
  EmbeddingUnavailableError
} from "./errors.js";
import type { ArtifactStore } from "./store.js";

export interface ArtifactFetcher {
  (url: string, signal?: AbortSignal): Promise<Uint8Array>;
}

export interface AcquiredArtifacts {
  read(path: string): Promise<Uint8Array>;
}

export interface AcquireOptions {
  readonly artifacts: readonly ArtifactSpec[];
  readonly store: ArtifactStore;
  readonly fetcher: ArtifactFetcher;
  readonly localOnly: boolean;
  readonly signal?: AbortSignal;
}

const root = `${CACHE_SCHEMA}/${ARTIFACT_REVISION}`;
const completeKey = `${root}/complete`;
const completeValue = new TextEncoder().encode(`${ARTIFACT_MANIFEST_SHA256}\n`);
const acquisitions = new Map<string, Promise<AcquiredArtifacts>>();

export async function acquireArtifacts(options: AcquireOptions): Promise<AcquiredArtifacts> {
  const key = `${options.store.identity}:${root}`;
  const current = acquisitions.get(key);
  if (options.localOnly) {
    const localOptions: AcquireOptions = {
      artifacts: options.artifacts,
      store: options.store,
      fetcher: options.fetcher,
      localOnly: true
    };
    const localAcquisition =
      current === undefined
        ? withBrowserLock(key, async () => await acquireOnce(localOptions))
        : current.then(
            async () =>
              await withBrowserLock(key, async () =>
                await acquireOnce(localOptions)
              ),
            async () =>
              await withBrowserLock(key, async () =>
                await acquireOnce(localOptions)
              )
          );
    return await abortable(localAcquisition, options.signal);
  }
  if (current !== undefined) return await abortable(current, options.signal);

  const sharedOptions: AcquireOptions = {
    artifacts: options.artifacts,
    store: options.store,
    fetcher: options.fetcher,
    localOnly: options.localOnly
  };
  const acquisition = withBrowserLock(key, async () => await acquireOnce(sharedOptions)).finally(() => {
    if (acquisitions.get(key) === acquisition) acquisitions.delete(key);
  });
  acquisitions.set(key, acquisition);
  return await abortable(acquisition, options.signal);
}

async function acquireOnce(options: AcquireOptions): Promise<AcquiredArtifacts> {
  cancelled(options.signal);
  const cached = await verifiedCache(options.store, options.artifacts, options.signal);
  if (cached !== undefined) return memoryReader(cached);

  await clean(options.store, root);
  if (options.localOnly) {
    throw new EmbeddingUnavailableError(
      "The verified FP32 MiniLM artifacts are not cached and localOnly forbids network access."
    );
  }

  try {
    const downloaded = new Map<string, Uint8Array>();
    for (const artifact of options.artifacts) {
      cancelled(options.signal);
      assertHttps(artifact.url);
      const bytes = await options.fetcher(artifact.url, options.signal);
      await verify(artifact, bytes);
      downloaded.set(artifact.path, bytes);
    }

    for (const artifact of options.artifacts) {
      cancelled(options.signal);
      const bytes = downloaded.get(artifact.path);
      if (bytes === undefined) {
        throw new EmbeddingArtifactError(
          `Downloaded artifact '${artifact.path}' disappeared before publication.`
        );
      }
      await options.store.write(`${root}/files/${artifact.path}`, bytes);
    }
    await options.store.write(completeKey, completeValue);
    return memoryReader(downloaded);
  } catch (error) {
    await clean(options.store, root).catch(() => undefined);
    if (error instanceof Error) throw error;
    throw new EmbeddingArtifactError(String(error));
  }
}

async function verifiedCache(
  store: ArtifactStore,
  artifacts: readonly ArtifactSpec[],
  signal?: AbortSignal
): Promise<ReadonlyMap<string, Uint8Array> | undefined> {
  const marker = await store.read(completeKey);
  if (marker === undefined || !equal(marker, completeValue)) return undefined;
  const verified = new Map<string, Uint8Array>();
  try {
    for (const artifact of artifacts) {
      cancelled(signal);
      const bytes = await store.read(`${root}/files/${artifact.path}`);
      if (bytes === undefined) return undefined;
      await verify(artifact, bytes);
      verified.set(artifact.path, bytes);
    }
    return verified;
  } catch (error) {
    if (error instanceof EmbeddingArtifactError) return undefined;
    throw error;
  }
}

function memoryReader(files: ReadonlyMap<string, Uint8Array>): AcquiredArtifacts {
  return {
    async read(path: string): Promise<Uint8Array> {
      const bytes = files.get(path);
      if (bytes === undefined) {
        throw new EmbeddingArtifactError(`Verified artifact '${path}' is missing.`);
      }
      return bytes;
    }
  };
}

async function verify(artifact: ArtifactSpec, bytes: Uint8Array): Promise<void> {
  if (bytes.byteLength !== artifact.bytes) {
    throw new EmbeddingArtifactError(
      `Artifact '${artifact.path}' has ${bytes.byteLength} bytes; expected ${artifact.bytes}.`
    );
  }
  const digest = hex(await crypto.subtle.digest("SHA-256", bytes.slice().buffer));
  if (digest !== artifact.sha256) {
    throw new EmbeddingArtifactError(
      `Artifact '${artifact.path}' failed SHA-256 verification.`
    );
  }
}

function assertHttps(url: string): void {
  const parsed = new URL(url);
  if (parsed.protocol !== "https:") {
    throw new EmbeddingArtifactError(`Refusing insecure artifact URL '${url}'.`);
  }
}

async function clean(store: ArtifactStore, prefix: string): Promise<void> {
  const keys = await store.keys(prefix);
  await Promise.all(keys.map(async (key) => await store.remove(key)));
}

function equal(left: Uint8Array, right: Uint8Array): boolean {
  return (
    left.byteLength === right.byteLength &&
    left.every((value, index) => value === right[index])
  );
}

function hex(buffer: ArrayBuffer): string {
  return [...new Uint8Array(buffer)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function abortable<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
  cancelled(signal);
  if (signal === undefined) return await promise;
  return await new Promise<T>((resolve, reject) => {
    const abort = (): void => {
      reject(new EmbeddingUnavailableError("Artifact acquisition wait was cancelled."));
    };
    signal.addEventListener("abort", abort, { once: true });
    void promise.then(resolve, reject).finally(() => {
      signal.removeEventListener("abort", abort);
    });
  });
}

export const defaultArtifactFetcher: ArtifactFetcher = async (url, signal) => {
  assertHttps(url);
  const response = await fetch(url, { ...(signal === undefined ? {} : { signal }) });
  if (!response.ok) {
    throw new EmbeddingUnavailableError(
      `Artifact request failed with HTTP ${response.status} for '${url}'.`
    );
  }
  return new Uint8Array(await response.arrayBuffer());
};

async function withBrowserLock<T>(key: string, action: () => Promise<T>): Promise<T> {
  const locks =
    typeof navigator === "undefined"
      ? undefined
      : (navigator as Navigator & { locks?: LockManager }).locks;
  if (locks === undefined) return await action();
  return await locks.request(`retrievalkit:${key}`, async () => await action());
}
