import { acquireArtifacts } from "../src/acquire.js";
import type { ArtifactSpec } from "../src/constants.js";
import {
  EmbeddingCacheError,
  EmbeddingUnavailableError
} from "../src/errors.js";
import { BrowserCacheArtifactStore } from "../src/store.js";
import { ControlledCacheStorage } from "./cache-storage.fixture.js";
import { describe, expect, it } from "vitest";

const cachePrefix = "retrievalkit-browser-embedding-v1/";

describe("real CacheStorage artifact adapter", () => {
  it("cold-publishes into CacheStorage, writes completion last, and reloads local-only", async () => {
    const fixture = await artifacts();
    const cacheStorage = new ControlledCacheStorage();
    const store = new BrowserCacheArtifactStore("cold-cache", cacheStorage);
    let fetches = 0;
    const acquired = await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async (url) => {
        fetches += 1;
        return requiredFile(fixture.byUrl, url);
      },
      localOnly: false
    });

    expect(await acquired.read("tokenizer/vocab.txt")).toEqual(
      requiredFile(fixture.byPath, "tokenizer/vocab.txt")
    );
    expect(fetches).toBe(fixture.specs.length);
    const puts = cacheStorage.events.filter((event) => event.startsWith("put:"));
    expect(puts).toHaveLength(fixture.specs.length + 1);
    expect(decodeKey(puts.at(-1))).toMatch(/\/complete$/);
    const published = puts.slice(0, -1).map((event) => decodeKey(event));
    expect(published).toHaveLength(fixture.specs.length);
    for (const spec of fixture.specs) {
      expect(published.some((key) => key.endsWith(`/files/${spec.path}`))).toBe(true);
    }

    const reloaded = await acquireArtifacts({
      artifacts: fixture.specs,
      store: new BrowserCacheArtifactStore("cold-cache", cacheStorage),
      fetcher: async () => {
        throw new Error("localOnly must not use the network");
      },
      localOnly: true
    });
    expect(await reloaded.read("manifest-v1.json")).toEqual(
      requiredFile(fixture.byPath, "manifest-v1.json")
    );
    expect(fetches).toBe(fixture.specs.length);
  });

  it("detects an evicted file despite a completion marker and removes the entire generation", async () => {
    const fixture = await artifacts();
    const cacheStorage = new ControlledCacheStorage();
    const store = new BrowserCacheArtifactStore("evicted-cache", cacheStorage);
    await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async (url) => requiredFile(fixture.byUrl, url),
      localOnly: false
    });
    const modelKey = (await store.keys(cachePrefix)).find((key) =>
      key.endsWith("/onnx/model.onnx")
    );
    expect(modelKey).toBeDefined();
    if (modelKey === undefined) throw new Error("model cache key is absent");
    await store.remove(modelKey);

    await expect(
      acquireArtifacts({
        artifacts: fixture.specs,
        store,
        fetcher: async () => {
          throw new Error("localOnly must not use the network");
        },
        localOnly: true
      })
    ).rejects.toBeInstanceOf(EmbeddingUnavailableError);
    expect(await store.keys(cachePrefix)).toEqual([]);
  });

  it("detects corrupted CacheStorage responses and repairs them with verified downloads", async () => {
    const fixture = await artifacts();
    const cacheStorage = new ControlledCacheStorage();
    const store = new BrowserCacheArtifactStore("corrupt-cache", cacheStorage);
    await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async (url) => requiredFile(fixture.byUrl, url),
      localOnly: false
    });
    const tokenizerKey = (await store.keys(cachePrefix)).find((key) =>
      key.endsWith("/tokenizer/tokenizer.json")
    );
    expect(tokenizerKey).toBeDefined();
    if (tokenizerKey === undefined) throw new Error("tokenizer cache key is absent");
    await store.write(tokenizerKey, new Uint8Array([1, 2, 3]));

    let repairs = 0;
    await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async (url) => {
        repairs += 1;
        return requiredFile(fixture.byUrl, url);
      },
      localOnly: false
    });
    expect(repairs).toBe(fixture.specs.length);

    await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async () => {
        throw new Error("repaired cache must be complete");
      },
      localOnly: true
    });
  });

  it("deduplicates concurrent callers sharing a CacheStorage generation", async () => {
    const fixture = await artifacts();
    const cacheStorage = new ControlledCacheStorage();
    const firstStore = new BrowserCacheArtifactStore("shared-cache", cacheStorage);
    const secondStore = new BrowserCacheArtifactStore("shared-cache", cacheStorage);
    let fetches = 0;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const fetcher = async (url: string): Promise<Uint8Array> => {
      fetches += 1;
      await gate;
      return requiredFile(fixture.byUrl, url);
    };

    const first = acquireArtifacts({
      artifacts: fixture.specs,
      store: firstStore,
      fetcher,
      localOnly: false
    });
    const second = acquireArtifacts({
      artifacts: fixture.specs,
      store: secondStore,
      fetcher,
      localOnly: false
    });
    release();
    await Promise.all([first, second]);

    expect(fetches).toBe(fixture.specs.length);
    expect(await firstStore.keys(cachePrefix)).toHaveLength(fixture.specs.length + 1);
  });

  it("cleans partial CacheStorage publication after quota failure and never leaves completion", async () => {
    const fixture = await artifacts();
    const cacheStorage = new ControlledCacheStorage();
    cacheStorage.failPut = new DOMException("quota exhausted", "QuotaExceededError");
    cacheStorage.failPutAt = 3;
    const store = new BrowserCacheArtifactStore("quota-cache", cacheStorage);

    await expect(
      acquireArtifacts({
        artifacts: fixture.specs,
        store,
        fetcher: async (url) => requiredFile(fixture.byUrl, url),
        localOnly: false
      })
    ).rejects.toSatisfy(
      (error: unknown) =>
        error instanceof EmbeddingCacheError &&
        error.cause instanceof DOMException &&
        error.cause.name === "QuotaExceededError"
    );
    expect(await store.keys(cachePrefix)).toEqual([]);

    cacheStorage.failPut = undefined;
    cacheStorage.failPutAt = undefined;
    await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async (url) => requiredFile(fixture.byUrl, url),
      localOnly: false
    });
    expect(
      (await store.keys(cachePrefix)).some((key) => key.endsWith("/complete"))
    ).toBe(true);
  });

  it("maps CacheStorage open, read, inspection, and deletion failures with their causes", async () => {
    const failureCases = [
      ["open", "failOpen", async (store: BrowserCacheArtifactStore) => await store.read("key")],
      ["read", "failMatch", async (store: BrowserCacheArtifactStore) => await store.read("key")],
      ["inspect", "failKeys", async (store: BrowserCacheArtifactStore) => await store.keys("")],
      ["remove", "failDelete", async (store: BrowserCacheArtifactStore) => await store.remove("key")]
    ] as const;

    for (const [label, property, operation] of failureCases) {
      const cacheStorage = new ControlledCacheStorage();
      const cause = new DOMException(`${label} failed`, "UnknownError");
      cacheStorage[property] = cause;
      const store = new BrowserCacheArtifactStore(`errors-${label}`, cacheStorage);
      await expect(operation(store)).rejects.toSatisfy(
        (error: unknown) =>
          error instanceof EmbeddingCacheError &&
          error.cause === cause
      );
    }
  });

  it("round-trips encoded artifact keys through Cache.keys", async () => {
    const cacheStorage = new ControlledCacheStorage();
    const store = new BrowserCacheArtifactStore("encoded-keys", cacheStorage);
    const key = `${cachePrefix}unicode/世界 and space/%value`;
    await store.write(key, new Uint8Array([7, 8, 9]));
    expect(await store.keys(cachePrefix)).toEqual([key]);
    expect(await store.read(key)).toEqual(new Uint8Array([7, 8, 9]));
  });
});

async function artifacts(): Promise<{
  specs: readonly ArtifactSpec[];
  byUrl: ReadonlyMap<string, Uint8Array>;
  byPath: ReadonlyMap<string, Uint8Array>;
}> {
  const paths = [
    "manifest-v1.json",
    "onnx/model.onnx",
    "tokenizer/tokenizer.json",
    "tokenizer/tokenizer_config.json",
    "tokenizer/special_tokens_map.json",
    "tokenizer/vocab.txt"
  ];
  const files = paths.map((path, index) =>
    new TextEncoder().encode(`cache-storage:${path}:${index}`)
  );
  const specs = await Promise.all(
    paths.map(async (path, index) => {
      const bytes = files[index];
      if (bytes === undefined) throw new Error("artifact fixture is incomplete");
      return {
        path,
        bytes: bytes.byteLength,
        sha256: await sha256(bytes),
        url: `https://cache.example/${path}`
      };
    })
  );
  return {
    specs,
    byUrl: new Map(
      specs.map((spec, index) => [spec.url, requiredIndex(files, index)])
    ),
    byPath: new Map(
      specs.map((spec, index) => [spec.path, requiredIndex(files, index)])
    )
  };
}

function requiredIndex(files: readonly Uint8Array[], index: number): Uint8Array {
  const file = files[index];
  if (file === undefined) throw new Error(`fixture file ${index} is absent`);
  return file;
}

function requiredFile(
  files: ReadonlyMap<string, Uint8Array>,
  key: string
): Uint8Array {
  const file = files.get(key);
  if (file === undefined) throw new Error(`fixture '${key}' is absent`);
  return file.slice();
}

function decodeKey(event: string | undefined): string {
  if (event === undefined) throw new Error("cache event is absent");
  const url = event.slice(event.indexOf(":") + 1);
  return decodeURIComponent(new URL(url).pathname.slice(1));
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes.slice().buffer);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}
