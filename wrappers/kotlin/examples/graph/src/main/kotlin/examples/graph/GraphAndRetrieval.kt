package examples.graph

import ai.retrievalkit.GraphNodeId
import ai.retrievalkit.GraphQuery
import ai.retrievalkit.GraphRetrievalDatabase
import ai.retrievalkit.GraphSeed
import ai.retrievalkit.VectorEncoding
import ai.retrievalkit.VectorMetric

fun main() {
    GraphRetrievalDatabase.Builder(
        corpusId = "people",
        schema = schema,
        metric = VectorMetric.DOT_PRODUCT,
        encoding = VectorEncoding.F32,
    ).use { builder ->
        builder.upsert(ada, floatArrayOf(1f, 0f))
        builder.upsert(grace, floatArrayOf(0f, 1f))
        builder.build().use { database ->
            database.query(
                GraphQuery(GraphSeed.Nodes(listOf(GraphNodeId("Person", "grace")))),
            ).use { selection ->
                database.search(floatArrayOf(1f, 0f), within = selection).forEach { hit ->
                    println("${hit.recordId}: ${hit.text}")
                }
            }
        }
    }
}
