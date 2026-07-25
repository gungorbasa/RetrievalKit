package ai.retrievalkit

import com.fasterxml.jackson.databind.JsonNode
import com.fasterxml.jackson.databind.ObjectMapper
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals

class GraphConformanceTest {
    private val mapper = ObjectMapper()

    @Test
    fun canonicalGraphFixture() {
        val fixture = mapper.readTree(
            Path.of(
                System.getProperty("retrievalkit.repo.root"),
                "benchmarks/graph-conformance/v1/fixture.json",
            ).toFile(),
        )
        assertEquals("generic-topics-v1", fixture["fixture_id"].asText())
        val schema = schema(fixture["schema"])
        GraphRetrievalDatabase.Builder(
            fixture["corpus_id"].asText(),
            schema,
            VectorMetric.DOT_PRODUCT,
            VectorEncoding.F32,
        ).use { builder ->
            fixture["records"].forEach { input ->
                val chunk = input["chunks"].single()
                builder.upsertFixtureChunk(
                    record(input["record"], input["projected_metadata"]),
                    chunk["key"].asText(),
                    chunk["text"].asText(),
                    floats(chunk["embedding"]),
                    metadata(chunk["metadata"]),
                )
            }
            builder.build().use { database ->
                val expected = fixture["expectations"]
                val equality = expected["equality"]
                database.query(
                    GraphQuery(
                        GraphSeed.Equals(
                            equality["node_type"].asText(),
                            FieldPath(strings(equality["field"])),
                            listOf(GraphScalar.Text(equality["value"].asText())),
                        ),
                    ),
                ).use { selection ->
                    assertEquals(strings(equality["node_ids"]), selection.snapshot.matches.map { it.nodeId.recordId })
                    assertEquals(equality["source_nodes"].asInt(), database.projectCandidates(selection).sourceNodes)
                }

                val traversal = expected["traversal"]
                database.query(
                    GraphQuery(
                        GraphSeed.Nodes(
                            listOf(GraphNodeId("Topic", traversal["seed_record_id"].asText())),
                        ),
                        listOf(
                            GraphTraversal(
                                traversal["relationship"].asText(),
                                minHops = traversal["min_hops"].asInt(),
                                maxHops = traversal["max_hops"].asInt(),
                            ),
                        ),
                    ),
                ).use { selection ->
                    val snapshot = selection.snapshot
                    assertEquals(strings(traversal["node_ids"]), snapshot.matches.map { it.nodeId.recordId })
                    assertEquals(
                        traversal["paths"].map { strings(it) },
                        snapshot.matches.map { match -> match.path.map { it.relationship } },
                    )
                    val projection = database.projectCandidates(selection)
                    assertEquals(listOf("beta", "gamma"), projection.candidates.map { it.recordId })
                    assertEquals(listOf("summary", "summary"), projection.candidates.map { it.chunkKey })
                    val filteredProjection = database.projectCandidates(
                        selection,
                        Filter.Range("rank", lower = MetadataValue.Integer(3)),
                    )
                    assertEquals(
                        listOf(ChunkIdentity("gamma", "summary")),
                        filteredProjection.candidates,
                    )
                    assertEquals(2, filteredProjection.projectedChunksBeforeFilter)
                    assertEquals(1, filteredProjection.projectedChunksAfterFilter)
                }

                val filtered = expected["filtered_exact"]
                database.query(
                    GraphQuery(
                        GraphSeed.Equals(
                            "Topic",
                            FieldPath("title"),
                            strings(filtered["seed_titles"]).map(GraphScalar::Text),
                        ),
                    ),
                ).use { selection ->
                    assertEquals(
                        strings(filtered["record_ids"]),
                        database.search(
                            floats(filtered["embedding"]),
                            filter = Filter.Equals(
                                filtered["filter_field"].asText(),
                                MetadataValue.Text(filtered["filter_value"].asText()),
                            ),
                            within = selection,
                        ).map { it.recordId },
                    )
                }
                assertEquals(
                    strings(expected["keyword"]["record_ids"]),
                    database.search(expected["keyword"]["text"].asText()).map { it.recordId },
                )
            }
        }
    }

    @Test
    fun projectionRejectsCrossCorpusSelection() {
        fun database(corpus: String): GraphRetrievalDatabase {
            val schema = GraphSchema(listOf(RecordNodeSchema("Topic", "Topic")))
            val builder = GraphRetrievalDatabase.Builder(
                corpus,
                schema,
                VectorMetric.DOT_PRODUCT,
                VectorEncoding.F32,
            )
            builder.upsert(Record("one", "Topic", content = "one"), floatArrayOf(1f, 0f))
            return builder.build()
        }
        database("first").use { first ->
            database("second").use { second ->
                first.query(
                    GraphQuery(GraphSeed.Nodes(listOf(GraphNodeId("Topic", "one")))),
                ).use { selection ->
                    kotlin.test.assertFailsWith<StaleSelectionException> {
                        second.projectCandidates(selection)
                    }
                }
            }
        }
    }

    private fun schema(node: JsonNode): GraphSchema = GraphSchema(
        recordNodes = node["record_nodes"].map {
            RecordNodeSchema(
                it["record_type"].asText(),
                it["node_type"].asText(),
                it["queryable_fields"].map { path -> FieldPath(strings(path)) },
            )
        },
        relationships = node["relationships"].map {
            RelationshipSchema(
                it["relationship_type"].asText(),
                it["source_node_type"].asText(),
                it["target_node_type"].asText(),
                FieldPath(strings(it["source_field"])),
                Cardinality.valueOf(it["cardinality"].asText().replace("OptionalOne", "OPTIONAL_ONE")),
                MissingTargetPolicy.valueOf(it["missing_target"].asText().uppercase()),
                DuplicateReferencePolicy.valueOf(it["duplicate_references"].asText().uppercase()),
                it["allow_self_edge"].asBoolean(),
                it["inverse_relationship"]?.takeUnless(JsonNode::isNull)?.asText(),
            )
        },
    )

    private fun record(node: JsonNode, projected: JsonNode): Record = Record(
        id = node["id"].asText(),
        type = node["record_type"].asText(),
        fields = node["fields"].fields().asSequence().associate {
            val tagged = it.value.fields().next()
            it.key to when (tagged.key) {
                "String" -> RecordValue.Text(tagged.value.asText())
                else -> error("unsupported fixture record tag ${tagged.key}")
            }
        },
        content = node["content"]?.takeUnless(JsonNode::isNull)?.asText(),
        metadata = metadata(projected),
    )

    private fun floats(node: JsonNode): FloatArray = node.map(JsonNode::floatValue).toFloatArray()
    private fun strings(node: JsonNode): List<String> = node.map(JsonNode::asText)
    private fun metadata(node: JsonNode): Metadata = node.fields().asSequence().associate {
        val tagged = it.value.fields().next()
        it.key to when (tagged.key) {
            "String" -> MetadataValue.Text(tagged.value.asText())
            "Integer" -> MetadataValue.Integer(tagged.value.asLong())
            else -> error("unsupported fixture metadata tag ${tagged.key}")
        }
    }
}
