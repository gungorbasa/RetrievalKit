import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  GraphDatabase,
  GraphDatabaseBuilder,
  GraphRetrievalDatabase,
  GraphRetrievalDatabaseBuilder,
  RetrievalKitLifecycleError,
  RetrievalKitStaleSelectionError,
  type Bm25Configuration,
  type GraphSchema
} from "../src/index.js";

const schema: GraphSchema = {
  recordNodes: [
    { recordType: "Topic", nodeType: "Topic", queryableFields: [["title"]] }
  ],
  relationships: [
    {
      relationship: "related_to",
      sourceNodeType: "Topic",
      targetNodeType: "Topic",
      sourceField: ["related_id"],
      cardinality: "optionalOne",
      inverseRelationship: "related_from"
    }
  ]
};
const databases: Array<GraphDatabase | GraphRetrievalDatabase> = [];
const directories: string[] = [];

afterEach(async () => {
  await Promise.all(databases.splice(0).map((database) => database.close()));
  await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true })));
});

function records() {
  return [
    {
      id: "alpha",
      type: "Topic",
      fields: { title: "Alpha", related_id: "beta" },
      metadata: { tenant: "red", rank: 1n },
      retrieval: {
        kind: "documents" as const,
        documents: [
          {
            id: "summary",
            text: "alpha local search",
            embedding: new Float32Array([1, 0])
          }
        ]
      }
    },
    {
      id: "beta",
      type: "Topic",
      fields: { title: "Beta", related_id: "gamma" },
      metadata: { tenant: "blue", rank: 2n },
      retrieval: {
        kind: "documents" as const,
        documents: [
          {
            id: "summary",
            text: "beta graph foundations",
            embedding: new Float32Array([0.6, 0.8])
          }
        ]
      }
    },
    {
      id: "gamma",
      type: "Topic",
      fields: { title: "Gamma" },
      metadata: { tenant: "blue", rank: 3n },
      retrieval: {
        kind: "documents" as const,
        documents: [
          {
            id: "summary",
            text: "gamma retrieval result özel",
            embedding: new Float32Array([0, 1])
          }
        ]
      }
    }
  ];
}

async function combined(
  corpusId = "node-graph-tests",
  bm25?: Bm25Configuration
): Promise<GraphRetrievalDatabase> {
  const builder = new GraphRetrievalDatabaseBuilder({
    corpusId,
    schema,
    metric: "dotProduct",
    encoding: "f32",
    ...(bm25 === undefined ? {} : { bm25 })
  });
  await builder.add(records());
  const database = await builder.build();
  databases.push(database);
  return database;
}

describe("graph aggregate", () => {
  it("rejects loading the base native aggregate in the same process", async () => {
    const baseModule = new URL("../../base/src/index.ts", import.meta.url).href;
    await expect(import(baseModule)).rejects.toThrow(
      /cannot load the RetrievalKit base native aggregate/
    );
  });

  it("runs graph-only queries independently", async () => {
    const builder = new GraphDatabaseBuilder({ corpusId: "graph-only", schema });
    await builder.add(
      records().map((record) => ({
        id: record.id,
        type: record.type,
        fields: record.fields,
        metadata: record.metadata,
        content: `${record.id} content`
      }))
    );
    const database = await builder.build();
    databases.push(database);
    const selection = await database.graph.query({
      seed: {
        kind: "equals",
        nodeType: "Topic",
        field: ["title"],
        values: ["Beta"]
      }
    });
    expect(selection.matches.map((match) => match.node.recordId)).toEqual(["beta"]);
    await selection.close();
  });

  it("returns typed full paths and stable candidate projections", async () => {
    const database = await combined();
    const selection = await database.graph.query({
      seed: {
        kind: "nodes",
        nodes: [{ kind: "record", nodeType: "Topic", recordId: "alpha" }]
      },
      traverse: [{ relationship: "related_to", minHops: 1, maxHops: 2 }]
    });
    expect(selection.matches.map((match) => match.node.recordId)).toEqual(["beta", "gamma"]);
    expect(selection.matches[1]?.path.map((edge) => edge.relationship)).toEqual([
      "related_to",
      "related_to"
    ]);
    expect(selection.matches[0]?.path[0]?.provenance.sourceRecordId).toBe("alpha");
    const projection = await database.graph.projectCandidates(selection, {
      where: { kind: "equals", field: "tenant", value: "blue" }
    });
    expect(projection.candidates).toEqual([
      { recordId: "beta", chunkKey: "summary" },
      { recordId: "gamma", chunkKey: "summary" }
    ]);
    expect(projection.sourceNodes).toBe(2);
    expect(projection.projectedChunksBeforeFilter).toBe(2);
    expect(projection.projectedChunksAfterFilter).toBe(2);
    await selection.close();
  });

  it("matches the shared graph conformance expectations", async () => {
    interface GraphFixture {
      expectations: {
        equality: { node_ids: string[] };
        traversal: { node_ids: string[]; paths: string[][] };
        keyword: { text: string; record_ids: string[] };
      };
    }
    const fixtureUrl = new URL(
      "../../../../benchmarks/graph-conformance/v1/fixture.json",
      import.meta.url
    );
    const fixture = JSON.parse(await readFile(fixtureUrl, "utf8")) as GraphFixture;
    const database = await combined("shared-graph-conformance");
    const equality = await database.graph.query({
      seed: {
        kind: "equals",
        nodeType: "Topic",
        field: ["title"],
        values: ["Beta"]
      }
    });
    expect(equality.matches.map((match) => match.node.recordId)).toEqual(
      fixture.expectations.equality.node_ids
    );
    await equality.close();
    const traversal = await database.graph.query({
      seed: {
        kind: "nodes",
        nodes: [{ kind: "record", nodeType: "Topic", recordId: "alpha" }]
      },
      traverse: [{ relationship: "related_to", minHops: 1, maxHops: 2 }]
    });
    expect(traversal.matches.map((match) => match.node.recordId)).toEqual(
      fixture.expectations.traversal.node_ids
    );
    expect(
      traversal.matches.map((match) => match.path.map((edge) => edge.relationship))
    ).toEqual(fixture.expectations.traversal.paths);
    await traversal.close();
    expect(
      (
        await database.retrieval.search({
          mode: "text",
          text: fixture.expectations.keyword.text
        })
      ).map((hit) => hit.documentId)
    ).toEqual(fixture.expectations.keyword.record_ids);
  });

  it("scopes the one retrieval search family with a graph selection", async () => {
    const database = await combined();
    const selection = await database.graph.query({
      seed: {
        kind: "equals",
        nodeType: "Topic",
        field: ["title"],
        values: ["Beta", "Gamma"]
      }
    });
    const hits = await database.retrieval.search({
      mode: "vector",
      embedding: new Float32Array([1, 0]),
      within: selection,
      where: { kind: "equals", field: "tenant", value: "blue" }
    });
    expect(hits.map((hit) => hit.documentId)).toEqual(["beta", "gamma"]);
    await selection.close();
  });

  it("persists, validates, and reloads the combined aggregate", async () => {
    const database = await combined();
    const directory = await mkdtemp(join(tmpdir(), "retrievalkit-node-graph-"));
    directories.push(directory);
    const report = await database.save(directory);
    expect(report.totalBytes).toBeGreaterThan(0);
    await GraphRetrievalDatabase.validate(directory);
    const loaded = await GraphRetrievalDatabase.load(directory);
    databases.push(loaded);
    const hits = await loaded.retrieval.search({ mode: "text", text: "retrieval", limit: 1 });
    expect(hits[0]?.documentId).toBe("gamma");
  });

  it("applies persisted BM25 configuration to scoped and unscoped text search", async () => {
    const database = await combined("graph-configured-bm25", {
      k1: 1.7,
      b: 0.4,
      stopWords: ["GRAPH"]
    });
    expect(await database.retrieval.search({ mode: "text", text: "graph" })).toEqual([]);
    const selection = await database.graph.query({
      seed: {
        kind: "nodes",
        nodes: [{ kind: "record", nodeType: "Topic", recordId: "beta" }]
      }
    });
    expect(
      await database.retrieval.search({ mode: "text", text: "graph", within: selection })
    ).toEqual([]);
    await selection.close();

    const directory = await mkdtemp(join(tmpdir(), "retrievalkit-node-graph-bm25-"));
    directories.push(directory);
    await database.save(directory);
    const loaded = await GraphRetrievalDatabase.load(directory);
    databases.push(loaded);
    expect(await loaded.retrieval.search({ mode: "text", text: "graph" })).toEqual([]);
  });

  it("rejects cross-corpus projection in Rust and closes all owners", async () => {
    const left = await combined("left");
    const right = await combined("right");
    const selection = await left.graph.query({
      seed: {
        kind: "nodes",
        nodes: [{ kind: "record", nodeType: "Topic", recordId: "alpha" }]
      }
    });
    await expect(right.graph.projectCandidates(selection)).rejects.toBeInstanceOf(
      RetrievalKitStaleSelectionError
    );
    await selection.close();
    await expect(left.graph.projectCandidates(selection)).rejects.toBeInstanceOf(
      RetrievalKitLifecycleError
    );

    const builder = new GraphDatabaseBuilder({ corpusId: "close-builder", schema });
    await builder.close();
    await expect(builder.add([])).rejects.toBeInstanceOf(RetrievalKitLifecycleError);
  });
});
