import { acquireArtifacts } from "../src/acquire.js";
import { BrowserEmbedder } from "../src/client.js";
import type { ArtifactSpec } from "../src/constants.js";
import {
  EmbeddingArtifactError,
  EmbeddingCancelledError,
  EmbeddingClosedError,
  EmbeddingOutputError,
  EmbeddingRuntimeError,
  EmbeddingUnavailableError
} from "../src/errors.js";
import type {
  WorkerIncomingMessage,
  WorkerResponse,
  WorkerScopeLike
} from "../src/protocol.js";
import type {
  EmbeddingRuntime,
  EmbeddingRuntimeFactory
} from "../src/runtime.js";
import { validateEmbedding } from "../src/runtime.js";
import { MemoryArtifactStore, type ArtifactStore } from "../src/store.js";
import { PinnedMiniLmTokenizer } from "../src/tokenizer.js";
import type { BrowserEmbeddingWorkerLike } from "../src/types.js";
import { installBrowserEmbeddingWorker } from "../src/worker.js";
import { describe, expect, it } from "vitest";

describe("verified browser artifact acquisition", () => {
  it("publishes six verified files, reloads locally, and never fetches during reads", async () => {
    const fixture = await artifactFixture();
    const store = new MemoryArtifactStore("verified");
    let fetches = 0;
    const fetcher = async (url: string): Promise<Uint8Array> => {
      fetches += 1;
      return fixture.byUrl.get(url)?.slice() ?? new Uint8Array();
    };
    const first = await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher,
      localOnly: false
    });
    expect(await first.read("manifest-v1.json")).toEqual(fixture.files[0]);
    expect(fetches).toBe(6);

    const second = await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async () => {
        throw new Error("network must not run");
      },
      localOnly: true
    });
    expect(await second.read("tokenizer/vocab.txt")).toEqual(fixture.files[5]);
    expect(fetches).toBe(6);
  });

  it("removes corrupt and partial cache state and fails closed in local-only mode", async () => {
    const fixture = await artifactFixture();
    const store = new MemoryArtifactStore("corrupt");
    await acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async (url) => fixture.byUrl.get(url) ?? new Uint8Array(),
      localOnly: false
    });
    const modelKey = (await store.keys("retrievalkit-browser-embedding-v1/"))
      .find((key) => key.endsWith("model.onnx"));
    expect(modelKey).toBeDefined();
    if (modelKey === undefined) throw new Error("model fixture key is missing");
    await store.write(modelKey, new Uint8Array([0]));

    await expect(
      acquireArtifacts({
        artifacts: fixture.specs,
        store,
        fetcher: async () => {
          throw new Error("network must not run");
        },
        localOnly: true
      })
    ).rejects.toBeInstanceOf(EmbeddingUnavailableError);
    expect(await store.keys("retrievalkit-browser-embedding-v1/")).toEqual([]);
  });

  it("rejects wrong sizes, digests, and non-HTTPS locations without publication", async () => {
    const fixture = await artifactFixture();
    const store = new MemoryArtifactStore("invalid");
    const wrong = fixture.specs.map((spec, index) =>
      index === 0 ? { ...spec, sha256: "0".repeat(64) } : spec
    );
    await expect(
      acquireArtifacts({
        artifacts: wrong,
        store,
        fetcher: async (url) => fixture.byUrl.get(url) ?? new Uint8Array(),
        localOnly: false
      })
    ).rejects.toBeInstanceOf(EmbeddingArtifactError);
    expect(await store.keys("retrievalkit-browser-embedding-v1/")).toEqual([]);

    const firstSpec = fixture.specs[0];
    const firstFile = fixture.files[0];
    if (firstSpec === undefined || firstFile === undefined) {
      throw new Error("artifact fixture is empty");
    }
    await expect(
      acquireArtifacts({
        artifacts: [{ ...firstSpec, url: "http://example.test/model" }],
        store,
        fetcher: async () => firstFile,
        localOnly: false
      })
    ).rejects.toBeInstanceOf(EmbeddingArtifactError);
  });

  it("shares concurrent acquisition and aborting one waiter does not cancel it", async () => {
    const fixture = await artifactFixture();
    const store = new MemoryArtifactStore("single-flight");
    let fetches = 0;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const fetcher = async (url: string): Promise<Uint8Array> => {
      fetches += 1;
      await gate;
      return fixture.byUrl.get(url) ?? new Uint8Array();
    };
    const controller = new AbortController();
    const first = acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher,
      localOnly: false,
      signal: controller.signal
    });
    const second = acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher,
      localOnly: false
    });
    controller.abort();
    release();
    await expect(first).rejects.toBeInstanceOf(EmbeddingUnavailableError);
    await second;
    expect(fetches).toBe(6);
  });

  it("does not let a local-only miss poison a concurrent network acquisition", async () => {
    const fixture = await artifactFixture();
    const store = new MemoryArtifactStore("local-only-vs-network");
    let fetches = 0;
    const local = acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async () => {
        throw new Error("local-only must not fetch");
      },
      localOnly: true
    });
    const network = acquireArtifacts({
      artifacts: fixture.specs,
      store,
      fetcher: async (url) => {
        fetches += 1;
        return fixture.byUrl.get(url) ?? new Uint8Array();
      },
      localOnly: false
    });
    await expect(local).rejects.toBeInstanceOf(EmbeddingUnavailableError);
    await network;
    expect(fetches).toBe(6);
  });

  it("cleans interrupted downloads and failed publication, and writes the marker last", async () => {
    const fixture = await artifactFixture();
    const interrupted = new MemoryArtifactStore("interrupted");
    let fetches = 0;
    await expect(
      acquireArtifacts({
        artifacts: fixture.specs,
        store: interrupted,
        fetcher: async (url) => {
          fetches += 1;
          if (fetches === 3) throw new Error("download interrupted");
          return fixture.byUrl.get(url) ?? new Uint8Array();
        },
        localOnly: false
      })
    ).rejects.toThrow("download interrupted");
    expect(await interrupted.keys("retrievalkit-browser-embedding-v1/")).toEqual([]);

    const writes: string[] = [];
    const publishedInner = new MemoryArtifactStore("published-inner");
    const published = recordingStore("published", publishedInner, writes);
    await acquireArtifacts({
      artifacts: fixture.specs,
      store: published,
      fetcher: async (url) => fixture.byUrl.get(url) ?? new Uint8Array(),
      localOnly: false
    });
    expect(writes).toHaveLength(7);
    expect(writes.at(-1)).toMatch(/\/complete$/);

    const failedInner = new MemoryArtifactStore("failed-inner");
    const failed = recordingStore("failed", failedInner, [], 3);
    await expect(
      acquireArtifacts({
        artifacts: fixture.specs,
        store: failed,
        fetcher: async (url) => fixture.byUrl.get(url) ?? new Uint8Array(),
        localOnly: false
      })
    ).rejects.toThrow("publication interrupted");
    expect(await failedInner.keys("retrievalkit-browser-embedding-v1/")).toEqual([]);
  });
});

describe("token and Worker contract", () => {
  it("ignores the stale tokenizer truncation, preserves SEP at 256, and pads to batch longest", () => {
    const tokenizer = new PinnedMiniLmTokenizer(
      jsonBytes(tokenizerFixture()),
      jsonBytes({
        do_lower_case: true,
        unk_token: "[UNK]",
        sep_token: "[SEP]",
        pad_token: "[PAD]",
        cls_token: "[CLS]",
        model_max_length: 512
      })
    );
    const long = Array.from({ length: 300 }, () => "hello").join(" ");
    const encoded = tokenizer.tokenize(["hello", long]);
    expect(encoded.sequenceLength).toBe(256);
    expect(encoded.batchSize).toBe(2);
    expect(encoded.inputIds[0]).toBe(101n);
    expect(encoded.inputIds[2]).toBe(102n);
    expect(encoded.inputIds[256]).toBe(101n);
    expect(encoded.inputIds[511]).toBe(102n);
    expect(encoded.attentionMask[3]).toBe(0n);
    expect(() => tokenizer.tokenize([" \n\t "])).toThrow(/cannot be empty/);
  });

  it("owns load and inference in the Worker, transfers normalized batches, and closes", async () => {
    const fixture = await serviceFixture("wasm");
    const pair = workerPair();
    installBrowserEmbeddingWorker(fixture.dependencies, pair.scope);
    const embedder = await BrowserEmbedder.load({
      worker: pair.worker,
      execution: "auto"
    });
    expect(embedder.provider).toBe("wasm");
    expect(embedder.modelInfo.dimension).toBe(384);
    const one = await embedder.embed("Unicode café 世界");
    expect(one).toHaveLength(384);
    const batch = await embedder.embedBatch(["one", "two"]);
    expect(batch).toHaveLength(2);
    expect(batch[0]).toHaveLength(384);
    expect(fixture.fetchCount()).toBe(6);
    await embedder.close();
    await expect(embedder.embed("closed")).rejects.toBeInstanceOf(EmbeddingClosedError);
    expect(fixture.runtimeClosed()).toBe(true);
  });

  it("serializes inference requests FIFO inside one Worker", async () => {
    const fixture = await serviceFixture("wasm");
    const events: string[] = [];
    const dependencies = {
      ...fixture.dependencies,
      runtimeFactory: {
        async create(): Promise<EmbeddingRuntime> {
          return {
            provider: "wasm",
            async embed(texts): Promise<Float32Array> {
              const text = texts[0] ?? "";
              events.push(`start:${text}`);
              if (text === "first") {
                await new Promise<void>((resolve) => setTimeout(resolve, 20));
              }
              events.push(`end:${text}`);
              const output = new Float32Array(texts.length * 384);
              for (let row = 0; row < texts.length; row += 1) output[row * 384] = 1;
              return output;
            },
            async close(): Promise<void> {}
          };
        }
      }
    };
    const pair = workerPair();
    installBrowserEmbeddingWorker(dependencies, pair.scope);
    const embedder = await BrowserEmbedder.load({ worker: pair.worker });
    await Promise.all([embedder.embed("first"), embedder.embed("second")]);
    expect(events).toEqual([
      "start:first",
      "end:first",
      "start:second",
      "end:second"
    ]);
    await embedder.close();
  });

  it("prefetches without constructing a runtime and supports local-only reload", async () => {
    const fixture = await serviceFixture("wasm");
    const first = workerPair();
    installBrowserEmbeddingWorker(fixture.dependencies, first.scope);
    await BrowserEmbedder.prefetch({ worker: first.worker });
    expect(fixture.runtimeCreates()).toBe(0);

    const second = workerPair();
    installBrowserEmbeddingWorker(fixture.dependencies, second.scope);
    const embedder = await BrowserEmbedder.load({
      worker: second.worker,
      localOnly: true,
      execution: "wasm"
    });
    expect(fixture.fetchCount()).toBe(6);
    await embedder.close();
  });

  it("maps cancellation and Worker crashes to deterministic typed errors", async () => {
    const fixture = await serviceFixture("wasm", true);
    const pair = workerPair();
    installBrowserEmbeddingWorker(fixture.dependencies, pair.scope);
    const embedder = await BrowserEmbedder.load({ worker: pair.worker });
    const controller = new AbortController();
    const pending = embedder.embed("slow", { signal: controller.signal });
    controller.abort();
    await expect(pending).rejects.toBeInstanceOf(EmbeddingCancelledError);
    pair.crash("boom");
    await expect(embedder.embed("later")).rejects.toMatchObject({
      code: "RK_EMBEDDING_WORKER"
    });
  });

  it("rejects malformed runtime output and maps session construction failures", async () => {
    expect(() => validateEmbedding(new Float32Array(383))).toThrow(EmbeddingOutputError);
    const nonFinite = new Float32Array(384);
    nonFinite[0] = Number.NaN;
    expect(() => validateEmbedding(nonFinite)).toThrow(EmbeddingOutputError);
    expect(() => validateEmbedding(new Float32Array(384))).toThrow(EmbeddingOutputError);

    const fixture = await serviceFixture("wasm");
    const pair = workerPair();
    installBrowserEmbeddingWorker(
      {
        ...fixture.dependencies,
        runtimeFactory: {
          async create(): Promise<EmbeddingRuntime> {
            throw new EmbeddingRuntimeError("session construction failed");
          }
        }
      },
      pair.scope
    );
    await expect(BrowserEmbedder.load({ worker: pair.worker })).rejects.toMatchObject({
      code: "RK_EMBEDDING_RUNTIME"
    });
  });
});

async function artifactFixture(): Promise<{
  specs: readonly ArtifactSpec[];
  files: readonly Uint8Array[];
  byUrl: ReadonlyMap<string, Uint8Array>;
}> {
  const paths = [
    "manifest-v1.json",
    "onnx/model.onnx",
    "tokenizer/tokenizer.json",
    "tokenizer/tokenizer_config.json",
    "tokenizer/special_tokens_map.json",
    "tokenizer/vocab.txt"
  ];
  const files = paths.map((path, index) => new TextEncoder().encode(`${path}:${index}`));
  const specs = await Promise.all(paths.map(async (path, index) => {
    const bytes = files[index];
    if (bytes === undefined) throw new Error("artifact fixture file is missing");
    return {
      path,
      bytes: bytes.byteLength,
      sha256: await sha256(bytes),
      url: `https://example.test/${path}`
    };
  }));
  return {
    specs,
    files,
    byUrl: new Map(
      specs.map((spec, index) => {
        const file = files[index];
        if (file === undefined) throw new Error("artifact fixture file is missing");
        return [spec.url, file];
      })
    )
  };
}

async function serviceFixture(provider: "webgpu" | "wasm", slow = false) {
  const artifacts = await artifactFixture();
  const store = new MemoryArtifactStore(`service:${provider}:${slow}`);
  let fetches = 0;
  let creates = 0;
  let closed = false;
  const runtimeFactory: EmbeddingRuntimeFactory = {
    async create(): Promise<EmbeddingRuntime> {
      creates += 1;
      return {
        provider,
        async embed(texts, signal): Promise<Float32Array> {
          if (slow) {
            await new Promise<void>((resolve, reject) => {
              const timer = setTimeout(resolve, 50);
              signal?.addEventListener(
                "abort",
                () => {
                  clearTimeout(timer);
                  reject(new EmbeddingCancelledError());
                },
                { once: true }
              );
            });
          }
          const output = new Float32Array(texts.length * 384);
          for (let row = 0; row < texts.length; row += 1) output[row * 384] = 1;
          return output;
        },
        async close(): Promise<void> {
          closed = true;
        }
      };
    }
  };
  return {
    dependencies: {
      artifacts: artifacts.specs.map((spec) =>
        spec.path === "onnx/model.onnx"
          ? { ...spec, path: "onnx/all-MiniLM-L6-v2-fp32.onnx" }
          : spec
      ),
      fetcher: async (url: string) => {
        fetches += 1;
        return artifacts.byUrl.get(url) ?? new Uint8Array();
      },
      createStore: () => store,
      runtimeFactory
    },
    fetchCount: () => fetches,
    runtimeCreates: () => creates,
    runtimeClosed: () => closed
  };
}

interface WorkerPair {
  readonly worker: BrowserEmbeddingWorkerLike;
  readonly scope: WorkerScopeLike;
  crash(message: string): void;
}

function workerPair(): WorkerPair {
  const main = new Map<string, Set<EventListener>>();
  const workerListeners = new Set<
    (event: MessageEvent<WorkerIncomingMessage>) => void
  >();
  let terminated = false;
  const emit = (type: string, event: Event): void => {
    for (const listener of main.get(type) ?? []) listener(event);
  };
  return {
    worker: {
      postMessage(message): void {
        if (terminated) throw new Error("terminated");
        queueMicrotask(() => {
          for (const listener of workerListeners) {
            listener(
              new MessageEvent<WorkerIncomingMessage>("message", {
                data: message as WorkerIncomingMessage
              })
            );
          }
        });
      },
      addEventListener(type, listener): void {
        const listeners = main.get(type) ?? new Set<EventListener>();
        listeners.add(listener);
        main.set(type, listeners);
      },
      removeEventListener(type, listener): void {
        main.get(type)?.delete(listener);
      },
      terminate(): void {
        terminated = true;
      }
    },
    scope: {
      postMessage(message: WorkerResponse): void {
        if (!terminated) {
          queueMicrotask(() =>
            emit("message", new MessageEvent("message", { data: message }))
          );
        }
      },
      addEventListener(_type, listener): void {
        workerListeners.add(listener);
      },
      removeEventListener(_type, listener): void {
        workerListeners.delete(listener);
      }
    },
    crash(message): void {
      const event = new Event("error");
      Object.defineProperty(event, "message", { value: message });
      emit("error", event);
    }
  };
}

function jsonBytes(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes.slice().buffer);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function recordingStore(
  identity: string,
  inner: ArtifactStore,
  writes: string[],
  failAt?: number
): ArtifactStore {
  return {
    identity,
    read: async (key) => await inner.read(key),
    write: async (key, value) => {
      writes.push(key);
      if (writes.length === failAt) throw new Error("publication interrupted");
      await inner.write(key, value);
    },
    remove: async (key) => await inner.remove(key),
    keys: async (prefix) => await inner.keys(prefix)
  };
}

function tokenizerFixture(): object {
  return {
    version: "1.0",
    truncation: { max_length: 128, strategy: "LongestFirst", stride: 0 },
    padding: null,
    added_tokens: [
      { id: 0, special: true, content: "[PAD]", normalized: false },
      { id: 100, special: true, content: "[UNK]", normalized: false },
      { id: 101, special: true, content: "[CLS]", normalized: false },
      { id: 102, special: true, content: "[SEP]", normalized: false }
    ],
    normalizer: {
      type: "BertNormalizer",
      clean_text: true,
      handle_chinese_chars: true,
      strip_accents: null,
      lowercase: true
    },
    pre_tokenizer: { type: "BertPreTokenizer" },
    post_processor: {
      type: "TemplateProcessing",
      single: [
        { SpecialToken: { id: "[CLS]", type_id: 0 } },
        { Sequence: { id: "A", type_id: 0 } },
        { SpecialToken: { id: "[SEP]", type_id: 0 } }
      ],
      pair: [],
      special_tokens: {
        "[CLS]": { id: "[CLS]", ids: [101], tokens: ["[CLS]"] },
        "[SEP]": { id: "[SEP]", ids: [102], tokens: ["[SEP]"] }
      }
    },
    decoder: { type: "WordPiece", prefix: "##", cleanup: true },
    model: {
      type: "WordPiece",
      unk_token: "[UNK]",
      continuing_subword_prefix: "##",
      max_input_chars_per_word: 100,
      vocab: {
        "[PAD]": 0,
        "[UNK]": 100,
        "[CLS]": 101,
        "[SEP]": 102,
        hello: 103
      }
    }
  };
}
