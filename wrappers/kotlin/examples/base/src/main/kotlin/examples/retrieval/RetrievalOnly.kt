package examples.retrieval

import ai.retrievalkit.Document
import ai.retrievalkit.RetrievalDatabase
import ai.retrievalkit.VectorEncoding

fun main() {
    RetrievalDatabase.Builder("quickstart", encoding = VectorEncoding.F32).use { builder ->
        builder.upsert(Document("kotlin", "Kotlin calls the local Rust retrieval core."), floatArrayOf(1f, 0f))
        builder.upsert(Document("rust", "Rust owns ranking and filtering."), floatArrayOf(0f, 1f))
        builder.build().use { database ->
            database.search(floatArrayOf(1f, 0f), limit = 1).forEach { hit ->
                println("${hit.documentId}: ${hit.text} (${hit.score})")
            }
        }
    }
}
