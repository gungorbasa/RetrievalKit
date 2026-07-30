import { describe, expect, it } from "vitest";
import {
  browserSupportsWasmSimd128,
  createAdaptiveGeneratedWasmAdapter,
  createGeneratedWasmAdapter,
  type GeneratedGraphDatabase,
  type GeneratedGraphRetrievalDatabase,
  type GeneratedRetrievalDatabase,
  type GeneratedWasmModule
} from "../src/adapter.js";
import {
  RetrievalKitDimensionError,
  RetrievalKitInputError,
  RetrievalKitLifecycleError
} from "../src/errors.js";

class FakeRetrievalDatabase implements GeneratedRetrievalDatabase {
  public static vectorError: Error | undefined;
  public addRecordsBatch(): number {
    return 0;
  }
  public build(): void {}
  public vectorSearch(): unknown {
    if (FakeRetrievalDatabase.vectorError !== undefined) {
      throw FakeRetrievalDatabase.vectorError;
    }
    return [];
  }
  public bm25Search(): unknown {
    return [];
  }
  public hybridSearch(): unknown {
    return [];
  }
  public close(): void {}
}

class FakeGraphDatabase implements GeneratedGraphDatabase {
  public static records?: unknown;
  public addRecordsBatch(records: unknown): number {
    FakeGraphDatabase.records = records;
    return 0;
  }
  public build(): void {}
  public query(): unknown {
    return graphResult();
  }
  public projectCandidates(): unknown {
    return projection();
  }
  public releaseSelection(): boolean {
    return true;
  }
  public close(): void {}
}

class FakeGraphRetrievalDatabase implements GeneratedGraphRetrievalDatabase {
  public static records?: unknown;
  public addRecordsBatch(records: unknown): number {
    FakeGraphRetrievalDatabase.records = records;
    return 0;
  }
  public build(): void {}
  public graphQuery(): unknown {
    return graphResult();
  }
  public projectCandidates(): unknown {
    return projection();
  }
  public vectorSearch(): unknown {
    return [];
  }
  public bm25Search(): unknown {
    return [];
  }
  public hybridSearch(): unknown {
    return [];
  }
  public releaseSelection(): boolean {
    return true;
  }
  public close(): void {}
}

const module: GeneratedWasmModule = {
  buildCapabilities: () => ({
    execution: "dedicated-worker",
    performanceTier: "portable",
    persistence: false,
    threads: false,
    simd: false,
    structuredDtos: true,
    bulkFloat32Embeddings: true
  }),
  RetrievalDatabase: FakeRetrievalDatabase,
  GraphDatabase: FakeGraphDatabase,
  GraphRetrievalDatabase: FakeGraphRetrievalDatabase
};

describe("generated WASM adapter", () => {
  it("selects SIMD128 only when the Worker validates browser support", async () => {
    let portableLoads = 0;
    let simdLoads = 0;
    const simdModule: GeneratedWasmModule = {
      ...module,
      buildCapabilities: () => ({
        execution: "dedicated-worker",
        performanceTier: "simd128",
        persistence: false,
        threads: false,
        simd: true,
        structuredDtos: true,
        bulkFloat32Embeddings: true
      })
    };
    const accelerated = createAdaptiveGeneratedWasmAdapter({
      portable: () => {
        portableLoads += 1;
        return module;
      },
      simd128: () => {
        simdLoads += 1;
        return simdModule;
      },
      supportsSimd128: () => true
    });
    await expect(
      accelerated.initialize(new AbortController().signal)
    ).resolves.toMatchObject({
      performanceTier: "simd128",
      simd: true
    });
    expect({ portableLoads, simdLoads }).toEqual({
      portableLoads: 0,
      simdLoads: 1
    });

    const portable = createAdaptiveGeneratedWasmAdapter({
      portable: () => {
        portableLoads += 1;
        return module;
      },
      simd128: () => {
        simdLoads += 1;
        return simdModule;
      },
      supportsSimd128: () => false
    });
    await expect(
      portable.initialize(new AbortController().signal)
    ).resolves.toMatchObject({
      performanceTier: "portable",
      simd: false
    });
    expect({ portableLoads, simdLoads }).toEqual({
      portableLoads: 1,
      simdLoads: 1
    });
  });

  it("uses a WebAssembly validation probe for SIMD128 support", () => {
    expect(browserSupportsWasmSimd128()).toBe(true);
  });

  it("decodes graph paths and provenance from generated DTOs", async () => {
    const adapter = createGeneratedWasmAdapter(module);
    const signal = new AbortController().signal;
    await adapter.initialize(signal);
    const database = await adapter.createDatabase(
      {
        kind: "graph",
        options: {
          corpusId: "graph",
          schema: { recordNodes: [{ recordType: "note", nodeType: "Note" }] }
        }
      },
      signal
    );
    await adapter.build(database, signal);
    const selection = await adapter.graphQuery(
      database,
      { seed: { kind: "nodes", nodes: [] } },
      signal
    );

    expect(selection.data.matches[0]?.path).toEqual([
      {
        relationship: "LINKS",
        source: {
          kind: "record",
          nodeType: "Note",
          recordId: "one"
        },
        target: {
          kind: "chunk",
          nodeType: "Chunk",
          recordId: "two",
          chunkKey: "summary"
        },
        occurrenceOrdinal: 2,
        provenance: {
          schemaRuleIndex: 3,
          sourceRecordId: "one",
          sourceField: ["links"],
          derivedInverse: false,
          builtIn: false
        }
      }
    ]);
  });

  it("rejects a selection used with another database handle", async () => {
    const adapter = createGeneratedWasmAdapter(module);
    const signal = new AbortController().signal;
    await adapter.initialize(signal);
    const options = {
      corpusId: "graph",
      schema: { recordNodes: [{ recordType: "note", nodeType: "Note" }] }
    };
    const first = await adapter.createDatabase(
      { kind: "graphRetrieval", options },
      signal
    );
    const second = await adapter.createDatabase(
      { kind: "graphRetrieval", options: { ...options, corpusId: "other" } },
      signal
    );
    await adapter.build(first, signal);
    await adapter.build(second, signal);
    const selection = await adapter.graphQuery(
      first,
      { seed: { kind: "nodes", nodes: [] } },
      signal
    );

    await expect(
      adapter.search(
        second,
        {
          mode: "text",
          text: "cross database",
          limit: 10,
          within: selection.handle
        },
        signal
      )
    ).rejects.toBeInstanceOf(RetrievalKitLifecycleError);
  });

  it("creates canonical content chunks for graph-only and retrieval records", async () => {
    const adapter = createGeneratedWasmAdapter(module);
    const signal = new AbortController().signal;
    await adapter.initialize(signal);
    const schema = {
      recordNodes: [{ recordType: "note", nodeType: "Note" }]
    };
    const graph = await adapter.createDatabase(
      { kind: "graph", options: { corpusId: "graph", schema } },
      signal
    );
    await adapter.addGraphRecords(
      graph,
      {
        dimension: 0,
        embeddings: new Float32Array(),
        records: [{ id: "one", type: "note", content: "Graph content" }]
      },
      signal
    );
    expect(FakeGraphDatabase.records).toMatchObject([
      {
        id: "one",
        metadata: [],
        chunks: [
          {
            key: "one",
            text: "Graph content",
            metadata: []
          }
        ]
      }
    ]);

    const combined = await adapter.createDatabase(
      { kind: "graphRetrieval", options: { corpusId: "combined", schema } },
      signal
    );
    await adapter.addGraphRecords(
      combined,
      {
        dimension: 2,
        embeddings: new Float32Array([1, 2]),
        records: [
          {
            id: "two",
            type: "note",
            content: "Retrieval content",
            metadata: { inherited: "record-only" },
            retrieval: {
              kind: "content",
              embeddingOffset: 0,
              dimension: 2
            }
          }
        ]
      },
      signal
    );
    expect(FakeGraphRetrievalDatabase.records).toMatchObject([
      {
        id: "two",
        metadata: [
          {
            field: "inherited",
            value: { kind: "string", value: "record-only" }
          }
        ],
        chunks: [
          {
            key: "two",
            text: "Retrieval content",
            metadata: [],
            embeddingIndex: 0
          }
        ]
      }
    ]);
  });

  it("only permits embedding-free hybrid search at alpha zero", async () => {
    const adapter = createGeneratedWasmAdapter(module);
    const signal = new AbortController().signal;
    await adapter.initialize(signal);
    const database = await adapter.createDatabase(
      { kind: "retrieval", options: { corpusId: "search" } },
      signal
    );
    await adapter.build(database, signal);
    await expect(
      adapter.search(
        database,
        { mode: "hybrid", text: "query", limit: 10 },
        signal
      )
    ).rejects.toBeInstanceOf(RetrievalKitInputError);
    await expect(
      adapter.search(
        database,
        { mode: "hybrid", text: "query", alpha: 0, limit: 10 },
        signal
      )
    ).resolves.toEqual([]);
  });

  it("maps stable generated error prefixes to typed public errors", async () => {
    const adapter = createGeneratedWasmAdapter(module);
    const signal = new AbortController().signal;
    await adapter.initialize(signal);
    const database = await adapter.createDatabase(
      { kind: "retrieval", options: { corpusId: "errors" } },
      signal
    );
    await adapter.build(database, signal);
    FakeRetrievalDatabase.vectorError = new Error(
      "RK_CORE: query embedding dimension mismatch"
    );
    await expect(
      adapter.search(
        database,
        {
          mode: "vector",
          embedding: new Float32Array([1]),
          limit: 10
        },
        signal
      )
    ).rejects.toBeInstanceOf(RetrievalKitDimensionError);
    FakeRetrievalDatabase.vectorError = undefined;
  });
});

function graphResult(): unknown {
  return {
    selectionId: 7,
    corpusId: "graph",
    generation: "1",
    matches: [
      {
        nodeType: "Chunk",
        sourceKind: "chunk",
        recordId: "two",
        chunkKey: "summary",
        depth: 1,
        path: [
          {
            relationship: "LINKS",
            source: {
              nodeType: "Note",
              sourceKind: "record",
              recordId: "one"
            },
            target: {
              nodeType: "Chunk",
              sourceKind: "chunk",
              recordId: "two",
              chunkKey: "summary"
            },
            occurrenceOrdinal: 2,
            schemaRuleIndex: 3,
            sourceRecordId: "one",
            sourceField: ["links"],
            derivedInverse: false,
            builtIn: false
          }
        ]
      }
    ],
    trace: {
      seedCount: 1,
      visitedStates: 2,
      traversedEdges: 1,
      resultCount: 1,
      diagnostics: 0
    }
  };
}

function projection(): unknown {
  return {
    candidates: [],
    sourceNodes: 0,
    projectedChunksBeforeFilter: 0,
    projectedChunksAfterFilter: 0
  };
}
