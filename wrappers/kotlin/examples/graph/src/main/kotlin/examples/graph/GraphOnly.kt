package examples.graph

import ai.retrievalkit.GraphDatabase
import ai.retrievalkit.GraphNodeId
import ai.retrievalkit.GraphQuery
import ai.retrievalkit.GraphSeed
import ai.retrievalkit.GraphTraversal

fun main() {
    GraphDatabase.Builder("people", schema).use { builder ->
        builder.upsert(listOf(ada, grace))
        builder.build().use { database ->
            database.query(
                GraphQuery(
                    seed = GraphSeed.Nodes(listOf(GraphNodeId("Person", "ada"))),
                    traversals = listOf(GraphTraversal("knows")),
                ),
            ).use { selection ->
                selection.snapshot.matches.forEach { println(it.nodeId.recordId) }
            }
        }
    }
}
