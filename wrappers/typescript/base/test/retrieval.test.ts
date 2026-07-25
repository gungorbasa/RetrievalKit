import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  RetrievalDatabase,
  RetrievalDatabaseBuilder,
  RetrievalKitDimensionError,
  RetrievalKitLifecycleError,
  floatingPoint,
  timestampMillis,
  type Metadata,
  type MetadataValue
} from "../src/index.js";

const databases: RetrievalDatabase[] = [];
const directories: string[] = [];

afterEach(async () => {
  await Promise.all(databases.splice(0).map((database) => database.close()));
  await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true })));
});

async function fixtureDatabase(): Promise<RetrievalDatabase> {
  const builder = new RetrievalDatabaseBuilder({
    corpusId: "node-base-tests",
    metric: "dotProduct",
    encoding: "f32"
  });
  await builder.add([
    {
      id: "vector-doc",
      text: "semantic local result Belge ğ",
      embedding: new Float32Array([1, 0]),
      metadata: {
        title: "Belge ğ",
        count: 7n,
        exact_i64: 9_007_199_254_740_992n,
        weight: floatingPoint(2.5),
        active: true,
        created_at: timestampMillis(1_700_000_000_000n),
        tenant: "red"
      }
    },
    {
      id: "keyword-doc",
      text: "rare keyword özel",
      embedding: new Float32Array([0, 1]),
      metadata: { tenant: "blue" }
    }
  ]);
  const database = await builder.build();
  databases.push(database);
  return database;
}

describe("RetrievalDatabase", () => {
  it("matches the shared retrieval conformance fixture", async () => {
    interface TaggedValue {
      String?: string;
      Integer?: number;
      Float?: number;
      Boolean?: boolean;
      TimestampMillis?: number;
    }
    interface Fixture {
      metric: string;
      documents: Array<{
        id: string;
        metadata: Record<string, TaggedValue>;
        chunks: Array<{
          text: string;
          embedding: number[];
          metadata: Record<string, TaggedValue>;
        }>;
      }>;
      expectations: {
        exact: { embedding: number[]; document_ids: string[] };
        keyword: { text: string; document_ids: string[] };
        hybrid: {
          text: string;
          embedding: number[];
          alpha: number;
          document_ids: string[];
        };
      };
    }
    const fixtureUrl = new URL(
      "../../../../benchmarks/retrieval-conformance/v1/fixture.json",
      import.meta.url
    );
    const fixture = JSON.parse(await readFile(fixtureUrl, "utf8")) as Fixture;
    const decode = (values: Record<string, TaggedValue>): Metadata =>
      Object.fromEntries(
        Object.entries(values).map(([field, tagged]): [string, MetadataValue] => {
          if (tagged.String !== undefined) return [field, tagged.String];
          if (tagged.Integer !== undefined) return [field, BigInt(tagged.Integer)];
          if (tagged.Float !== undefined) return [field, floatingPoint(tagged.Float)];
          if (tagged.Boolean !== undefined) return [field, tagged.Boolean];
          return [field, timestampMillis(BigInt(tagged.TimestampMillis ?? 0))];
        })
      );
    const builder = new RetrievalDatabaseBuilder({
      corpusId: "shared-retrieval-conformance",
      metric: fixture.metric === "dot_product" ? "dotProduct" : "cosine",
      encoding: "f32"
    });
    await builder.add(
      fixture.documents.map((document) => {
        const chunk = document.chunks[0];
        if (chunk === undefined) throw new Error(`Fixture document ${document.id} has no chunk`);
        return {
          id: document.id,
          text: chunk.text,
          embedding: new Float32Array(chunk.embedding),
          metadata: { ...decode(document.metadata), ...decode(chunk.metadata) }
        };
      })
    );
    const database = await builder.build();
    databases.push(database);
    expect(
      (
        await database.search({
          mode: "vector",
          embedding: new Float32Array(fixture.expectations.exact.embedding),
          limit: 1
        })
      ).map((hit) => hit.documentId)
    ).toEqual(fixture.expectations.exact.document_ids);
    expect(
      (
        await database.search({
          mode: "text",
          text: fixture.expectations.keyword.text,
          limit: 1
        })
      ).map((hit) => hit.documentId)
    ).toEqual(fixture.expectations.keyword.document_ids);
    expect(
      (
        await database.search({
          mode: "hybrid",
          text: fixture.expectations.hybrid.text,
          embedding: new Float32Array(fixture.expectations.hybrid.embedding),
          alpha: fixture.expectations.hybrid.alpha,
          limit: 2
        })
      ).map((hit) => hit.documentId)
    ).toEqual(fixture.expectations.hybrid.document_ids);
  });

  it("preserves Unicode, typed metadata, and exact i64 values", async () => {
    const database = await fixtureDatabase();
    const [hit] = await database.search({
      mode: "vector",
      embedding: new Float32Array([1, 0]),
      where: { kind: "equals", field: "tenant", value: "red" }
    });
    expect(hit?.documentId).toBe("vector-doc");
    expect(hit?.text).toContain("ğ");
    expect(hit?.metadata.exact_i64).toBe(9_007_199_254_740_992n);
    expect(hit?.trace.kind).toBe("vector");
  });

  it("uses the shared alpha endpoints through one search family", async () => {
    const database = await fixtureDatabase();
    const vectorOnly = await database.search({
      mode: "hybrid",
      text: "rare keyword",
      embedding: new Float32Array([1, 0]),
      alpha: 1,
      limit: 1
    });
    const textOnly = await database.search({
      mode: "text",
      text: "rare keyword",
      limit: 1
    });
    expect(vectorOnly.map((hit) => hit.documentId)).toEqual(["vector-doc"]);
    expect(textOnly.map((hit) => hit.documentId)).toEqual(["keyword-doc"]);
  });

  it("persists, validates, and reloads with BM25 rebuilt", async () => {
    const database = await fixtureDatabase();
    const directory = await mkdtemp(join(tmpdir(), "retrievalkit-node-base-"));
    directories.push(directory);
    const report = await database.save(directory);
    expect(report.totalBytes).toBeGreaterThan(0);
    await RetrievalDatabase.validate(directory);
    const loaded = await RetrievalDatabase.load(directory);
    databases.push(loaded);
    const hits = await loaded.search({ mode: "text", text: "rare keyword", limit: 1 });
    expect(hits[0]?.documentId).toBe("keyword-doc");
  });

  it("maps Rust dimension failures to typed errors", async () => {
    const database = await fixtureDatabase();
    await expect(
      database.search({ mode: "vector", embedding: new Float32Array([1]) })
    ).rejects.toBeInstanceOf(RetrievalKitDimensionError);
  });

  it("releases builders and databases deterministically", async () => {
    const builder = new RetrievalDatabaseBuilder({ corpusId: "closed-builder" });
    await builder.close();
    await expect(
      builder.add([
        { id: "x", text: "x", embedding: new Float32Array([1, 0]) }
      ])
    ).rejects.toBeInstanceOf(RetrievalKitLifecycleError);

    const database = await fixtureDatabase();
    await database.close();
    await expect(
      database.search({ mode: "vector", embedding: new Float32Array([1, 0]) })
    ).rejects.toBeInstanceOf(RetrievalKitLifecycleError);
  });
});
