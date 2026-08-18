package ai.retrievalkit

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class GraphDatabaseTest {
    private val schema = GraphSchema(
        recordNodes = listOf(
            RecordNodeSchema("Topic", "Topic", listOf(FieldPath("name"))),
        ),
        relationships = listOf(
            RelationshipSchema(
                relationshipType = "follows",
                sourceNodeType = "Topic",
                targetNodeType = "Topic",
                sourceField = FieldPath("follows"),
                cardinality = Cardinality.OPTIONAL_ONE,
            ),
        ),
        chunkNodes = ChunkNodeSchema("Chunk", "owns", "ownedBy"),
    )

    private val first = Record(
        id = "bir",
        type = "Topic",
        fields = mapOf(
            "name" to RecordValue.Text("Bir"),
            "follows" to RecordValue.Text("iki"),
        ),
        content = "ilk içerik",
        metadata = mapOf("visible" to MetadataValue.Boolean(true)),
    )
    private val second = Record(
        id = "iki",
        type = "Topic",
        fields = mapOf("name" to RecordValue.Text("İki")),
        content = "ikinci içerik",
        metadata = mapOf("visible" to MetadataValue.Boolean(false)),
    )

    @Test
    fun graphOnlyQueryPathsProjectionAndPersistence() {
        val directory = Files.createTempDirectory("retrievalkit-kotlin-graph-")
        GraphDatabase.Builder("graph-only", schema).use { builder ->
            builder.upsert(listOf(first, second))
            builder.build().use { database ->
                database.query(
                    GraphQuery(
                        GraphSeed.Nodes(listOf(GraphNodeId("Topic", "bir"))),
                        listOf(GraphTraversal("follows")),
                    ),
                ).use { selection ->
                    val snapshot = selection.snapshot
                    assertEquals("iki", snapshot.matches.single().nodeId.recordId)
                    assertEquals("follows", snapshot.matches.single().path.single().relationship)
                    assertEquals("bir", snapshot.matches.single().path.single().source.recordId)
                    assertEquals("iki", snapshot.matches.single().path.single().target.recordId)
                    assertEquals("bir", snapshot.matches.single().path.single().provenance.sourceRecordId)

                    val projection = database.projectCandidates(selection)
                    assertEquals(1, projection.sourceNodes)
                    assertEquals(listOf(ChunkIdentity("iki", "iki")), projection.candidates)
                }
                database.save(directory)
            }
        }
        GraphDatabase.validate(directory)
        GraphDatabase.load(directory).use { loaded ->
            loaded.query(
                GraphQuery(
                    GraphSeed.Equals("Topic", FieldPath("name"), listOf(GraphScalar.Text("İki"))),
                ),
            ).use { selection ->
                assertEquals("iki", selection.snapshot.matches.single().nodeId.recordId)
            }
        }
    }

    @Test
    fun combinedSelectionScopesRetrievalAndRejectsClosedSelection() {
        GraphRetrievalDatabase.Builder(
            corpusId = "combined",
            schema = schema,
            metric = VectorMetric.DOT_PRODUCT,
            encoding = VectorEncoding.F32,
            bm25 = Bm25Configuration(k1 = 1.7f, b = 0.4f, stopWords = setOf("ILK")),
        ).use { builder ->
            builder.upsert(
                listOf(
                    GraphRecordInput(first, floatArrayOf(1f, 0f)),
                    GraphRecordInput(second, floatArrayOf(0f, 1f)),
                ),
            )
            builder.build().use { database ->
                assertTrue(database.search("ilk").isEmpty())
                val selection = database.query(
                    GraphQuery(GraphSeed.Nodes(listOf(GraphNodeId("Topic", "iki")))),
                )
                selection.use {
                    val hits = database.search(floatArrayOf(1f, 0f), within = selection)
                    assertEquals(listOf("iki"), hits.map { hit -> hit.recordId })
                    val filtered = database.projectCandidates(
                        selection,
                        Filter.Equals("visible", MetadataValue.Boolean(false)),
                    )
                    assertEquals(1, filtered.projectedChunksAfterFilter)
                }
                assertFailsWith<ClosedResourceException> {
                    database.search(floatArrayOf(1f, 0f), within = selection)
                }
            }
        }
    }

    @Test
    fun schemaFailuresUseTypedException() {
        val badSchema = GraphSchema(
            recordNodes = listOf(RecordNodeSchema("bad-type", "Topic")),
        )
        val error = assertFailsWith<InvalidIdentityException> {
            GraphDatabase.Builder("bad", badSchema)
        }
        assertTrue(error.message.orEmpty().contains("RecordType"))
    }
}
