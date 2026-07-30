import { CACHE_SCHEMA } from "./constants.js";
import { EmbeddingCacheError } from "./errors.js";

export interface ArtifactStore {
  readonly identity: string;
  read(key: string): Promise<Uint8Array | undefined>;
  write(key: string, value: Uint8Array): Promise<void>;
  remove(key: string): Promise<void>;
  keys(prefix: string): Promise<readonly string[]>;
}

const keyOrigin = "https://retrievalkit.invalid/";

export class BrowserCacheArtifactStore implements ArtifactStore {
  public readonly identity: string;
  readonly #cacheName: string;
  readonly #cacheStorage: CacheStorage;

  public constructor(
    cacheName = CACHE_SCHEMA,
    cacheStorage?: CacheStorage
  ) {
    this.#cacheName = cacheName;
    const resolved = cacheStorage ?? globalThis.caches;
    if (resolved === undefined) {
      throw new EmbeddingCacheError(
        "CacheStorage is unavailable; supply an application ArtifactStore."
      );
    }
    this.#cacheStorage = resolved;
    this.identity = `cache-api:${cacheName}`;
  }

  public async read(key: string): Promise<Uint8Array | undefined> {
    try {
      const cache = await this.#cacheStorage.open(this.#cacheName);
      const response = await cache.match(this.#request(key));
      return response === undefined
        ? undefined
        : new Uint8Array(await response.arrayBuffer());
    } catch (error) {
      throw new EmbeddingCacheError(`Failed to read browser cache key '${key}'.`, error);
    }
  }

  public async write(key: string, value: Uint8Array): Promise<void> {
    try {
      const cache = await this.#cacheStorage.open(this.#cacheName);
      await cache.put(this.#request(key), new Response(value.slice()));
    } catch (error) {
      throw new EmbeddingCacheError(`Failed to write browser cache key '${key}'.`, error);
    }
  }

  public async remove(key: string): Promise<void> {
    try {
      const cache = await this.#cacheStorage.open(this.#cacheName);
      await cache.delete(this.#request(key));
    } catch (error) {
      throw new EmbeddingCacheError(`Failed to remove browser cache key '${key}'.`, error);
    }
  }

  public async keys(prefix: string): Promise<readonly string[]> {
    try {
      const cache = await this.#cacheStorage.open(this.#cacheName);
      const requests = await cache.keys();
      return requests
        .map((request) => decodeURIComponent(new URL(request.url).pathname.slice(1)))
        .filter((key) => key.startsWith(prefix));
    } catch (error) {
      throw new EmbeddingCacheError("Failed to inspect the browser embedding cache.", error);
    }
  }

  #request(key: string): Request {
    return new Request(`${keyOrigin}${encodeURIComponent(key)}`);
  }
}

export class MemoryArtifactStore implements ArtifactStore {
  public readonly identity: string;
  readonly #entries = new Map<string, Uint8Array>();

  public constructor(identity = `memory:${crypto.randomUUID()}`) {
    this.identity = identity;
  }

  public async read(key: string): Promise<Uint8Array | undefined> {
    return this.#entries.get(key)?.slice();
  }

  public async write(key: string, value: Uint8Array): Promise<void> {
    this.#entries.set(key, value.slice());
  }

  public async remove(key: string): Promise<void> {
    this.#entries.delete(key);
  }

  public async keys(prefix: string): Promise<readonly string[]> {
    return [...this.#entries.keys()].filter((key) => key.startsWith(prefix));
  }
}
