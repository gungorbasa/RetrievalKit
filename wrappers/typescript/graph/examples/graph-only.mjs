import { GraphDatabaseBuilder } from "@gungorbasa/retrievalkit-graph";

const builder = new GraphDatabaseBuilder({
  corpusId: "graph-example",
  schema: {
    recordNodes: [
      { recordType: "Topic", nodeType: "Topic", queryableFields: [["title"]] }
    ]
  }
});
await builder.add([
  { id: "local", type: "Topic", fields: { title: "Local" }, content: "Local graph" }
]);
const database = await builder.build();
try {
  const selection = await database.graph.query({
    seed: { kind: "equals", nodeType: "Topic", field: ["title"], values: ["Local"] }
  });
  try {
    console.log(selection.matches);
  } finally {
    await selection.close();
  }
} finally {
  await database.close();
}
