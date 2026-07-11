use std::collections::BTreeMap;

use vectorkit_core::{
    ChunkId, ChunkIdentity, ChunkInput, ChunkKey, CorpusId, Document, ExactVectorIndex, FieldName,
    Filter, HybridQuery, IndexConfig, KeywordQuery, Metadata, MetadataValue, Record,
    RecordChunkInput, RecordId, RecordType, RecordValue, SearchQuery, VectorEncoding,
    VectorKitError, VectorMetric,
};

fn build_index(corpus: &str, count: usize) -> (ExactVectorIndex, Vec<ChunkId>) {
    let config =
        IndexConfig::new(3, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
    let mut index =
        ExactVectorIndex::try_with_config_in_corpus(config, CorpusId::new(corpus).unwrap())
            .unwrap();
    let mut ids = Vec::with_capacity(count);
    for ordinal in 0..count {
        let group = if ordinal % 2 == 0 { "even" } else { "odd" };
        let document = Document {
            id: format!("record-{ordinal}"),
            text: format!("record {ordinal}"),
            metadata: BTreeMap::from([(
                "group".to_owned(),
                MetadataValue::String(group.to_owned()),
            )]),
        };
        let chunk_ids = index
            .upsert_document(
                document,
                vec![ChunkInput {
                    text: format!("shared token record {ordinal}"),
                    embedding: vec![ordinal as f32 + 1.0, 1.0, 0.0],
                    metadata: Metadata::new(),
                }],
            )
            .unwrap();
        ids.push(chunk_ids[0]);
    }
    (index, ids)
}

fn hit_ids<T>(hits: &[T], id: impl Fn(&T) -> ChunkId) -> Vec<ChunkId> {
    hits.iter().map(id).collect()
}

fn canonical_record(id: &str, title: &str) -> Record {
    Record {
        id: RecordId::new(id).unwrap(),
        record_type: RecordType::new("Note").unwrap(),
        fields: BTreeMap::from([(
            FieldName::new("title").unwrap(),
            RecordValue::String(title.to_owned()),
        )]),
        content: Some(format!("canonical content for {title}")),
    }
}

fn record_chunk(key: &str, text: &str, embedding: Vec<f32>) -> RecordChunkInput {
    RecordChunkInput {
        key: ChunkKey::new(key).unwrap(),
        text: text.to_owned(),
        embedding,
        metadata: Metadata::new(),
    }
}

#[test]
fn full_scope_matches_every_unscoped_ranker() {
    let (index, ids) = build_index("equivalence", 16);
    let scope = index.candidate_scope(ids).unwrap();

    let vector_query = SearchQuery::new(vec![1.0, 0.0, 0.0], 8);
    assert_eq!(
        index.search(&vector_query).unwrap(),
        index.search_in_candidates(&vector_query, &scope).unwrap()
    );

    let keyword_query = KeywordQuery::new("shared record", 8);
    assert_eq!(
        index.keyword_search(&keyword_query).unwrap(),
        index
            .keyword_search_in_candidates(&keyword_query, &scope)
            .unwrap()
    );

    let hybrid_query =
        HybridQuery::new("shared record", vec![1.0, 0.0, 0.0], 8).with_candidate_limits(16, 16);
    assert_eq!(
        index.hybrid_search(&hybrid_query).unwrap(),
        index
            .hybrid_search_in_candidates(&hybrid_query, &scope)
            .unwrap()
    );
}

#[test]
fn empty_sparse_and_dense_scopes_are_correct() {
    let (index, ids) = build_index("scope-shapes", 128);
    let query = SearchQuery::new(vec![1.0, 0.0, 0.0], 128);

    let empty = index.candidate_scope([]).unwrap();
    assert!(index
        .search_in_candidates(&query, &empty)
        .unwrap()
        .is_empty());

    let sparse_ids = vec![ids[1], ids[63], ids[127]];
    let sparse = index.candidate_scope(sparse_ids.clone()).unwrap();
    let sparse_hits = index.search_in_candidates(&query, &sparse).unwrap();
    let mut actual = hit_ids(&sparse_hits, |hit| hit.chunk_id);
    actual.sort_unstable();
    assert_eq!(actual, sparse_ids);

    let dense_ids = ids
        .iter()
        .copied()
        .filter(|id| id % 4 != 0)
        .collect::<Vec<_>>();
    let dense = index.candidate_scope(dense_ids.clone()).unwrap();
    let dense_hits = index.search_in_candidates(&query, &dense).unwrap();
    let mut actual = hit_ids(&dense_hits, |hit| hit.chunk_id);
    actual.sort_unstable();
    assert_eq!(actual, dense_ids);
}

#[test]
fn scope_and_metadata_filter_intersect_before_final_top_k() {
    let (index, ids) = build_index("filters", 12);
    let scope_ids = vec![ids[1], ids[2], ids[3], ids[4], ids[5], ids[6]];
    let scope = index.candidate_scope(scope_ids).unwrap();
    let filter = Filter::eq("group", MetadataValue::String("even".to_owned()));

    let vector = SearchQuery::new(vec![1.0, 0.0, 0.0], 10).with_filter(filter.clone());
    let vector_ids = hit_ids(
        &index.search_in_candidates(&vector, &scope).unwrap(),
        |hit| hit.chunk_id,
    );
    assert_eq!(vector_ids.len(), 3);
    assert!(vector_ids.iter().all(|id| id % 2 == 0));

    let keyword = KeywordQuery::new("shared", 10).with_filter(filter.clone());
    let keyword_ids = hit_ids(
        &index
            .keyword_search_in_candidates(&keyword, &scope)
            .unwrap(),
        |hit| hit.chunk_id,
    );
    assert_eq!(keyword_ids.len(), 3);
    assert!(keyword_ids.iter().all(|id| id % 2 == 0));

    let hybrid = HybridQuery::new("shared", vec![1.0, 0.0, 0.0], 10)
        .with_candidate_limits(10, 10)
        .with_filter(filter);
    let hybrid_ids = hit_ids(
        &index.hybrid_search_in_candidates(&hybrid, &scope).unwrap(),
        |hit| hit.chunk_id,
    );
    assert_eq!(hybrid_ids.len(), 3);
    assert!(hybrid_ids.iter().all(|id| id % 2 == 0));
}

#[test]
fn mutations_reject_stale_scopes_and_retired_ids() {
    let (mut index, ids) = build_index("lifecycle", 2);
    let old_id = ids[0];
    let scope = index.candidate_scope([old_id]).unwrap();

    let replacement_ids = index
        .upsert_document(
            Document {
                id: "record-0".to_owned(),
                text: "replacement".to_owned(),
                metadata: Metadata::new(),
            },
            vec![ChunkInput {
                text: "replacement shared".to_owned(),
                embedding: vec![99.0, 0.0, 0.0],
                metadata: Metadata::new(),
            }],
        )
        .unwrap();

    let error = index
        .search_in_candidates(&SearchQuery::new(vec![1.0, 0.0, 0.0], 1), &scope)
        .unwrap_err();
    assert!(matches!(error, VectorKitError::StaleGeneration { .. }));
    assert!(matches!(
        index.candidate_scope([old_id]).unwrap_err(),
        VectorKitError::InvalidCandidateScope { .. }
    ));
    assert!(index.hydrate_chunks(&[old_id])[0].is_none());

    let replacement_scope = index.candidate_scope(replacement_ids.clone()).unwrap();
    assert_eq!(index.delete_document("record-0"), 1);
    assert!(matches!(
        index
            .keyword_search_in_candidates(&KeywordQuery::new("replacement", 1), &replacement_scope)
            .unwrap_err(),
        VectorKitError::StaleGeneration { .. }
    ));
    assert!(index.hydrate_chunks(&replacement_ids)[0].is_none());
}

#[test]
fn scope_rejects_a_different_corpus_even_at_the_same_generation() {
    let (left, left_ids) = build_index("left", 2);
    let (right, _) = build_index("right", 2);
    let scope = left.candidate_scope(left_ids).unwrap();
    assert!(matches!(
        right
            .search_in_candidates(&SearchQuery::new(vec![1.0, 0.0, 0.0], 1), &scope)
            .unwrap_err(),
        VectorKitError::StaleGeneration { .. }
    ));
}

#[test]
fn bulk_hydration_preserves_order_duplicates_and_missing_slots() {
    let (index, ids) = build_index("hydration", 3);
    let hydrated = index.hydrate_chunks(&[ids[2], u64::MAX, ids[0], ids[2]]);
    assert_eq!(hydrated.len(), 4);
    assert_eq!(hydrated[0].unwrap().chunk_id, ids[2]);
    assert!(hydrated[1].is_none());
    assert_eq!(hydrated[2].unwrap().chunk_id, ids[0]);
    assert_eq!(hydrated[3].unwrap().chunk_id, ids[2]);
}

#[test]
fn persisted_index_preserves_scope_namespace_and_generation() {
    let (index, ids) = build_index("persisted-corpus", 4);
    let generation = index.generation();
    let directory = std::env::temp_dir().join(format!(
        "vectorkit-m1-scope-{}-{}",
        std::process::id(),
        generation.get()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    index.save_to_dir(&directory).unwrap();
    let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
    assert_eq!(loaded.corpus_id().as_str(), "persisted-corpus");
    assert_eq!(loaded.generation(), generation);
    let scope = loaded.candidate_scope(ids).unwrap();
    assert_eq!(scope.generation(), generation);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn record_first_upsert_rebinds_stable_chunk_identity_to_the_new_internal_id() {
    let config =
        IndexConfig::new(3, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
    let mut index =
        ExactVectorIndex::try_with_config_in_corpus(config, CorpusId::new("record-first").unwrap())
            .unwrap();
    let identity = ChunkIdentity::new(
        RecordId::new("note-1").unwrap(),
        ChunkKey::new("body").unwrap(),
    );

    let first = index
        .upsert_record(
            canonical_record("note-1", "first"),
            Metadata::new(),
            vec![record_chunk("body", "first shared", vec![1.0, 0.0, 0.0])],
        )
        .unwrap()[0];
    assert_eq!(index.chunk_id_for_identity(&identity), Some(first));
    assert_eq!(index.chunk_identity(first), Some(&identity));

    let second = index
        .upsert_record(
            canonical_record("note-1", "second"),
            Metadata::new(),
            vec![record_chunk("body", "second shared", vec![2.0, 0.0, 0.0])],
        )
        .unwrap()[0];
    assert_ne!(first, second);
    assert_eq!(index.chunk_id_for_identity(&identity), Some(second));
    assert!(index.chunk_identity(first).is_none());
    assert!(index.hydrate_chunks(&[first])[0].is_none());

    let scope = index
        .candidate_scope_for_identities([identity.clone()])
        .unwrap();
    let hits = index
        .search_in_candidates(&SearchQuery::new(vec![1.0, 0.0, 0.0], 1), &scope)
        .unwrap();
    assert_eq!(hits[0].chunk_id, second);
    assert_eq!(
        index
            .record(&RecordId::new("note-1").unwrap())
            .unwrap()
            .fields[&FieldName::new("title").unwrap()],
        RecordValue::String("second".to_owned())
    );
}

#[test]
fn duplicate_chunk_keys_fail_before_replacing_canonical_state() {
    let config =
        IndexConfig::new(3, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
    let mut index = ExactVectorIndex::try_with_config(config).unwrap();
    index
        .upsert_record(
            canonical_record("note-1", "first"),
            Metadata::new(),
            vec![record_chunk("body", "first", vec![1.0, 0.0, 0.0])],
        )
        .unwrap();
    let generation = index.generation();

    let error = index
        .upsert_record(
            canonical_record("note-1", "invalid replacement"),
            Metadata::new(),
            vec![
                record_chunk("duplicate", "one", vec![1.0, 0.0, 0.0]),
                record_chunk("duplicate", "two", vec![0.0, 1.0, 0.0]),
            ],
        )
        .unwrap_err();
    assert!(matches!(error, VectorKitError::InvalidIdentity { .. }));
    assert_eq!(index.generation(), generation);
    assert_eq!(index.active_chunk_count(), 1);
    assert_eq!(
        index
            .record(&RecordId::new("note-1").unwrap())
            .unwrap()
            .fields[&FieldName::new("title").unwrap()],
        RecordValue::String("first".to_owned())
    );
}

#[test]
fn canonical_records_and_identity_mapping_survive_persistence_and_delete_together() {
    let config =
        IndexConfig::new(3, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
    let mut index = ExactVectorIndex::try_with_config_in_corpus(
        config,
        CorpusId::new("record-persistence").unwrap(),
    )
    .unwrap();
    let record_id = RecordId::new("note-1").unwrap();
    let identity = ChunkIdentity::new(record_id.clone(), ChunkKey::new("body").unwrap());
    let chunk_id = index
        .upsert_record(
            canonical_record("note-1", "persisted"),
            Metadata::new(),
            vec![record_chunk(
                "body",
                "persisted shared",
                vec![1.0, 0.0, 0.0],
            )],
        )
        .unwrap()[0];
    let directory = std::env::temp_dir().join(format!(
        "vectorkit-record-state-{}-{}",
        std::process::id(),
        index.generation().get()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    let sizes = index.save_to_dir(&directory).unwrap();
    assert!(sizes.records_bytes > 0);

    let mut loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
    assert_eq!(loaded.chunk_id_for_identity(&identity), Some(chunk_id));
    let hydrated = loaded.hydrate_records(&[RecordId::new("missing").unwrap(), record_id.clone()]);
    assert!(hydrated[0].is_none());
    assert_eq!(hydrated[1].unwrap().id, record_id);

    assert_eq!(loaded.delete_record(&record_id), 1);
    assert!(loaded.record(&record_id).is_none());
    assert!(loaded.chunk_id_for_identity(&identity).is_none());
    assert!(loaded.hydrate_chunks(&[chunk_id])[0].is_none());
    std::fs::remove_dir_all(directory).unwrap();
}
