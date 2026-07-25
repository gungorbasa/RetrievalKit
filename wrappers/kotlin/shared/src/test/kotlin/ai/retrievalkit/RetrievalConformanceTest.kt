package ai.retrievalkit

import com.fasterxml.jackson.databind.JsonNode
import com.fasterxml.jackson.databind.ObjectMapper
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals

class RetrievalConformanceTest {
    private val mapper = ObjectMapper()

    @Test
    fun canonicalRetrievalFixture() {
        val fixture = mapper.readTree(
            Path.of(
                System.getProperty("retrievalkit.repo.root"),
                "benchmarks/retrieval-conformance/v1/fixture.json",
            ).toFile(),
        )
        assertEquals("retrieval-results-v1", fixture["fixture_id"].asText())
        RetrievalDatabase.Builder(
            corpusId = fixture["fixture_id"].asText(),
            metric = VectorMetric.DOT_PRODUCT,
            encoding = VectorEncoding.F32,
        ).use { builder ->
            fixture["documents"].forEach { document ->
                val chunk = document["chunks"].single()
                builder.upsert(
                    Document(
                        document["id"].asText(),
                        chunk["text"].asText(),
                        metadata(document["metadata"]) + metadata(chunk["metadata"]),
                    ),
                    floats(chunk["embedding"]),
                )
            }
            builder.build().use { database ->
                val expectations = fixture["expectations"]
                val exact = database.search(
                    floats(expectations["exact"]["embedding"]),
                    limit = 1,
                )
                assertEquals(strings(expectations["exact"]["document_ids"]), exact.map { it.documentId })
                assertEquals(expectations["exact"]["text"].asText(), exact.single().text)
                assertEquals(metadata(expectations["exact"]["metadata"]), exact.single().metadata)

                val keyword = database.search(expectations["keyword"]["text"].asText(), limit = 1)
                assertEquals(strings(expectations["keyword"]["document_ids"]), keyword.map { it.documentId })
                assertEquals(
                    strings(expectations["keyword"]["matched_terms"]),
                    keyword.single().trace.matchedTerms,
                )

                val hybrid = expectations["hybrid"]
                val hybridHits = database.search(
                    hybrid["text"].asText(),
                    floats(hybrid["embedding"]),
                    alpha = hybrid["alpha"].floatValue(),
                )
                assertEquals(strings(hybrid["document_ids"]), hybridHits.map { it.documentId })
                assertEquals(
                    strings(expectations["alpha_one"]["document_ids"]),
                    database.search(
                        hybrid["text"].asText(),
                        floats(hybrid["embedding"]),
                        limit = 1,
                        alpha = 1f,
                    ).map { it.documentId },
                )
                assertEquals(
                    strings(expectations["alpha_zero"]["document_ids"]),
                    database.search(hybrid["text"].asText(), limit = 1).map { it.documentId },
                )
            }
        }
    }

    private fun floats(node: JsonNode): FloatArray =
        node.map(JsonNode::floatValue).toFloatArray()

    private fun strings(node: JsonNode): List<String> = node.map(JsonNode::asText)

    private fun metadata(node: JsonNode): Metadata = node.fields().asSequence().associate { field ->
        val tagged = field.value.fields().next()
        field.key to when (tagged.key) {
            "String" -> MetadataValue.Text(tagged.value.asText())
            "Integer" -> MetadataValue.Integer(tagged.value.asLong())
            "Float" -> MetadataValue.Decimal(tagged.value.asDouble())
            "Boolean" -> MetadataValue.Boolean(tagged.value.asBoolean())
            "TimestampMillis" -> MetadataValue.TimestampMillis(tagged.value.asLong())
            else -> error("unsupported fixture metadata tag ${tagged.key}")
        }
    }
}
