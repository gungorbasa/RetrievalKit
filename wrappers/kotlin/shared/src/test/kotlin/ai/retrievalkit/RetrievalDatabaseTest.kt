package ai.retrievalkit

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class RetrievalDatabaseTest {
    @Test
    fun lifecycleUnicodeMetadataAlphaAndPersistence() {
        val documents = listOf(
            EmbeddedDocument(
                Document(
                    id = "belge-ğ",
                    text = "İstanbul native arama",
                    metadata = mapOf(
                        "kind" to MetadataValue.Text("not"),
                        "rank" to MetadataValue.Integer(2),
                        "ratio" to MetadataValue.Decimal(1.5),
                        "active" to MetadataValue.Boolean(true),
                        "created" to MetadataValue.TimestampMillis(1_700_000_000_000),
                    ),
                ),
                floatArrayOf(1f, 0f),
            ),
            EmbeddedDocument(
                Document("second", "keyword only document"),
                floatArrayOf(0f, 1f),
            ),
        )
        val directory = Files.createTempDirectory("retrievalkit-kotlin-")
        RetrievalDatabase.Builder(
            corpusId = "unicode",
            metric = VectorMetric.DOT_PRODUCT,
            encoding = VectorEncoding.F32,
            bm25 = Bm25Configuration(k1 = 1.7f, b = 0.4f, stopWords = setOf("ONLY")),
        ).use { builder ->
            builder.upsert(documents)
            builder.build().use { database ->
                assertEquals(2, database.dimension)
                val vector = database.search(floatArrayOf(1f, 0f), limit = 1)
                assertEquals("belge-ğ", vector.single().documentId)
                assertEquals(MetadataValue.Text("not"), vector.single().metadata["kind"])

                val keyword = database.search("keyword", limit = 1)
                assertEquals("second", keyword.single().documentId)
                assertEquals(listOf("keyword"), keyword.single().matchedTerms)
                assertTrue(database.search("only").isEmpty())

                val filtered = database.search(
                    embedding = floatArrayOf(1f, 0f),
                    filter = Filter.Equals("active", MetadataValue.Boolean(true)),
                )
                assertEquals(listOf("belge-ğ"), filtered.map { it.documentId })
                database.save(directory, includeBm25 = false)
            }
        }

        RetrievalDatabase.validate(directory)
        RetrievalDatabase.load(directory).use { loaded ->
            assertEquals("second", loaded.search("keyword", limit = 1).single().documentId)
            assertTrue(loaded.search("only").isEmpty())
            assertFailsWith<InvalidDimensionException> {
                loaded.search(floatArrayOf(1f), limit = 1)
            }
        }

        val closed = RetrievalDatabase.load(directory)
        closed.close()
        assertFailsWith<ClosedResourceException> {
            closed.search(floatArrayOf(1f, 0f))
        }
        assertTrue(Files.exists(directory.resolve("manifest.json")))
    }

    @Test
    fun invalidAlphaIsTypedAndActionable() {
        RetrievalDatabase.Builder("alpha", encoding = VectorEncoding.F32).use { builder ->
            builder.upsert(Document("one", "one"), floatArrayOf(1f, 0f))
            builder.build().use { database ->
                val error = assertFailsWith<InvalidQueryException> {
                    database.search("one", floatArrayOf(1f, 0f), alpha = 1.1f)
                }
                assertTrue(error.message.orEmpty().contains("alpha"))
                assertTrue(error.message.orEmpty().contains("between 0 and 1"))
            }
        }
    }
}
