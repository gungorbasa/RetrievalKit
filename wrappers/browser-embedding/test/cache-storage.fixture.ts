/**
 * A browser-API-faithful, in-memory CacheStorage fixture.
 *
 * It deliberately stores Request/Response objects rather than artifact bytes,
 * so tests exercise BrowserCacheArtifactStore's key encoding, response
 * cloning, and Cache API error mapping. It contains no Node-only APIs and can
 * run unchanged in a browser test runner.
 */
export class ControlledCacheStorage implements CacheStorage {
  readonly #caches = new Map<string, ControlledCache>();
  public readonly events: string[] = [];
  public failOpen: Error | undefined;
  public failMatch: Error | undefined;
  public failPut: Error | undefined;
  public failDelete: Error | undefined;
  public failKeys: Error | undefined;
  public failPutAt: number | undefined;
  public putCount = 0;

  public async delete(cacheName: string): Promise<boolean> {
    return this.#caches.delete(cacheName);
  }

  public async has(cacheName: string): Promise<boolean> {
    return this.#caches.has(cacheName);
  }

  public async keys(): Promise<string[]> {
    return [...this.#caches.keys()];
  }

  public async match(
    request: RequestInfo | URL,
    options?: MultiCacheQueryOptions
  ): Promise<Response | undefined> {
    const names =
      options?.cacheName === undefined
        ? [...this.#caches.keys()]
        : [options.cacheName];
    for (const name of names) {
      const response = await this.#caches.get(name)?.match(request, options);
      if (response !== undefined) return response;
    }
    return undefined;
  }

  public async open(cacheName: string): Promise<Cache> {
    if (this.failOpen !== undefined) throw this.failOpen;
    let cache = this.#caches.get(cacheName);
    if (cache === undefined) {
      cache = new ControlledCache(this);
      this.#caches.set(cacheName, cache);
    }
    return cache;
  }
}

class ControlledCache implements Cache {
  readonly #owner: ControlledCacheStorage;
  readonly #entries = new Map<string, { request: Request; response: Response }>();

  public constructor(owner: ControlledCacheStorage) {
    this.#owner = owner;
  }

  public async add(request: RequestInfo | URL): Promise<void> {
    const response = await fetch(request);
    if (!response.ok) throw new TypeError(`Request failed with HTTP ${response.status}.`);
    await this.put(request, response);
  }

  public async addAll(requests: RequestInfo[]): Promise<void> {
    await Promise.all(requests.map(async (request) => await this.add(request)));
  }

  public async delete(
    request: RequestInfo | URL,
    _options?: CacheQueryOptions
  ): Promise<boolean> {
    if (this.#owner.failDelete !== undefined) throw this.#owner.failDelete;
    const normalized = normalize(request);
    this.#owner.events.push(`delete:${normalized.url}`);
    return this.#entries.delete(normalized.url);
  }

  public async keys(
    request?: RequestInfo | URL,
    _options?: CacheQueryOptions
  ): Promise<readonly Request[]> {
    if (this.#owner.failKeys !== undefined) throw this.#owner.failKeys;
    const requests = [...this.#entries.values()].map(({ request: key }) => key.clone());
    if (request === undefined) return requests;
    const url = normalize(request).url;
    return requests.filter((candidate) => candidate.url === url);
  }

  public async match(
    request: RequestInfo | URL,
    _options?: CacheQueryOptions
  ): Promise<Response | undefined> {
    if (this.#owner.failMatch !== undefined) throw this.#owner.failMatch;
    const normalized = normalize(request);
    this.#owner.events.push(`match:${normalized.url}`);
    return this.#entries.get(normalized.url)?.response.clone();
  }

  public async matchAll(
    request?: RequestInfo | URL,
    _options?: CacheQueryOptions
  ): Promise<readonly Response[]> {
    if (request !== undefined) {
      const response = await this.match(request);
      return response === undefined ? [] : [response];
    }
    return [...this.#entries.values()].map(({ response }) => response.clone());
  }

  public async put(request: RequestInfo | URL, response: Response): Promise<void> {
    this.#owner.putCount += 1;
    if (
      this.#owner.failPut !== undefined &&
      (this.#owner.failPutAt === undefined ||
        this.#owner.putCount === this.#owner.failPutAt)
    ) {
      throw this.#owner.failPut;
    }
    const normalized = normalize(request);
    this.#owner.events.push(`put:${normalized.url}`);
    this.#entries.set(normalized.url, {
      request: normalized,
      response: response.clone()
    });
  }
}

function normalize(request: RequestInfo | URL): Request {
  return request instanceof Request ? request : new Request(request);
}
