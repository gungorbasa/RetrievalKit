package examples.graph

import ai.retrievalkit.Cardinality
import ai.retrievalkit.FieldPath
import ai.retrievalkit.GraphSchema
import ai.retrievalkit.Record
import ai.retrievalkit.RecordNodeSchema
import ai.retrievalkit.RecordValue
import ai.retrievalkit.RelationshipSchema

internal val schema = GraphSchema(
    recordNodes = listOf(RecordNodeSchema("Person", "Person", listOf(FieldPath("name")))),
    relationships = listOf(
        RelationshipSchema(
            relationshipType = "knows",
            sourceNodeType = "Person",
            targetNodeType = "Person",
            sourceField = FieldPath("knows"),
            cardinality = Cardinality.OPTIONAL_ONE,
        ),
    ),
)

internal val ada = Record(
    id = "ada",
    type = "Person",
    fields = mapOf("name" to RecordValue.Text("Ada"), "knows" to RecordValue.Text("grace")),
    content = "Ada builds analytical engines.",
)

internal val grace = Record(
    id = "grace",
    type = "Person",
    fields = mapOf("name" to RecordValue.Text("Grace")),
    content = "Grace designs compilers.",
)
