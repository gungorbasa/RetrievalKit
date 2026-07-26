import { GraphRetrievalDatabaseBuilder } from "@gungorbasa/retrievalkit-graph";

const builder = new GraphRetrievalDatabaseBuilder({
  corpusId: "combined-example",
  metric: "dotProduct",
  encoding: "f32",
  schema: {
    recordNodes: [
      { recordType: "Topic", nodeType: "Topic", queryableFields: [["title"]] }
    ]
  }
});
await builder.add([
  {
    id: "local",
    type: "Topic",
    fields: { title: "Local" },
    content: "Local graph retrieval",
    retrieval: { kind: "content", embedding: new Float32Array([1, 0]) }
  }
]);
const database = await builder.build();
try {
  const selection = await database.graph.query({
    seed: { kind: "equals", nodeType: "Topic", field: ["title"], values: ["Local"] }
  });
  try {
    console.log(
      await database.retrieval.search({
        mode: "vector",
        embedding: new Float32Array([1, 0]),
        within: selection
      })
    );
  } finally {
    await selection.close();
  }
} finally {
  await database.close();
}
