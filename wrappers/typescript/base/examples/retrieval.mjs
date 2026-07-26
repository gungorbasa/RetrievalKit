import { RetrievalDatabaseBuilder } from "@gungorbasa/retrievalkit";

const builder = new RetrievalDatabaseBuilder({ corpusId: "example-notes" });
await builder.add([
  {
    id: "one",
    text: "RetrievalKit keeps search local.",
    embedding: new Float32Array([1, 0])
  },
  {
    id: "two",
    text: "BM25 finds exact terms.",
    embedding: new Float32Array([0, 1])
  }
]);
await using database = await builder.build();
console.log(
  await database.search({
    mode: "hybrid",
    text: "exact terms",
    embedding: new Float32Array([1, 0]),
    alpha: 0.4
  })
);
