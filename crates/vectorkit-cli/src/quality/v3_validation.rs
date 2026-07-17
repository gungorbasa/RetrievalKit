use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::v3_canonical::{
    canonical_json, parse_canonical_json, parse_canonical_jsonl, sha256, validate_text_bytes,
};
use super::v3_population::{population_hash, verify_normative_fixture, Populations};
use super::v3_runs::{canonical_runs, quantization_policy, RunContext, RunIdentity};
use super::v3_schema::*;

const EXPECTED_PATHS: [(&str, &str); 15] = [
    ("chunking_manifest", "manifests/chunking.json"),
    ("corpus_embeddings_f32", "corpus-embeddings.f32.jsonl"),
    ("embedding_manifest", "manifests/embedding.json"),
    ("evidence_judgments", "evidence-judgments.jsonl"),
    ("exclusions", "exclusions.jsonl"),
    ("expected_paths", "expected-paths.jsonl"),
    (
        "graph_construction_manifest",
        "manifests/graph-construction.json",
    ),
    ("graph_schema", "graph-schema.json"),
    ("preprocessing_manifest", "manifests/preprocessing.json"),
    ("qrels", "qrels.tsv"),
    ("queries", "queries.jsonl"),
    ("query_embeddings_f32", "query-embeddings.f32.jsonl"),
    ("records", "records.jsonl"),
    ("seed_policy_manifest", "manifests/seed-policy.json"),
    ("split_manifest", "manifests/split.json"),
];

pub(super) const LABEL_PATHS: [&str; 3] = [
    "evidence-judgments.jsonl",
    "expected-paths.jsonl",
    "qrels.tsv",
];

type ManifestSpec<'a> = (&'a str, &'a [&'a str], &'a [&'a str], &'a [&'a str]);
type AliasSortKey = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

#[derive(Debug)]
pub(super) struct ValidatedCollection {
    pub root: PathBuf,
    pub collection: Collection,
    pub records: Vec<Record>,
    pub graph_schema: GraphSchema,
    pub queries: Vec<Query>,
    pub corpus_embeddings: Vec<CorpusEmbedding>,
    pub query_embeddings: Vec<QueryEmbedding>,
    pub qrels: Vec<Qrel>,
    pub evidence: Vec<EvidenceJudgment>,
    pub expected_paths: Vec<ExpectedPaths>,
    pub exclusions: Vec<Exclusion>,
    pub dimension: usize,
    pub populations: Populations,
    pub runs: Vec<RunIdentity>,
    pub bytes: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub(super) struct RankingInputValidation {
    pub validated: ValidatedCollection,
    pub opened_collection_files: Vec<String>,
}

/// Validate every input needed to execute rankings without opening test labels.
///
/// Label file identities are bound by the already-validated canonical
/// `collection.json`; their bytes are deliberately absent from `bytes` until
/// the scoring stage calls `validate`.
pub(super) fn validate_ranking_inputs(root: &Path) -> Result<RankingInputValidation, String> {
    verify_normative_fixture()?;
    let root = root.canonicalize().map_err(|error| {
        format!(
            "failed to resolve collection root '{}': {error}",
            root.display()
        )
    })?;
    validate_layout(&root)?;
    let collection_bytes = read(&root, "collection.json")?;
    let collection_value = parse_canonical_json(&root.join("collection.json"), &collection_bytes)?;
    let collection: Collection = from_value("collection.json", collection_value)?;
    validate_collection_header(&collection)?;

    let mut bytes = BTreeMap::new();
    for (_, relative) in EXPECTED_PATHS {
        if !LABEL_PATHS.contains(&relative) {
            bytes.insert(relative.to_owned(), read(&root, relative)?);
        }
    }
    validate_file_index_for_ranking(&collection, &bytes)?;

    let records = parse_rows::<Record>(
        "records.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.records),
            &bytes[&collection.paths.records],
        )?,
    )?;
    let graph_schema: GraphSchema = from_value(
        "graph-schema.json",
        parse_canonical_json(
            &root.join(&collection.paths.graph_schema),
            &bytes[&collection.paths.graph_schema],
        )?,
    )?;
    let queries = parse_rows::<Query>(
        "queries.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.queries),
            &bytes[&collection.paths.queries],
        )?,
    )?;
    let corpus_embeddings = parse_rows::<CorpusEmbedding>(
        "corpus-embeddings.f32.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.corpus_embeddings_f32),
            &bytes[&collection.paths.corpus_embeddings_f32],
        )?,
    )?;
    let query_embeddings = parse_rows::<QueryEmbedding>(
        "query-embeddings.f32.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.query_embeddings_f32),
            &bytes[&collection.paths.query_embeddings_f32],
        )?,
    )?;
    let exclusions = parse_rows::<Exclusion>(
        "exclusions.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.exclusions),
            &bytes[&collection.paths.exclusions],
        )?,
    )?;
    let manifests = load_manifests(&root, &collection, &bytes)?;
    validate_records(&records)?;
    validate_graph_schema(&graph_schema, &records)?;
    validate_queries(&queries, &collection, &graph_schema)?;
    validate_exclusions(&exclusions, &queries)?;
    if records.len() != collection.counts.records
        || records
            .iter()
            .map(|record| record.chunks.len())
            .sum::<usize>()
            != collection.counts.chunks
        || queries.len() != collection.counts.queries
        || exclusions.len() != collection.counts.exclusion_rows
    {
        return Err("ranking-input collection counts differ from collection.json".to_owned());
    }
    let source_inventory_sha256 = validate_manifests(&manifests, &bytes, Some(&collection))?;
    let dimension = embedding_dimension(&manifests["embedding"])?;
    validate_embeddings(
        &records,
        &queries,
        &corpus_embeddings,
        &query_embeddings,
        dimension,
    )?;
    let populations = Populations::derive(&queries, &exclusions)?;
    validate_seed_policy(
        &manifests["seed-policy"],
        &queries,
        &populations,
        &exclusions,
        false,
    )?;
    validate_split_manifest(
        &manifests["split"],
        &collection,
        &populations,
        &exclusions,
        &source_inventory_sha256,
    )?;
    let context = RunContext {
        graph_schema_sha256: sha256(&bytes[&collection.paths.graph_schema]),
        seed_policy_sha256: sha256(&bytes[&collection.paths.seed_policy_manifest]),
        implementation_revision: serde_json::json!({
            "binary_sha256":"cc57e402a8c92ff14601f6390c76b15d1b6a4598e219c8b58009c36e2daa4f97",
            "git_commit":"d145b76ef60b964dcf004516fc4b94b00147d7c7",
            "source_sha256":null
        }),
    };
    let runs = canonical_runs(&collection, &queries, &populations, &context)?;
    validate_run_preimages(&runs)?;
    let mut opened_collection_files = vec!["collection.json".to_owned()];
    opened_collection_files.extend(bytes.keys().cloned());
    opened_collection_files.sort();
    Ok(RankingInputValidation {
        validated: ValidatedCollection {
            root,
            collection,
            records,
            graph_schema,
            queries,
            corpus_embeddings,
            query_embeddings,
            qrels: Vec::new(),
            evidence: Vec::new(),
            expected_paths: Vec::new(),
            exclusions,
            dimension,
            populations,
            runs,
            bytes,
        },
        opened_collection_files,
    })
}

pub(super) fn validate(root: &Path) -> Result<ValidatedCollection, String> {
    verify_normative_fixture()?;
    let root = root.canonicalize().map_err(|error| {
        format!(
            "failed to resolve collection root '{}': {error}",
            root.display()
        )
    })?;
    validate_layout(&root)?;
    let collection_bytes = read(&root, "collection.json")?;
    let collection_value = parse_canonical_json(&root.join("collection.json"), &collection_bytes)?;
    let collection: Collection = from_value("collection.json", collection_value)?;
    validate_collection_header(&collection)?;

    let mut bytes = BTreeMap::new();
    for (_, relative) in EXPECTED_PATHS {
        bytes.insert(relative.to_owned(), read(&root, relative)?);
    }
    validate_file_index(&collection, &bytes)?;

    let records = parse_rows::<Record>(
        "records.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.records),
            &bytes[&collection.paths.records],
        )?,
    )?;
    let graph_schema: GraphSchema = from_value(
        "graph-schema.json",
        parse_canonical_json(
            &root.join(&collection.paths.graph_schema),
            &bytes[&collection.paths.graph_schema],
        )?,
    )?;
    let queries = parse_rows::<Query>(
        "queries.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.queries),
            &bytes[&collection.paths.queries],
        )?,
    )?;
    let corpus_embeddings = parse_rows::<CorpusEmbedding>(
        "corpus-embeddings.f32.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.corpus_embeddings_f32),
            &bytes[&collection.paths.corpus_embeddings_f32],
        )?,
    )?;
    let query_embeddings = parse_rows::<QueryEmbedding>(
        "query-embeddings.f32.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.query_embeddings_f32),
            &bytes[&collection.paths.query_embeddings_f32],
        )?,
    )?;
    let evidence = parse_rows::<EvidenceJudgment>(
        "evidence-judgments.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.evidence_judgments),
            &bytes[&collection.paths.evidence_judgments],
        )?,
    )?;
    let expected_paths = parse_rows::<ExpectedPaths>(
        "expected-paths.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.expected_paths),
            &bytes[&collection.paths.expected_paths],
        )?,
    )?;
    let exclusions = parse_rows::<Exclusion>(
        "exclusions.jsonl",
        parse_canonical_jsonl(
            &root.join(&collection.paths.exclusions),
            &bytes[&collection.paths.exclusions],
        )?,
    )?;
    let qrels = parse_qrels(
        &root.join(&collection.paths.qrels),
        &bytes[&collection.paths.qrels],
    )?;

    let manifests = load_manifests(&root, &collection, &bytes)?;
    validate_records(&records)?;
    validate_graph_schema(&graph_schema, &records)?;
    validate_queries(&queries, &collection, &graph_schema)?;
    validate_exclusions(&exclusions, &queries)?;
    validate_judgments(
        &records,
        &queries,
        &qrels,
        &evidence,
        &expected_paths,
        &exclusions,
        &graph_schema,
    )?;
    validate_counts(
        &collection,
        &records,
        &queries,
        &qrels,
        &evidence,
        &expected_paths,
        &exclusions,
    )?;
    let source_inventory_sha256 = validate_manifests(&manifests, &bytes, None)?;

    let dimension = embedding_dimension(&manifests["embedding"])?;
    validate_embeddings(
        &records,
        &queries,
        &corpus_embeddings,
        &query_embeddings,
        dimension,
    )?;
    let populations = Populations::derive(&queries, &exclusions)?;
    validate_seed_policy(
        &manifests["seed-policy"],
        &queries,
        &populations,
        &exclusions,
        !matches!(
            collection.collection_id.as_str(),
            "hotpotqa-linked-abstracts-graph-v1-development"
                | "hotpotqa-linked-abstracts-graph-v1-test"
        ),
    )?;
    validate_split_manifest(
        &manifests["split"],
        &collection,
        &populations,
        &exclusions,
        &source_inventory_sha256,
    )?;
    let context = RunContext {
        graph_schema_sha256: sha256(&bytes[&collection.paths.graph_schema]),
        seed_policy_sha256: sha256(&bytes[&collection.paths.seed_policy_manifest]),
        implementation_revision: serde_json::json!({
            "binary_sha256":"cc57e402a8c92ff14601f6390c76b15d1b6a4598e219c8b58009c36e2daa4f97",
            "git_commit":"d145b76ef60b964dcf004516fc4b94b00147d7c7",
            "source_sha256":null
        }),
    };
    let runs = canonical_runs(&collection, &queries, &populations, &context)?;
    validate_run_preimages(&runs)?;
    Ok(ValidatedCollection {
        root,
        collection,
        records,
        graph_schema,
        queries,
        corpus_embeddings,
        query_embeddings,
        qrels,
        evidence,
        expected_paths,
        exclusions,
        dimension,
        populations,
        runs,
        bytes,
    })
}

fn validate_layout(root: &Path) -> Result<(), String> {
    let allowed_files = std::iter::once("collection.json".to_owned())
        .chain(EXPECTED_PATHS.iter().map(|(_, path)| (*path).to_owned()))
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    collect_files(root, root, &mut actual)?;
    actual.remove("README.md");
    let unexpected = actual.difference(&allowed_files).collect::<Vec<_>>();
    let missing = allowed_files.difference(&actual).collect::<Vec<_>>();
    if !unexpected.is_empty() || !missing.is_empty() {
        return Err(format!(
            "collection layout expected only section 3.1 files; missing {:?}, unexpected {:?}",
            missing, unexpected
        ));
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("failed to read '{}': {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        let kind = entry
            .file_type()
            .map_err(|error| format!("failed to inspect '{}': {error}", entry.path().display()))?;
        if kind.is_symlink() {
            return Err(format!(
                "{}: symlinks are forbidden",
                entry.path().display()
            ));
        }
        if kind.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else if kind.is_file() {
            let path = entry.path();
            let relative = path.strip_prefix(root).expect("entry is under root");
            let relative = relative
                .to_str()
                .ok_or_else(|| format!("{}: path is not UTF-8", relative.display()))?;
            files.insert(relative.to_owned());
        } else {
            return Err(format!(
                "{}: expected a regular file",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn validate_collection_header(collection: &Collection) -> Result<(), String> {
    if collection.schema_version != 3 {
        return Err(format!(
            "collection.json: field 'schema_version' expected 3, actual {}",
            collection.schema_version
        ));
    }
    for (field, value) in [
        ("collection_id", collection.collection_id.as_str()),
        ("collection_version", collection.collection_version.as_str()),
        ("corpus_id", collection.corpus_id.as_str()),
    ] {
        validate_eval_id("collection.json", field, value)?;
    }
    if !matches!(collection.split.as_str(), "development" | "test") {
        return Err(format!(
            "collection.json: field 'split' expected development or test, actual '{}'",
            collection.split
        ));
    }
    if collection.top_k == 0
        || collection.evaluation_depth < 10
        || collection.evaluation_depth < collection.top_k
    {
        return Err(format!(
            "collection.json: fields 'top_k'/'evaluation_depth' violate positive depth rule; actual {}/{}",
            collection.top_k, collection.evaluation_depth
        ));
    }
    if collection.relevance_threshold != 1 {
        return Err(format!(
            "collection.json: field 'relevance_threshold' expected 1, actual {}",
            collection.relevance_threshold
        ));
    }
    for ((field, actual), (expected_field, expected)) in
        collection.paths.entries().into_iter().zip(EXPECTED_PATHS)
    {
        if field != expected_field || actual != expected {
            return Err(format!(
                "collection.json: field 'paths.{field}' expected '{expected}', actual '{actual}'"
            ));
        }
        validate_relative_path("collection.json", &format!("paths.{field}"), actual)?;
    }
    Ok(())
}

fn validate_file_index(
    collection: &Collection,
    bytes: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let expected_paths = EXPECTED_PATHS
        .iter()
        .map(|(_, path)| *path)
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&str> = None;
    let mut actual_paths = BTreeSet::new();
    for entry in &collection.files {
        validate_relative_path("collection.json", "files[].path", &entry.path)?;
        validate_sha("collection.json", "files[].sha256", &entry.sha256)?;
        if previous.is_some_and(|value| value.as_bytes() >= entry.path.as_bytes()) {
            return Err(format!(
                "collection.json: field 'files' expected strict lexical path order, actual '{}' after '{}'",
                entry.path,
                previous.unwrap()
            ));
        }
        previous = Some(&entry.path);
        if !actual_paths.insert(entry.path.as_str()) {
            return Err(format!(
                "collection.json: duplicate files path '{}'",
                entry.path
            ));
        }
        let actual = bytes.get(&entry.path).ok_or_else(|| {
            format!(
                "collection.json: files path '{}' is not a required input",
                entry.path
            )
        })?;
        if entry.bytes != actual.len() as u64 {
            return Err(format!(
                "collection.json: file '{}' expected bytes {}, actual {}",
                entry.path,
                entry.bytes,
                actual.len()
            ));
        }
        let digest = sha256(actual);
        if entry.sha256 != digest {
            return Err(format!(
                "collection.json: file '{}' expected sha256 {}, actual {}",
                entry.path, entry.sha256, digest
            ));
        }
    }
    if actual_paths != expected_paths {
        return Err(format!(
            "collection.json: field 'files' path set expected {:?}, actual {:?}",
            expected_paths, actual_paths
        ));
    }
    Ok(())
}

fn validate_file_index_for_ranking(
    collection: &Collection,
    bytes: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let indexed = collection
        .files
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let expected = EXPECTED_PATHS
        .iter()
        .map(|(_, path)| *path)
        .collect::<BTreeSet<_>>();
    if indexed.keys().copied().collect::<BTreeSet<_>>() != expected {
        return Err("collection.json: ranking input file inventory changed".to_owned());
    }
    for (path, actual) in bytes {
        let entry = indexed[path.as_str()];
        let digest = sha256(actual);
        if entry.bytes != actual.len() as u64 || entry.sha256 != digest {
            return Err(format!(
                "collection.json: ranking input '{path}' checksum or byte count mismatch"
            ));
        }
    }
    for path in LABEL_PATHS {
        let entry = indexed[path];
        validate_sha("collection.json", "files[].sha256", &entry.sha256)?;
    }
    Ok(())
}

fn validate_records(records: &[Record]) -> Result<(), String> {
    let mut previous: Option<&str> = None;
    let mut ids = BTreeSet::new();
    for record in records {
        let _ = &record.content;
        validate_eval_id("records.jsonl", "record_id", &record.record_id)?;
        validate_production_id("records.jsonl", "record_type", &record.record_type)?;
        if previous.is_some_and(|value| value.as_bytes() >= record.record_id.as_bytes()) {
            return Err(format!(
                "records.jsonl: incorrect record order at '{}'; expected strict lexical order after '{}'",
                record.record_id,
                previous.unwrap()
            ));
        }
        previous = Some(&record.record_id);
        if !ids.insert(record.record_id.as_str()) {
            return Err(format!(
                "records.jsonl: duplicate record_id '{}'",
                record.record_id
            ));
        }
        if record.chunks.is_empty() {
            return Err(format!(
                "records.jsonl: record '{}' field 'chunks' expected non-empty array, actual empty",
                record.record_id
            ));
        }
        for (field, value) in &record.fields {
            validate_production_id("records.jsonl", "fields key", field)?;
            validate_tagged_value(
                "records.jsonl",
                &format!("record '{}'.fields.{field}", record.record_id),
                value,
            )?;
        }
        validate_metadata("records.jsonl", &record.metadata)?;
        let mut chunk_previous: Option<&str> = None;
        let mut chunks = BTreeSet::new();
        for chunk in &record.chunks {
            validate_eval_id("records.jsonl", "chunk_key", &chunk.chunk_key)?;
            if chunk.text.is_empty() {
                return Err(format!(
                    "records.jsonl: record '{}', chunk '{}': field 'text' expected non-empty string",
                    record.record_id, chunk.chunk_key
                ));
            }
            if chunk_previous.is_some_and(|value| value.as_bytes() >= chunk.chunk_key.as_bytes()) {
                return Err(format!(
                    "records.jsonl: record '{}': chunks expected strict lexical order; actual '{}' after '{}'",
                    record.record_id,
                    chunk.chunk_key,
                    chunk_previous.unwrap()
                ));
            }
            chunk_previous = Some(&chunk.chunk_key);
            if !chunks.insert(chunk.chunk_key.as_str()) {
                return Err(format!(
                    "records.jsonl: record '{}': duplicate chunk_key '{}'",
                    record.record_id, chunk.chunk_key
                ));
            }
            validate_metadata("records.jsonl", &chunk.metadata)?;
        }
    }
    Ok(())
}

fn validate_graph_schema(schema: &GraphSchema, records: &[Record]) -> Result<(), String> {
    if schema.version != 1 {
        return Err(format!(
            "graph-schema.json: field 'version' expected 1, actual {}",
            schema.version
        ));
    }
    let record_types = records
        .iter()
        .map(|record| record.record_type.as_str())
        .collect::<BTreeSet<_>>();
    let mut node_types = BTreeSet::new();
    for rule in &schema.record_nodes {
        validate_production_id(
            "graph-schema.json",
            "record_nodes[].record_type",
            &rule.record_type,
        )?;
        validate_production_id(
            "graph-schema.json",
            "record_nodes[].node_type",
            &rule.node_type,
        )?;
        if !record_types.contains(rule.record_type.as_str()) {
            return Err(format!(
                "graph-schema.json: record node references unused record_type '{}'",
                rule.record_type
            ));
        }
        if !node_types.insert(rule.node_type.as_str()) {
            return Err(format!(
                "graph-schema.json: duplicate node_type '{}'",
                rule.node_type
            ));
        }
        if rule.queryable_fields.is_empty() {
            return Err("graph-schema.json: queryable_fields expected non-empty array".to_owned());
        }
        for path in &rule.queryable_fields {
            validate_field_path("graph-schema.json", "queryable_fields[]", path)?;
        }
    }
    let mut relationships = BTreeSet::new();
    for rule in &schema.relationships {
        let _ = rule.allow_self_edge;
        validate_production_id(
            "graph-schema.json",
            "relationship_type",
            &rule.relationship_type,
        )?;
        if !relationships.insert(rule.relationship_type.as_str()) {
            return Err(format!(
                "graph-schema.json: duplicate relationship_type '{}'",
                rule.relationship_type
            ));
        }
        if !node_types.contains(rule.source_node_type.as_str())
            || !node_types.contains(rule.target_node_type.as_str())
        {
            return Err(format!(
                "graph-schema.json: relationship '{}' references unknown node type",
                rule.relationship_type
            ));
        }
        validate_field_path("graph-schema.json", "source_field", &rule.source_field)?;
        if !matches!(rule.cardinality.as_str(), "one" | "optional_one" | "many") {
            return Err(format!(
                "graph-schema.json: field 'cardinality' expected one/optional_one/many, actual '{}'",
                rule.cardinality
            ));
        }
        if !matches!(rule.missing_target.as_str(), "error" | "omit_edge") {
            return Err(format!(
                "graph-schema.json: invalid missing_target '{}'",
                rule.missing_target
            ));
        }
        if !matches!(rule.duplicate_references.as_str(), "error" | "deduplicate") {
            return Err(format!(
                "graph-schema.json: invalid duplicate_references '{}'",
                rule.duplicate_references
            ));
        }
        if let Some(inverse) = &rule.inverse_relationship {
            validate_production_id("graph-schema.json", "inverse_relationship", inverse)?;
        }
    }
    if let Some(chunk) = &schema.chunk_nodes {
        validate_production_id(
            "graph-schema.json",
            "chunk_nodes.node_type",
            &chunk.node_type,
        )?;
        validate_production_id(
            "graph-schema.json",
            "chunk_nodes.owns_relationship",
            &chunk.owns_relationship,
        )?;
        if let Some(inverse) = &chunk.inverse_relationship {
            validate_production_id(
                "graph-schema.json",
                "chunk_nodes.inverse_relationship",
                inverse,
            )?;
        }
    }
    Ok(())
}

fn validate_queries(
    queries: &[Query],
    collection: &Collection,
    schema: &GraphSchema,
) -> Result<(), String> {
    let relationships = schema
        .relationships
        .iter()
        .map(|rule| rule.relationship_type.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&str> = None;
    let mut ids = BTreeSet::new();
    for query in queries {
        validate_eval_id("queries.jsonl", "query_id", &query.query_id)?;
        if previous.is_some_and(|value| value.as_bytes() >= query.query_id.as_bytes()) {
            return Err(format!(
                "queries.jsonl: incorrect query order at '{}'; expected strict lexical order after '{}'",
                query.query_id,
                previous.unwrap()
            ));
        }
        previous = Some(&query.query_id);
        if !ids.insert(query.query_id.as_str()) {
            return Err(format!(
                "queries.jsonl: duplicate query_id '{}'",
                query.query_id
            ));
        }
        if query.split != collection.split {
            return Err(format!(
                "queries.jsonl: query '{}' field 'split' expected '{}', actual '{}'",
                query.query_id, collection.split, query.split
            ));
        }
        if query.category.is_empty() || query.text.is_empty() {
            return Err(format!(
                "queries.jsonl: query '{}': category and text must be non-empty",
                query.query_id
            ));
        }
        validate_sorted_unique_strings("queries.jsonl", "tasks", &query.tasks)?;
        if query.tasks.is_empty()
            || query
                .tasks
                .iter()
                .any(|task| !matches!(task.as_str(), "retrieval" | "evidence" | "path"))
        {
            return Err(format!(
                "queries.jsonl: query '{}': tasks expected non-empty subset of evidence/path/retrieval, actual {:?}",
                query.query_id, query.tasks
            ));
        }
        if query
            .tasks
            .iter()
            .any(|task| matches!(task.as_str(), "evidence" | "path"))
            && query.explicit_seed.is_none()
            && query.derived_seed_policy_id.is_none()
        {
            return Err(format!(
                "queries.jsonl: query '{}': evidence/path task requires an explicit or derived seed",
                query.query_id
            ));
        }
        if let Some(policy) = &query.derived_seed_policy_id {
            validate_derived_policy_id("queries.jsonl", policy)?;
        }
        if let Some(seed) = &query.explicit_seed {
            validate_seed("queries.jsonl", "explicit_seed", seed)?;
        }
        if let Some(filter) = &query.metadata_filter {
            validate_filter("queries.jsonl", "metadata_filter", filter, 1)?;
        }
        if query.traversal.limits.max_hops == 0
            || query.traversal.limits.max_visited == 0
            || query.traversal.limits.max_results == 0
            || query.traversal.limits.max_working_bytes == 0
        {
            return Err(format!(
                "queries.jsonl: query '{}': traversal limits must all be positive",
                query.query_id
            ));
        }
        for step in &query.traversal.steps {
            if !relationships.contains(step.relationship_type.as_str()) {
                return Err(format!(
                    "queries.jsonl: query '{}': traversal references unknown relationship_type '{}'",
                    query.query_id, step.relationship_type
                ));
            }
            if !matches!(step.direction.as_str(), "outgoing" | "incoming")
                || step.min_hops > step.max_hops
            {
                return Err(format!(
                    "queries.jsonl: query '{}': invalid traversal step bounds/direction",
                    query.query_id
                ));
            }
        }
    }
    if !queries
        .iter()
        .any(|query| query.tasks.iter().any(|task| task == "retrieval"))
    {
        return Err(
            "queries.jsonl: collection must contain at least one retrieval query".to_owned(),
        );
    }
    Ok(())
}

fn validate_exclusions(exclusions: &[Exclusion], queries: &[Query]) -> Result<(), String> {
    let by_id = queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut previous: Option<(String, String, String, String, String)> = None;
    let mut pairs = BTreeSet::new();
    let global_reasons = [
        "not_in_frozen_corpus",
        "missing_complete_evidence",
        "invalid_upstream_record",
        "duplicate_identity",
        "filter_label_conflict",
        "no_relevant_documents",
    ];
    for exclusion in exclusions {
        validate_eval_id("exclusions.jsonl", "query_id", &exclusion.query_id)?;
        if exclusion.phase != "pre_freeze"
            || exclusion.details.is_empty()
            || exclusion.source.is_empty()
        {
            return Err(format!(
                "exclusions.jsonl: query '{}': phase must be pre_freeze and details/source non-empty",
                exclusion.query_id
            ));
        }
        let key = (
            exclusion.query_id.clone(),
            exclusion.lane.clone(),
            exclusion.phase.clone(),
            exclusion.reason.clone(),
            exclusion.source.clone(),
        );
        if previous.as_ref().is_some_and(|value| value >= &key) {
            return Err(format!(
                "exclusions.jsonl: incorrect row order at query '{}', lane '{}'",
                exclusion.query_id, exclusion.lane
            ));
        }
        previous = Some(key);
        if !pairs.insert((exclusion.query_id.as_str(), exclusion.lane.as_str())) {
            return Err(format!(
                "exclusions.jsonl: duplicate exclusion for query '{}', lane '{}'",
                exclusion.query_id, exclusion.lane
            ));
        }
        if global_reasons.contains(&exclusion.reason.as_str()) {
            if exclusion.lane != "global" || by_id.contains_key(exclusion.query_id.as_str()) {
                return Err(format!(
                    "exclusions.jsonl: global reason '{}' requires lane global and query absent from queries.jsonl; actual query '{}', lane '{}'",
                    exclusion.reason, exclusion.query_id, exclusion.lane
                ));
            }
        } else if matches!(
            exclusion.reason.as_str(),
            "derived_seed_no_match" | "derived_seed_ambiguous"
        ) {
            let query = by_id.get(exclusion.query_id.as_str()).ok_or_else(|| {
                format!(
                    "exclusions.jsonl: derived exclusion references missing query '{}'",
                    exclusion.query_id
                )
            })?;
            if query.derived_seed_policy_id.as_deref() != Some(exclusion.lane.as_str())
                || exclusion.lane == "explicit"
                || exclusion.lane == "global"
            {
                return Err(format!(
                    "exclusions.jsonl: illegal global versus lane exclusion for query '{}': policy {:?}, lane '{}'",
                    exclusion.query_id, query.derived_seed_policy_id, exclusion.lane
                ));
            }
        } else {
            return Err(format!(
                "exclusions.jsonl: query '{}': unknown reason '{}'",
                exclusion.query_id, exclusion.reason
            ));
        }
    }
    Ok(())
}

fn validate_judgments(
    records: &[Record],
    queries: &[Query],
    qrels: &[Qrel],
    evidence: &[EvidenceJudgment],
    expected: &[ExpectedPaths],
    exclusions: &[Exclusion],
    schema: &GraphSchema,
) -> Result<(), String> {
    let record_ids = records
        .iter()
        .map(|record| record.record_id.as_str())
        .collect::<BTreeSet<_>>();
    let records_by_id = records
        .iter()
        .map(|record| (record.record_id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let query_by_id = queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let global = exclusions
        .iter()
        .filter(|row| row.lane == "global")
        .map(|row| row.query_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut positive = BTreeSet::new();
    for qrel in qrels {
        let query = query_by_id.get(qrel.query_id.as_str()).ok_or_else(|| {
            format!(
                "qrels.tsv: invalid qrel reference to query '{}'",
                qrel.query_id
            )
        })?;
        if !query.tasks.iter().any(|task| task == "retrieval") {
            return Err(format!(
                "qrels.tsv: query '{}' does not declare retrieval",
                qrel.query_id
            ));
        }
        if !record_ids.contains(qrel.record_id.as_str()) {
            return Err(format!(
                "qrels.tsv: invalid qrel reference to record '{}'",
                qrel.record_id
            ));
        }
        if qrel.relevance >= 1 {
            if !document_satisfies_filter(
                records_by_id[qrel.record_id.as_str()],
                query.metadata_filter.as_ref(),
            )? {
                return Err(format!(
                    "qrels.tsv: positive record '{}' does not satisfy query '{}' metadata filter",
                    qrel.record_id, qrel.query_id
                ));
            }
            positive.insert(qrel.query_id.as_str());
        }
    }
    for query in queries
        .iter()
        .filter(|query| query.tasks.iter().any(|task| task == "retrieval"))
    {
        if !positive.contains(query.query_id.as_str()) {
            return Err(format!(
                "qrels.tsv: retrieval query '{}' has no positive judgment",
                query.query_id
            ));
        }
    }
    if qrels
        .iter()
        .any(|qrel| global.contains(qrel.query_id.as_str()))
    {
        return Err("qrels.tsv: globally excluded query must not have qrels".to_owned());
    }
    let mut evidence_ids = BTreeSet::new();
    for row in evidence {
        let query = query_by_id
            .get(row.query_id.as_str())
            .ok_or_else(|| format!("evidence-judgments.jsonl: unknown query '{}'", row.query_id))?;
        if !evidence_ids.insert(row.query_id.as_str()) {
            return Err(format!(
                "evidence-judgments.jsonl: duplicate query_id '{}'",
                row.query_id
            ));
        }
        if !query.tasks.iter().any(|task| task == "evidence") || row.evidence_sets.is_empty() {
            return Err(format!(
                "evidence-judgments.jsonl: incomplete evidence task for query '{}'",
                row.query_id
            ));
        }
        validate_sorted_unique_arrays(
            "evidence-judgments.jsonl",
            "evidence_sets",
            &row.evidence_sets,
        )?;
        for set in &row.evidence_sets {
            if set.is_empty()
                || set
                    .iter()
                    .any(|record| !record_ids.contains(record.as_str()))
            {
                return Err(format!(
                    "evidence-judgments.jsonl: query '{}': evidence set is empty or references unknown record",
                    row.query_id
                ));
            }
            for record_id in set {
                if !document_satisfies_filter(
                    records_by_id[record_id.as_str()],
                    query.metadata_filter.as_ref(),
                )? {
                    return Err(format!(
                        "evidence-judgments.jsonl: supporting record '{}' does not satisfy query '{}' metadata filter",
                        record_id, row.query_id
                    ));
                }
            }
        }
    }
    for query in queries {
        let declares = query.tasks.iter().any(|task| task == "evidence");
        if declares != evidence_ids.contains(query.query_id.as_str()) {
            return Err(format!(
                "evidence-judgments.jsonl: incomplete evidence task for query '{}': declares {}, row present {}",
                query.query_id,
                declares,
                evidence_ids.contains(query.query_id.as_str())
            ));
        }
    }
    let relationships = schema
        .relationships
        .iter()
        .map(|rule| rule.relationship_type.as_str())
        .collect::<BTreeSet<_>>();
    let mut path_keys = BTreeSet::new();
    let mut previous: Option<(&str, &str)> = None;
    for row in expected {
        let query = query_by_id
            .get(row.query_id.as_str())
            .ok_or_else(|| format!("expected-paths.jsonl: unknown query '{}'", row.query_id))?;
        let key = (row.query_id.as_str(), row.seed_policy.as_str());
        if previous.is_some_and(|value| value >= key) {
            return Err(format!(
                "expected-paths.jsonl: incorrect row order at ({}, {})",
                row.query_id, row.seed_policy
            ));
        }
        previous = Some(key);
        if !path_keys.insert(key)
            || !query.tasks.iter().any(|task| task == "path")
            || row.expected_paths.is_empty()
        {
            return Err(format!(
                "expected-paths.jsonl: invalid path row for query '{}', lane '{}'",
                row.query_id, row.seed_policy
            ));
        }
        let lane_valid = row.seed_policy == "explicit" && query.explicit_seed.is_some()
            || query.derived_seed_policy_id.as_deref() == Some(row.seed_policy.as_str());
        if !lane_valid {
            return Err(format!(
                "expected-paths.jsonl: query '{}' does not supply seed lane '{}'",
                row.query_id, row.seed_policy
            ));
        }
        let encoded = row
            .expected_paths
            .iter()
            .map(|path| canonical_json(&serde_json::to_value(path).expect("path serializes")))
            .collect::<Result<Vec<_>, _>>()?;
        validate_sorted_unique_encoded("expected-paths.jsonl", "expected_paths", &encoded)?;
        for path in &row.expected_paths {
            for edge in path {
                if !matches!(edge.direction.as_str(), "outgoing" | "incoming")
                    || !relationships.contains(edge.relationship_type.as_str())
                {
                    return Err(format!(
                        "expected-paths.jsonl: query '{}': invalid relationship/direction '{}'/'{}'",
                        row.query_id, edge.relationship_type, edge.direction
                    ));
                }
                validate_node_reference(
                    "expected-paths.jsonl",
                    &edge.source_node,
                    &record_ids,
                    records,
                )?;
                validate_node_reference(
                    "expected-paths.jsonl",
                    &edge.target_node,
                    &record_ids,
                    records,
                )?;
            }
        }
    }
    Ok(())
}

fn document_satisfies_filter(record: &Record, filter: Option<&Value>) -> Result<bool, String> {
    let Some(filter) = filter else {
        return Ok(true);
    };
    for chunk in &record.chunks {
        let mut metadata = record.metadata.clone();
        metadata.extend(chunk.metadata.clone());
        if evaluate_filter(filter, &metadata)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn evaluate_filter(filter: &Value, metadata: &BTreeMap<String, Value>) -> Result<bool, String> {
    let object = filter
        .as_object()
        .ok_or_else(|| "queries.jsonl: validated filter was not an object".to_owned())?;
    let op = object["op"].as_str().unwrap();
    let field_value = || {
        object["field"]
            .as_str()
            .and_then(|field| metadata.get(field))
    };
    match op {
        "equals" => Ok(field_value() == Some(&object["value"])),
        "not_equals" => Ok(field_value().is_some_and(|value| value != &object["value"])),
        "in" => Ok(field_value().is_some_and(|value| {
            object["values"]
                .as_array()
                .unwrap()
                .iter()
                .any(|candidate| candidate == value)
        })),
        "range" => {
            let Some(value) = field_value() else {
                return Ok(false);
            };
            let lower = &object["lower"];
            let upper = &object["upper"];
            Ok((lower.is_null()
                || compare_metadata_number(value, lower)? >= std::cmp::Ordering::Equal)
                && (upper.is_null()
                    || compare_metadata_number(value, upper)? <= std::cmp::Ordering::Equal))
        }
        "exists" => Ok(field_value().is_some()),
        "all" => {
            for child in object["children"].as_array().unwrap() {
                if !evaluate_filter(child, metadata)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        "any" => {
            for child in object["children"].as_array().unwrap() {
                if evaluate_filter(child, metadata)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        _ => Err(format!(
            "queries.jsonl: unsupported validated filter op '{op}'"
        )),
    }
}

fn compare_metadata_number(left: &Value, right: &Value) -> Result<std::cmp::Ordering, String> {
    let tag = left["type"].as_str();
    if tag != right["type"].as_str() {
        return Err("queries.jsonl: range operand types do not match corpus metadata".to_owned());
    }
    match tag {
        Some("integer" | "timestamp_millis") => Ok(left["value"]
            .as_i64()
            .unwrap()
            .cmp(&right["value"].as_i64().unwrap())),
        Some("float") => left["value"]
            .as_f64()
            .unwrap()
            .partial_cmp(&right["value"].as_f64().unwrap())
            .ok_or_else(|| "queries.jsonl: non-finite metadata comparison".to_owned()),
        _ => Err("queries.jsonl: range requires numeric corpus metadata".to_owned()),
    }
}

fn validate_embeddings(
    records: &[Record],
    queries: &[Query],
    corpus: &[CorpusEmbedding],
    query_embeddings: &[QueryEmbedding],
    dimension: usize,
) -> Result<(), String> {
    let expected_corpus = records
        .iter()
        .flat_map(|record| {
            record
                .chunks
                .iter()
                .map(move |chunk| (record.record_id.clone(), chunk.chunk_key.clone()))
        })
        .collect::<BTreeSet<_>>();
    let actual_corpus = corpus
        .iter()
        .map(|row| (row.record_id.clone(), row.chunk_key.clone()))
        .collect::<BTreeSet<_>>();
    if corpus.len() != actual_corpus.len() {
        return Err("corpus-embeddings.f32.jsonl: duplicate embedding key".to_owned());
    }
    if actual_corpus != expected_corpus {
        return Err(format!(
            "corpus-embeddings.f32.jsonl: missing or unexpected embeddings; expected {:?}, actual {:?}",
            expected_corpus, actual_corpus
        ));
    }
    let corpus_order = corpus
        .iter()
        .map(|row| (row.record_id.as_str(), row.chunk_key.as_str()))
        .collect::<Vec<_>>();
    if !corpus_order.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err("corpus-embeddings.f32.jsonl: incorrect embedding order".to_owned());
    }
    let expected_queries = queries
        .iter()
        .filter(|query| query.tasks.iter().any(|task| task == "retrieval"))
        .map(|query| query.query_id.clone())
        .collect::<BTreeSet<_>>();
    let actual_queries = query_embeddings
        .iter()
        .map(|row| row.query_id.clone())
        .collect::<BTreeSet<_>>();
    if query_embeddings.len() != actual_queries.len() || actual_queries != expected_queries {
        return Err(format!(
            "query-embeddings.f32.jsonl: missing or unexpected embeddings; expected {:?}, actual {:?}",
            expected_queries, actual_queries
        ));
    }
    if !query_embeddings
        .windows(2)
        .all(|pair| pair[0].query_id.as_bytes() < pair[1].query_id.as_bytes())
    {
        return Err("query-embeddings.f32.jsonl: incorrect embedding order".to_owned());
    }
    for (label, id, values) in corpus
        .iter()
        .map(|row| {
            (
                "corpus-embeddings.f32.jsonl",
                format!("{}/{}", row.record_id, row.chunk_key),
                &row.values,
            )
        })
        .chain(query_embeddings.iter().map(|row| {
            (
                "query-embeddings.f32.jsonl",
                row.query_id.clone(),
                &row.values,
            )
        }))
    {
        if values.len() != dimension || values.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "{label}: embedding '{id}' expected finite dimension {dimension}, actual dimension {}",
                values.len()
            ));
        }
    }
    Ok(())
}

fn validate_counts(
    collection: &Collection,
    records: &[Record],
    queries: &[Query],
    qrels: &[Qrel],
    evidence: &[EvidenceJudgment],
    expected: &[ExpectedPaths],
    exclusions: &[Exclusion],
) -> Result<(), String> {
    let actual = CollectionCounts {
        records: records.len(),
        chunks: records.iter().map(|record| record.chunks.len()).sum(),
        queries: queries.len(),
        qrel_rows: qrels.len(),
        evidence_rows: evidence.len(),
        expected_path_rows: expected.len(),
        exclusion_rows: exclusions.len(),
    };
    for (field, expected, actual) in [
        ("records", collection.counts.records, actual.records),
        ("chunks", collection.counts.chunks, actual.chunks),
        ("queries", collection.counts.queries, actual.queries),
        ("qrel_rows", collection.counts.qrel_rows, actual.qrel_rows),
        (
            "evidence_rows",
            collection.counts.evidence_rows,
            actual.evidence_rows,
        ),
        (
            "expected_path_rows",
            collection.counts.expected_path_rows,
            actual.expected_path_rows,
        ),
        (
            "exclusion_rows",
            collection.counts.exclusion_rows,
            actual.exclusion_rows,
        ),
    ] {
        if expected != actual {
            return Err(format!(
                "collection.json: counts.{field} expected {expected}, actual {actual}"
            ));
        }
    }
    Ok(())
}

fn load_manifests(
    root: &Path,
    collection: &Collection,
    bytes: &BTreeMap<String, Vec<u8>>,
) -> Result<BTreeMap<String, TransformationManifest>, String> {
    let paths = [
        ("preprocessing", &collection.paths.preprocessing_manifest),
        ("chunking", &collection.paths.chunking_manifest),
        ("embedding", &collection.paths.embedding_manifest),
        (
            "graph-construction",
            &collection.paths.graph_construction_manifest,
        ),
        ("seed-policy", &collection.paths.seed_policy_manifest),
        ("split", &collection.paths.split_manifest),
    ];
    let mut manifests = BTreeMap::new();
    for (name, path) in paths {
        let value = parse_canonical_json(&root.join(path), &bytes[path])?;
        manifests.insert(name.to_owned(), from_value(path, value)?);
    }
    Ok(manifests)
}

fn validate_manifests(
    manifests: &BTreeMap<String, TransformationManifest>,
    bytes: &BTreeMap<String, Vec<u8>>,
    ranking_collection: Option<&Collection>,
) -> Result<String, String> {
    let specs: [ManifestSpec<'_>; 6] = [
        ("preprocessing", &[], &["upstream/corpus/"], &[]),
        ("chunking", &[], &["upstream/corpus/"], &["records.jsonl"]),
        (
            "graph-construction",
            &["records.jsonl"],
            &["upstream/graph/"],
            &["graph-schema.json"],
        ),
        (
            "split",
            &["graph-schema.json", "records.jsonl"],
            &[
                "upstream/judgment/",
                "upstream/license/",
                "upstream/query/",
                "upstream/scenario/",
            ],
            &[
                "evidence-judgments.jsonl",
                "exclusions.jsonl",
                "expected-paths.jsonl",
                "qrels.tsv",
                "queries.jsonl",
            ],
        ),
        (
            "seed-policy",
            &[
                "exclusions.jsonl",
                "graph-schema.json",
                "queries.jsonl",
                "records.jsonl",
            ],
            &["upstream/scenario/"],
            &[],
        ),
        (
            "embedding",
            &["queries.jsonl", "records.jsonl"],
            &["upstream/model/", "upstream/tokenizer/"],
            &["corpus-embeddings.f32.jsonl", "query-embeddings.f32.jsonl"],
        ),
    ];
    let parameter_keys: BTreeMap<&str, &[&str]> = BTreeMap::from([
        (
            "preprocessing",
            &[
                "field_selection",
                "source_record_id_path",
                "source_record_type_path",
                "source_to_record_mapping",
                "text_join_separator",
                "title_path",
                "unicode_handling",
                "whitespace_rules",
            ] as &[_],
        ),
        (
            "chunking",
            &[
                "boundary_policy",
                "chunker_name",
                "chunker_version",
                "maximum_size",
                "overlap",
                "source_offset_policy",
                "stable_key_derivation",
                "units",
            ] as &[_],
        ),
        (
            "embedding",
            &[
                "dimension",
                "document_prefix",
                "input_construction",
                "model_checksum",
                "model_id",
                "model_output_normalization",
                "model_revision",
                "pooling",
                "quantization",
                "query_prefix",
                "runtime",
                "sequence_length",
                "tokenizer_id",
                "tokenizer_revision",
                "truncation_policy",
            ] as &[_],
        ),
        (
            "graph-construction",
            &[
                "duplicate_references",
                "inverse_edges",
                "judgment_inputs_sha256",
                "missing_target",
                "node_derivation",
                "relationship_derivation",
                "schema_sha256",
                "self_edges",
                "source_fields",
            ] as &[_],
        ),
        (
            "seed-policy",
            &["derived_policies", "explicit_policy", "normalization"] as &[_],
        ),
        (
            "split",
            &[
                "archive_sha256",
                "archive_url",
                "collection_rule",
                "development_population_sha256",
                "exclusion_counts",
                "license_id",
                "license_notice_source_id",
                "release_id",
                "source_inventory_sha256",
                "split_id",
                "test_lock_sha256",
                "test_population_sha256",
            ] as &[_],
        ),
    ]);
    for &(name, collection_inputs, upstream_prefixes, outputs) in &specs {
        let manifest = &manifests[name];
        let file = format!("manifests/{name}.json");
        if manifest.schema_version != 1
            || manifest.policy_id.is_empty()
            || manifest.policy_version.is_empty()
            || manifest.tool.name.is_empty()
            || manifest.tool.version.is_empty()
        {
            return Err(format!(
                "{file}: schema_version expected 1 and policy/tool fields must be non-empty"
            ));
        }
        object_fields(
            &file,
            "parameters",
            &manifest.parameters,
            parameter_keys[name],
        )?;
        validate_manifest_parameters(name, manifest, bytes)?;
        if !manifest
            .inputs
            .windows(2)
            .all(|pair| pair[0].source_id.as_bytes() < pair[1].source_id.as_bytes())
        {
            return Err(format!(
                "{file}: inputs expected strict source_id order without duplicates"
            ));
        }
        if !manifest
            .outputs
            .windows(2)
            .all(|pair| pair[0].path.as_bytes() < pair[1].path.as_bytes())
        {
            return Err(format!(
                "{file}: outputs expected strict path order without duplicates"
            ));
        }
        let actual_collection = manifest
            .inputs
            .iter()
            .filter_map(|input| input.source_id.strip_prefix("collection/"))
            .collect::<BTreeSet<_>>();
        let expected_collection = collection_inputs.iter().copied().collect::<BTreeSet<_>>();
        if actual_collection != expected_collection {
            return Err(format!(
                "{file}: transformation DAG collection inputs expected {:?}, actual {:?}",
                expected_collection, actual_collection
            ));
        }
        for input in &manifest.inputs {
            validate_sha(&file, "inputs[].sha256", &input.sha256)?;
            if let Some(path) = input.source_id.strip_prefix("collection/") {
                let actual = bytes.get(path).ok_or_else(|| {
                    format!(
                        "{file}: input '{}' references unknown collection path",
                        input.source_id
                    )
                })?;
                let digest = sha256(actual);
                if input.sha256 != digest {
                    return Err(format!(
                        "{file}: input '{}' expected sha256 {}, actual {}",
                        input.source_id, input.sha256, digest
                    ));
                }
            } else if !upstream_prefixes
                .iter()
                .any(|prefix| input.source_id.starts_with(prefix))
            {
                return Err(format!(
                    "{file}: unsafe or unpermitted input source_id '{}'",
                    input.source_id
                ));
            }
        }
        let expected_outputs = outputs.iter().copied().collect::<BTreeSet<_>>();
        let actual_outputs = manifest
            .outputs
            .iter()
            .map(|output| output.path.as_str())
            .collect::<BTreeSet<_>>();
        if actual_outputs != expected_outputs {
            return Err(format!(
                "{file}: transformation DAG outputs expected {:?}, actual {:?}",
                expected_outputs, actual_outputs
            ));
        }
        for output in &manifest.outputs {
            validate_relative_path(&file, "outputs[].path", &output.path)?;
            validate_sha(&file, "outputs[].sha256", &output.sha256)?;
            let digest = if let Some(value) = bytes.get(&output.path) {
                sha256(value)
            } else if let Some(collection) = ranking_collection {
                collection
                    .files
                    .iter()
                    .find(|entry| entry.path == output.path)
                    .map(|entry| entry.sha256.clone())
                    .ok_or_else(|| {
                        format!(
                            "{file}: output '{}' is absent from collection index",
                            output.path
                        )
                    })?
            } else {
                return Err(format!(
                    "{file}: output '{}' bytes are missing",
                    output.path
                ));
            };
            if output.sha256 != digest {
                return Err(format!(
                    "{file}: output '{}' expected sha256 {}, actual {}",
                    output.path, output.sha256, digest
                ));
            }
        }
        for required in upstream_prefixes
            .iter()
            .filter(|prefix| !matches!(**prefix, "upstream/graph/" | "upstream/scenario/"))
        {
            if !manifest
                .inputs
                .iter()
                .any(|input| input.source_id.starts_with(required))
            {
                return Err(format!(
                    "{file}: required upstream prefix '{required}' has no input"
                ));
            }
        }
    }
    let mut inventory = BTreeMap::new();
    for manifest in manifests.values() {
        for input in &manifest.inputs {
            if input.source_id.starts_with("collection/") {
                continue;
            }
            if let Some(previous) = inventory.insert(input.source_id.clone(), input.sha256.clone())
            {
                if previous != input.sha256 {
                    return Err(format!(
                        "transformation manifests: upstream source '{}' has conflicting digests {} and {}",
                        input.source_id, previous, input.sha256
                    ));
                }
            }
        }
    }
    for &(name, _, upstream_prefixes, _) in &specs {
        let manifest = &manifests[name];
        let actual = manifest
            .inputs
            .iter()
            .filter(|input| !input.source_id.starts_with("collection/"))
            .map(|input| input.source_id.as_str())
            .collect::<BTreeSet<_>>();
        let expected = inventory
            .keys()
            .filter(|source_id| {
                upstream_prefixes
                    .iter()
                    .any(|prefix| source_id.starts_with(prefix))
            })
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if actual != expected {
            return Err(format!(
                "manifests/{name}.json: upstream inventory closure expected {:?}, actual {:?}",
                expected, actual
            ));
        }
    }
    let inventory_preimage = Value::Array(
        inventory
            .into_iter()
            .map(|(source_id, sha256)| serde_json::json!({"sha256":sha256,"source_id":source_id}))
            .collect(),
    );
    let inventory_hash = sha256(canonical_json(&inventory_preimage)?.as_bytes());
    Ok(inventory_hash)
}

fn validate_manifest_parameters(
    name: &str,
    manifest: &TransformationManifest,
    bytes: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let file = format!("manifests/{name}.json");
    let parameters = manifest
        .parameters
        .as_object()
        .expect("closed parameters object");
    match name {
        "preprocessing" => {
            for field in [
                "source_to_record_mapping",
                "unicode_handling",
                "whitespace_rules",
            ] {
                if parameters[field].as_str().is_none_or(str::is_empty) {
                    return Err(format!(
                        "{file}: parameters.{field} expected non-empty string"
                    ));
                }
            }
            for field in ["field_selection", "source_record_id_path"] {
                let paths = parameters[field]
                    .as_array()
                    .ok_or_else(|| format!("{file}: parameters.{field} expected array"))?;
                if paths.is_empty() {
                    return Err(format!(
                        "{file}: parameters.{field} expected non-empty array"
                    ));
                }
            }
        }
        "chunking" => {
            for field in [
                "boundary_policy",
                "chunker_name",
                "chunker_version",
                "source_offset_policy",
                "stable_key_derivation",
                "units",
            ] {
                if parameters[field].as_str().is_none_or(str::is_empty) {
                    return Err(format!(
                        "{file}: parameters.{field} expected non-empty string"
                    ));
                }
            }
            let maximum = parameters["maximum_size"]
                .as_u64()
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    format!("{file}: parameters.maximum_size expected positive integer")
                })?;
            let overlap = parameters["overlap"].as_u64().ok_or_else(|| {
                format!("{file}: parameters.overlap expected non-negative integer")
            })?;
            if overlap >= maximum {
                return Err(format!(
                    "{file}: parameters.overlap must be smaller than maximum_size"
                ));
            }
        }
        "embedding" => {
            for field in [
                "model_checksum",
                "model_id",
                "model_revision",
                "tokenizer_id",
                "tokenizer_revision",
            ] {
                if parameters[field].as_str().is_none_or(str::is_empty) {
                    return Err(format!(
                        "{file}: parameters.{field} expected non-empty string"
                    ));
                }
            }
            validate_sha(
                &file,
                "parameters.model_checksum",
                parameters["model_checksum"].as_str().unwrap(),
            )?;
            if parameters["sequence_length"]
                .as_u64()
                .is_none_or(|value| value == 0)
            {
                return Err(format!(
                    "{file}: parameters.sequence_length expected positive integer"
                ));
            }
            if parameters["quantization"] != quantization_policy() {
                return Err(format!(
                    "{file}: parameters.quantization does not equal the frozen V3 policy"
                ));
            }
        }
        "graph-construction" => {
            if !parameters["judgment_inputs_sha256"].is_null() {
                return Err(format!(
                    "{file}: parameters.judgment_inputs_sha256 expected null"
                ));
            }
            let expected = sha256(&bytes["graph-schema.json"]);
            if parameters["schema_sha256"] != expected {
                return Err(format!(
                    "{file}: parameters.schema_sha256 expected {expected}, actual {}",
                    parameters["schema_sha256"]
                ));
            }
            let paths = parameters["source_fields"]
                .as_array()
                .ok_or_else(|| format!("{file}: parameters.source_fields expected array"))?;
            if paths.is_empty() {
                return Err(format!(
                    "{file}: parameters.source_fields expected non-empty array"
                ));
            }
            validate_sorted_unique_values(&file, "parameters.source_fields", paths)?;
        }
        "seed-policy" => {
            let normalization = &parameters["normalization"];
            object_fields(
                &file,
                "parameters.normalization",
                normalization,
                &[
                    "case_folding",
                    "normalization_form",
                    "normalization_version",
                    "punctuation",
                    "unicode_tables_sha256",
                    "unicode_version",
                    "whitespace",
                ],
            )?;
            let expected = serde_json::json!({
                "case_folding":"unicode_default_full_case_folding",
                "normalization_form":"NFC",
                "normalization_version":"unicode-15.1-nfc-full-fold-whitespace-v1",
                "punctuation":"preserve",
                "unicode_tables_sha256":normalization["unicode_tables_sha256"],
                "unicode_version":"15.1",
                "whitespace":"unicode_white_space_to_ascii_collapse_trim"
            });
            if *normalization != expected {
                return Err(format!(
                    "{file}: parameters.normalization does not equal the frozen V3 policy"
                ));
            }
            validate_sha(
                &file,
                "parameters.normalization.unicode_tables_sha256",
                normalization["unicode_tables_sha256"]
                    .as_str()
                    .unwrap_or(""),
            )?;
        }
        "split" => {
            for field in [
                "archive_sha256",
                "development_population_sha256",
                "source_inventory_sha256",
                "test_lock_sha256",
                "test_population_sha256",
            ] {
                validate_sha(
                    &file,
                    &format!("parameters.{field}"),
                    parameters[field].as_str().unwrap_or(""),
                )?;
            }
            for field in [
                "archive_url",
                "collection_rule",
                "license_id",
                "license_notice_source_id",
                "release_id",
                "split_id",
            ] {
                if parameters[field].as_str().is_none_or(str::is_empty) {
                    return Err(format!(
                        "{file}: parameters.{field} expected non-empty string"
                    ));
                }
            }
        }
        _ => unreachable!("closed manifest set"),
    }
    Ok(())
}

fn embedding_dimension(manifest: &TransformationManifest) -> Result<usize, String> {
    let value = manifest
        .parameters
        .get("dimension")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "manifests/embedding.json: parameters.dimension expected positive integer".to_owned()
        })? as usize;
    if value == 0 || value.saturating_mul(16_384) > i32::MAX as usize {
        return Err(format!(
            "manifests/embedding.json: dimension expected 1..={}, actual {value}",
            i32::MAX as usize / 16_384
        ));
    }
    Ok(value)
}

fn validate_seed_policy(
    manifest: &TransformationManifest,
    queries: &[Query],
    populations: &Populations,
    exclusions: &[Exclusion],
    replay_retained_aliases: bool,
) -> Result<(), String> {
    let parameters = manifest
        .parameters
        .as_object()
        .expect("validated parameters object");
    object_fields(
        "manifests/seed-policy.json",
        "parameters.explicit_policy",
        &parameters["explicit_policy"],
        &["policy_id", "policy_version", "provenance"],
    )?;
    let explicit = parameters["explicit_policy"].as_object().unwrap();
    if explicit["policy_id"] != "explicit"
        || explicit["policy_version"]
            .as_str()
            .is_none_or(str::is_empty)
    {
        return Err("manifests/seed-policy.json: explicit policy ID/version is invalid".to_owned());
    }
    let provenance = explicit["provenance"].as_array().ok_or_else(|| {
        "manifests/seed-policy.json: explicit provenance expected array".to_owned()
    })?;
    let mut provenance_ids = BTreeSet::new();
    let mut previous_provenance: Option<&str> = None;
    for row in provenance {
        object_fields(
            "manifests/seed-policy.json",
            "explicit provenance row",
            row,
            &["query_id", "source_id", "transformation_id"],
        )?;
        let query_id = row["query_id"].as_str().ok_or_else(|| {
            "manifests/seed-policy.json: explicit provenance query_id expected string".to_owned()
        })?;
        if previous_provenance.is_some_and(|previous| previous.as_bytes() >= query_id.as_bytes())
            || !provenance_ids.insert(query_id.to_owned())
            || row["source_id"].as_str().is_none_or(str::is_empty)
            || row["transformation_id"].as_str().is_none_or(str::is_empty)
        {
            return Err(
                "manifests/seed-policy.json: explicit provenance expected strict query order, unique IDs, and non-empty sources"
                    .to_owned(),
            );
        }
        previous_provenance = Some(query_id);
    }
    if provenance_ids != populations.explicit {
        return Err(format!(
            "manifests/seed-policy.json: explicit provenance population expected {:?}, actual {:?}",
            populations.explicit, provenance_ids
        ));
    }
    let policies = parameters["derived_policies"]
        .as_array()
        .ok_or_else(|| "manifests/seed-policy.json: derived_policies expected array".to_owned())?;
    let mut ids = BTreeSet::new();
    let mut previous_policy: Option<&str> = None;
    for policy in policies {
        object_fields(
            "manifests/seed-policy.json",
            "derived policy",
            policy,
            &[
                "aliases",
                "alias_table_sha256",
                "declared_population_sha256",
                "failure_population_sha256",
                "policy_id",
                "policy_version",
                "source_fields",
                "successful_population_sha256",
            ],
        )?;
        let id = policy["policy_id"].as_str().ok_or_else(|| {
            "manifests/seed-policy.json: derived policy_id expected string".to_owned()
        })?;
        validate_derived_policy_id("manifests/seed-policy.json", id)?;
        if previous_policy.is_some_and(|previous| previous.as_bytes() >= id.as_bytes())
            || !ids.insert(id)
        {
            return Err(format!(
                "manifests/seed-policy.json: derived policies are not in strict policy-ID order or collide at '{id}'"
            ));
        }
        previous_policy = Some(id);
        if policy["policy_version"].as_str().is_none_or(str::is_empty) {
            return Err(format!(
                "manifests/seed-policy.json: policy '{id}' has empty policy_version"
            ));
        }
        let source_fields = policy["source_fields"].as_array().ok_or_else(|| {
            format!("manifests/seed-policy.json: policy '{id}' source_fields expected array")
        })?;
        if source_fields.is_empty() {
            return Err(format!(
                "manifests/seed-policy.json: policy '{id}' source_fields expected non-empty array"
            ));
        }
        validate_sorted_unique_values(
            "manifests/seed-policy.json",
            "derived_policies[].source_fields",
            source_fields,
        )?;
        let declared = populations.derived_declared.get(id).ok_or_else(|| {
            format!("manifests/seed-policy.json: policy '{id}' has no declared query population")
        })?;
        let failed = &populations.derived_failed[id];
        let successful = populations.successful(id);
        for (field, actual, expected) in [
            (
                "declared_population_sha256",
                policy["declared_population_sha256"].as_str(),
                population_hash(declared),
            ),
            (
                "failure_population_sha256",
                policy["failure_population_sha256"].as_str(),
                population_hash(failed),
            ),
            (
                "successful_population_sha256",
                policy["successful_population_sha256"].as_str(),
                population_hash(&successful),
            ),
        ] {
            if actual != Some(expected.as_str()) {
                return Err(format!(
                    "manifests/seed-policy.json: policy '{id}' field '{field}' expected {expected}, actual {:?}",
                    actual
                ));
            }
        }
        let aliases = policy["aliases"].as_array().ok_or_else(|| {
            format!("manifests/seed-policy.json: policy '{id}' aliases expected array")
        })?;
        let alias_preimage = canonical_json(&Value::Array(aliases.clone()))?;
        let alias_hash = sha256(alias_preimage.as_bytes());
        if policy["alias_table_sha256"] != alias_hash {
            return Err(format!(
                "manifests/seed-policy.json: policy '{id}' alias_table_sha256 expected {alias_hash}, actual {}",
                policy["alias_table_sha256"]
            ));
        }
        validate_aliases(id, aliases)?;
        // HotpotQA freezes exact-title ambiguity against the complete upstream
        // title universe before retaining its bounded, label-blind corpus. Its
        // independent adapter validator replays that upstream resolution. The
        // retained alias table cannot reproduce candidates intentionally left
        // outside the frozen corpus, so only those two contract-locked roots
        // skip this generic retained-table replay.
        if replay_retained_aliases {
            validate_resolutions(id, aliases, queries, exclusions)?;
        }
    }
    let expected_ids = populations
        .derived_declared
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if ids != expected_ids {
        return Err(
            "manifests/seed-policy.json: derived policy IDs do not match query declarations"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_aliases(policy: &str, aliases: &[Value]) -> Result<(), String> {
    let mut previous: Option<AliasSortKey> = None;
    let mut exact_rows = BTreeSet::new();
    for alias in aliases {
        object_fields(
            "manifests/seed-policy.json",
            "alias row",
            alias,
            &["alias", "normalized_alias", "seed", "source"],
        )?;
        object_fields(
            "manifests/seed-policy.json",
            "alias source",
            &alias["source"],
            &["field", "record_id"],
        )?;
        let raw = alias["alias"].as_str().ok_or_else(|| {
            format!("manifests/seed-policy.json: policy '{policy}' alias expected string")
        })?;
        let normalized = normalize(raw);
        if normalized.is_empty() || alias["normalized_alias"] != normalized {
            return Err(format!(
                "manifests/seed-policy.json: policy '{policy}' alias '{raw}' normalized_alias expected '{normalized}', actual {}",
                alias["normalized_alias"]
            ));
        }
        validate_seed(
            "manifests/seed-policy.json",
            "aliases[].seed",
            &alias["seed"],
        )?;
        let source = alias["source"].as_object().unwrap();
        let record_id = source["record_id"].as_str().unwrap_or("");
        validate_eval_id(
            "manifests/seed-policy.json",
            "aliases[].source.record_id",
            record_id,
        )?;
        let field = source["field"].as_array().ok_or_else(|| {
            "manifests/seed-policy.json: aliases[].source.field expected field path".to_owned()
        })?;
        let path = field
            .iter()
            .map(|segment| segment.as_str().unwrap_or("").to_owned())
            .collect::<Vec<_>>();
        validate_field_path(
            "manifests/seed-policy.json",
            "aliases[].source.field",
            &path,
        )?;
        let key = (
            normalized.as_bytes().to_vec(),
            canonical_json(&alias["seed"])?.into_bytes(),
            record_id.as_bytes().to_vec(),
            canonical_json(&source["field"])?.into_bytes(),
            raw.as_bytes().to_vec(),
        );
        let encoded = canonical_json(alias)?;
        if previous.as_ref().is_some_and(|value| value >= &key) || !exact_rows.insert(encoded) {
            return Err(format!(
                "manifests/seed-policy.json: policy '{policy}' aliases are not in the required strict tuple order or contain duplicates"
            ));
        }
        previous = Some(key);
    }
    Ok(())
}

fn validate_resolutions(
    policy: &str,
    aliases: &[Value],
    queries: &[Query],
    exclusions: &[Exclusion],
) -> Result<(), String> {
    for query in queries
        .iter()
        .filter(|query| query.derived_seed_policy_id.as_deref() == Some(policy))
    {
        let normalized_query = normalize(&query.text);
        let mut matches = aliases
            .iter()
            .filter(|row| {
                let alias = row["normalized_alias"].as_str().unwrap();
                normalized_query
                    .match_indices(alias)
                    .any(|(start, _)| boundary_match(&normalized_query, start, start + alias.len()))
            })
            .collect::<Vec<_>>();
        let longest = matches
            .iter()
            .map(|row| row["normalized_alias"].as_str().unwrap().chars().count())
            .max()
            .unwrap_or(0);
        matches.retain(|row| row["normalized_alias"].as_str().unwrap().chars().count() == longest);
        let seeds = matches
            .iter()
            .map(|row| canonical_json(&row["seed"]))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let actual_reason = if matches.is_empty() {
            Some("derived_seed_no_match")
        } else if seeds.len() > 1 {
            Some("derived_seed_ambiguous")
        } else {
            None
        };
        let declared_reason = exclusions
            .iter()
            .find(|row| row.query_id == query.query_id && row.lane == policy)
            .map(|row| row.reason.as_str());
        if actual_reason != declared_reason {
            return Err(format!(
                "manifests/seed-policy.json: resolver query '{}' policy '{}' expected exclusion {:?}, actual {:?}",
                query.query_id, policy, actual_reason, declared_reason
            ));
        }
    }
    Ok(())
}

fn validate_split_manifest(
    manifest: &TransformationManifest,
    collection: &Collection,
    populations: &Populations,
    exclusions: &[Exclusion],
    source_inventory_sha256: &str,
) -> Result<(), String> {
    let parameters = manifest
        .parameters
        .as_object()
        .expect("validated parameters object");
    let hash = population_hash(&populations.q);
    let empty = sha256(b"");
    let hotpot_development = "1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f";
    let hotpot_test = "9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010";
    let (development, test) = match collection.collection_id.as_str() {
        "hotpotqa-linked-abstracts-graph-v1-development"
        | "hotpotqa-linked-abstracts-graph-v1-test" => (hotpot_development, hotpot_test),
        _ if collection.split == "development" => (hash.as_str(), empty.as_str()),
        _ => (empty.as_str(), hash.as_str()),
    };
    if parameters["development_population_sha256"] != development
        || parameters["test_population_sha256"] != test
    {
        return Err(format!(
            "manifests/split.json: population hashes expected development {development}, test {test}; actual {}/{}",
            parameters["development_population_sha256"], parameters["test_population_sha256"]
        ));
    }
    if parameters["source_inventory_sha256"] != source_inventory_sha256 {
        return Err(format!(
            "manifests/split.json: source_inventory_sha256 expected {source_inventory_sha256}, actual {}",
            parameters["source_inventory_sha256"]
        ));
    }
    let inventory_has_license = manifest.inputs.iter().any(|input| {
        Some(input.source_id.as_str()) == parameters["license_notice_source_id"].as_str()
            && input.source_id.starts_with("upstream/license/")
    });
    if !inventory_has_license {
        return Err(
            "manifests/split.json: license_notice_source_id must name its upstream/license input"
                .to_owned(),
        );
    }
    let rows = parameters["exclusion_counts"]
        .as_array()
        .ok_or_else(|| "manifests/split.json: exclusion_counts expected array".to_owned())?;
    let mut expected_rows = Vec::new();
    let mut global_before =
        populations.q.len() + exclusions.iter().filter(|row| row.lane == "global").count();
    for reason in [
        "duplicate_identity",
        "filter_label_conflict",
        "invalid_upstream_record",
        "missing_complete_evidence",
        "no_relevant_documents",
        "not_in_frozen_corpus",
    ] {
        let excluded = exclusions
            .iter()
            .filter(|row| row.lane == "global" && row.reason == reason)
            .count();
        expected_rows.push(serde_json::json!({
            "after":global_before-excluded,
            "before":global_before,
            "excluded":excluded,
            "lane":"global",
            "reason":reason
        }));
        global_before -= excluded;
    }
    for (policy, declared) in &populations.derived_declared {
        let mut before = declared.len();
        for reason in ["derived_seed_ambiguous", "derived_seed_no_match"] {
            let excluded = exclusions
                .iter()
                .filter(|row| row.lane == *policy && row.reason == reason)
                .count();
            expected_rows.push(serde_json::json!({
                "after":before-excluded,
                "before":before,
                "excluded":excluded,
                "lane":policy,
                "reason":reason
            }));
            before -= excluded;
        }
    }
    if rows != &expected_rows {
        return Err(format!(
            "manifests/split.json: exclusion_counts expected {}, actual {}",
            canonical_json(&Value::Array(expected_rows))?,
            canonical_json(&Value::Array(rows.clone()))?
        ));
    }
    let lock_preimage = serde_json::json!({
        "collection_rule":parameters["collection_rule"],
        "development_population_sha256":parameters["development_population_sha256"],
        "exclusion_counts":parameters["exclusion_counts"],
        "release_id":parameters["release_id"],
        "source_inventory_sha256":parameters["source_inventory_sha256"],
        "split_id":parameters["split_id"],
        "test_population_sha256":parameters["test_population_sha256"]
    });
    let lock_hash = sha256(canonical_json(&lock_preimage)?.as_bytes());
    if parameters["test_lock_sha256"] != lock_hash {
        return Err(format!(
            "manifests/split.json: test_lock_sha256 expected {lock_hash}, actual {}",
            parameters["test_lock_sha256"]
        ));
    }
    Ok(())
}

fn validate_run_preimages(runs: &[RunIdentity]) -> Result<(), String> {
    let mut logical = BTreeSet::new();
    for run in runs {
        let reencoded = canonical_json(&run.configuration)?;
        if reencoded != run.configuration_preimage {
            return Err(format!(
                "run '{}' has invalid run configuration preimage",
                run.run_id
            ));
        }
        if !logical.insert(run.logical_run_sha256.as_str()) {
            return Err(format!("run '{}' collides on logical-run hash", run.run_id));
        }
    }
    Ok(())
}

fn parse_qrels(path: &Path, bytes: &[u8]) -> Result<Vec<Qrel>, String> {
    validate_text_bytes(path, bytes, true)?;
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let text = std::str::from_utf8(bytes).expect("already validated UTF-8");
    let mut rows = Vec::new();
    let mut previous: Option<(&str, &str)> = None;
    for (offset, line) in text[..text.len() - 1].split('\n').enumerate() {
        let fields = line.split(' ').collect::<Vec<_>>();
        if fields.len() != 4 || fields.iter().any(|field| field.is_empty()) || fields[1] != "0" {
            return Err(format!(
                "{}: row {} expected exactly 'query_id 0 record_id relevance', actual '{line}'",
                path.display(),
                offset + 1
            ));
        }
        validate_eval_id("qrels.tsv", "query_id", fields[0])?;
        validate_eval_id("qrels.tsv", "record_id", fields[2])?;
        if fields[3].len() > 1 && fields[3].starts_with('0') {
            return Err(format!(
                "qrels.tsv: row {} relevance has leading zero '{}'",
                offset + 1,
                fields[3]
            ));
        }
        let relevance = fields[3].parse::<u8>().map_err(|_| {
            format!(
                "qrels.tsv: row {} relevance expected 0..127, actual '{}'",
                offset + 1,
                fields[3]
            )
        })?;
        if relevance > 127 {
            return Err(format!(
                "qrels.tsv: row {} relevance expected 0..127, actual {relevance}",
                offset + 1
            ));
        }
        let key = (fields[0], fields[2]);
        if previous.is_some_and(|value| value >= key) {
            return Err(format!(
                "qrels.tsv: incorrect file order or duplicate at row {}",
                offset + 1
            ));
        }
        previous = Some(key);
        rows.push(Qrel {
            query_id: fields[0].to_owned(),
            record_id: fields[2].to_owned(),
            relevance,
        });
    }
    Ok(rows)
}

fn validate_tagged_value(file: &str, field: &str, value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{file}: field '{field}' expected tagged object"))?;
    let tag = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{file}: field '{field}.type' expected string"))?;
    let keys = if tag == "null" {
        &["type"][..]
    } else {
        &["type", "value"][..]
    };
    object_fields(file, field, value, keys)?;
    let payload = object.get("value");
    let valid = match tag {
        "null" => true,
        "boolean" => payload.is_some_and(Value::is_boolean),
        "integer" => payload.and_then(Value::as_i64).is_some(),
        "float" => payload.and_then(Value::as_f64).is_some_and(f64::is_finite),
        "string" => payload.is_some_and(Value::is_string),
        "list" => {
            if let Some(values) = payload.and_then(Value::as_array) {
                for (index, value) in values.iter().enumerate() {
                    validate_tagged_value(file, &format!("{field}.value[{index}]"), value)?;
                }
                true
            } else {
                false
            }
        }
        "object" => {
            if let Some(values) = payload.and_then(Value::as_object) {
                for (key, value) in values {
                    validate_production_id(file, field, key)?;
                    validate_tagged_value(file, &format!("{field}.value.{key}"), value)?;
                }
                true
            } else {
                false
            }
        }
        _ => false,
    };
    if !valid {
        return Err(format!(
            "{file}: field '{field}' has invalid tagged type/value '{tag}'"
        ));
    }
    Ok(())
}

fn validate_metadata(file: &str, metadata: &BTreeMap<String, Value>) -> Result<(), String> {
    for (field, value) in metadata {
        if field.is_empty() {
            return Err(format!("{file}: metadata field name must be non-empty"));
        }
        let object = value
            .as_object()
            .ok_or_else(|| format!("{file}: metadata.{field} expected tagged scalar object"))?;
        let tag = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{file}: metadata.{field}.type expected string"))?;
        object_fields(
            file,
            &format!("metadata.{field}"),
            value,
            &["type", "value"],
        )?;
        let payload = &object["value"];
        let valid = match tag {
            "string" => payload.is_string(),
            "integer" | "timestamp_millis" => payload.as_i64().is_some(),
            "float" => payload.as_f64().is_some_and(f64::is_finite),
            "boolean" => payload.is_boolean(),
            _ => false,
        };
        if !valid {
            return Err(format!(
                "{file}: metadata.{field} invalid scalar type/value '{tag}'"
            ));
        }
    }
    Ok(())
}

fn validate_filter(file: &str, field: &str, value: &Value, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err(format!("{file}: field '{field}' logical depth exceeds 16"));
    }
    let object = value
        .as_object()
        .ok_or_else(|| format!("{file}: field '{field}' expected filter object"))?;
    let op = object
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{file}: field '{field}.op' expected string"))?;
    match op {
        "equals" | "not_equals" => {
            object_fields(file, field, value, &["field", "op", "value"])?;
            validate_metadata_operand(file, field, &object["value"])?;
        }
        "in" => {
            object_fields(file, field, value, &["field", "op", "values"])?;
            let values = object["values"]
                .as_array()
                .ok_or_else(|| format!("{file}: {field}.values expected array"))?;
            if values.is_empty() {
                return Err(format!("{file}: {field}.values expected non-empty"));
            }
            for value in values {
                validate_metadata_operand(file, field, value)?;
            }
            validate_sorted_unique_values(file, field, values)?;
            let types = values
                .iter()
                .filter_map(|value| value["type"].as_str())
                .collect::<BTreeSet<_>>();
            if types.len() != 1 {
                return Err(format!(
                    "{file}: {field}.values expected homogeneous metadata types"
                ));
            }
        }
        "range" => {
            object_fields(file, field, value, &["field", "lower", "op", "upper"])?;
            if object["lower"].is_null() && object["upper"].is_null() {
                return Err(format!("{file}: {field} range requires at least one bound"));
            }
            for bound in ["lower", "upper"] {
                if !object[bound].is_null() {
                    validate_metadata_operand(file, field, &object[bound])?;
                    if !matches!(
                        object[bound]["type"].as_str(),
                        Some("integer" | "float" | "timestamp_millis")
                    ) {
                        return Err(format!(
                            "{file}: {field}.{bound} expected numeric metadata type"
                        ));
                    }
                }
            }
            let lower_type = object["lower"]["type"].as_str();
            let upper_type = object["upper"]["type"].as_str();
            if lower_type.is_some() && upper_type.is_some() && lower_type != upper_type {
                return Err(format!(
                    "{file}: {field} range bounds expected the same metadata type"
                ));
            }
        }
        "exists" => {
            object_fields(file, field, value, &["field", "op"])?;
        }
        "all" | "any" => {
            object_fields(file, field, value, &["children", "op"])?;
            let children = object["children"]
                .as_array()
                .ok_or_else(|| format!("{file}: {field}.children expected array"))?;
            if children.is_empty() {
                return Err(format!("{file}: {field}.children expected non-empty"));
            }
            for child in children {
                validate_filter(file, field, child, depth + 1)?;
            }
            validate_sorted_unique_values(file, field, children)?;
        }
        _ => {
            return Err(format!(
                "{file}: field '{field}.op' expected supported filter, actual '{op}'"
            ))
        }
    }
    if let Some(name) = object.get("field").and_then(Value::as_str) {
        if name.is_empty() {
            return Err(format!(
                "{file}: field '{field}.field' expected non-empty string"
            ));
        }
    }
    Ok(())
}

fn validate_metadata_operand(file: &str, field: &str, value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{file}: {field} metadata operand expected object"))?;
    object_fields(file, field, value, &["type", "value"])?;
    let tag = object["type"]
        .as_str()
        .ok_or_else(|| format!("{file}: {field} operand type expected string"))?;
    let valid = match tag {
        "string" => object["value"].is_string(),
        "integer" | "timestamp_millis" => object["value"].as_i64().is_some(),
        "float" => object["value"].as_f64().is_some_and(f64::is_finite),
        "boolean" => object["value"].is_boolean(),
        _ => false,
    };
    if !valid {
        return Err(format!("{file}: {field} invalid metadata operand '{tag}'"));
    }
    Ok(())
}

fn validate_seed(file: &str, field: &str, value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{file}: field '{field}' expected seed object"))?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{file}: field '{field}.kind' expected string"))?;
    match kind {
        "node_ids" => {
            object_fields(file, field, value, &["kind", "nodes"])?;
            let nodes = object["nodes"]
                .as_array()
                .ok_or_else(|| format!("{file}: {field}.nodes expected array"))?;
            if nodes.is_empty() {
                return Err(format!("{file}: {field}.nodes expected non-empty"));
            }
            for node in nodes {
                validate_node_identity_value(file, node)?;
            }
            validate_sorted_unique_values(file, field, nodes)?;
        }
        "equals" => {
            object_fields(
                file,
                field,
                value,
                &["field", "kind", "node_type", "values"],
            )?;
            validate_production_id(file, field, object["node_type"].as_str().unwrap_or(""))?;
            let path = object["field"]
                .as_array()
                .ok_or_else(|| format!("{file}: {field}.field expected array"))?
                .iter()
                .map(|value| value.as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>();
            validate_field_path(file, field, &path)?;
            let values = object["values"]
                .as_array()
                .ok_or_else(|| format!("{file}: {field}.values expected array"))?;
            if values.is_empty() {
                return Err(format!("{file}: {field}.values expected non-empty"));
            }
            for scalar in values {
                validate_graph_scalar(file, field, scalar)?;
            }
            validate_sorted_unique_values(file, field, values)?;
        }
        _ => {
            return Err(format!(
                "{file}: field '{field}.kind' expected node_ids or equals, actual '{kind}'"
            ))
        }
    }
    Ok(())
}

fn validate_graph_scalar(file: &str, field: &str, value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{file}: {field} graph scalar expected object"))?;
    object_fields(file, field, value, &["type", "value"])?;
    let tag = object["type"].as_str().unwrap_or("");
    let valid = match tag {
        "string" => object["value"].is_string(),
        "integer" => object["value"].as_i64().is_some(),
        "boolean" => object["value"].is_boolean(),
        _ => false,
    };
    if !valid {
        return Err(format!("{file}: {field} invalid graph scalar type '{tag}'"));
    }
    Ok(())
}

fn validate_node_identity_value(file: &str, value: &Value) -> Result<(), String> {
    object_fields(file, "node identity", value, &["node_type", "source"])?;
    validate_production_id(file, "node_type", value["node_type"].as_str().unwrap_or(""))?;
    let source = &value["source"];
    let object = source
        .as_object()
        .ok_or_else(|| format!("{file}: node source expected object"))?;
    match object.get("kind").and_then(Value::as_str) {
        Some("record") => {
            object_fields(file, "node source", source, &["kind", "record_id"])?;
            validate_eval_id(
                file,
                "record_id",
                source["record_id"].as_str().unwrap_or(""),
            )?;
        }
        Some("chunk") => {
            object_fields(
                file,
                "node source",
                source,
                &["chunk_key", "kind", "record_id"],
            )?;
            validate_eval_id(
                file,
                "record_id",
                source["record_id"].as_str().unwrap_or(""),
            )?;
            validate_eval_id(
                file,
                "chunk_key",
                source["chunk_key"].as_str().unwrap_or(""),
            )?;
        }
        actual => {
            return Err(format!(
                "{file}: node source.kind expected record or chunk, actual {actual:?}"
            ))
        }
    }
    Ok(())
}

fn validate_node_reference(
    file: &str,
    node: &NodeIdentity,
    record_ids: &BTreeSet<&str>,
    records: &[Record],
) -> Result<(), String> {
    validate_production_id(file, "node_type", &node.node_type)?;
    match &node.source {
        NodeSource::Record { record_id } => {
            if !record_ids.contains(record_id.as_str()) {
                return Err(format!(
                    "{file}: invalid expected-path record reference '{record_id}'"
                ));
            }
        }
        NodeSource::Chunk {
            record_id,
            chunk_key,
        } => {
            if !records.iter().any(|record| {
                record.record_id == *record_id
                    && record
                        .chunks
                        .iter()
                        .any(|chunk| chunk.chunk_key == *chunk_key)
            }) {
                return Err(format!(
                    "{file}: invalid expected-path chunk reference '{record_id}/{chunk_key}'"
                ));
            }
        }
    }
    Ok(())
}

fn validate_sorted_unique_values(file: &str, field: &str, values: &[Value]) -> Result<(), String> {
    let encoded = values
        .iter()
        .map(canonical_json)
        .collect::<Result<Vec<_>, _>>()?;
    validate_sorted_unique_encoded(file, field, &encoded)
}

fn validate_sorted_unique_arrays(
    file: &str,
    field: &str,
    values: &[Vec<String>],
) -> Result<(), String> {
    for value in values {
        validate_sorted_unique_strings(file, field, value)?;
    }
    let encoded = values
        .iter()
        .map(|value| canonical_json(&serde_json::json!(value)))
        .collect::<Result<Vec<_>, _>>()?;
    validate_sorted_unique_encoded(file, field, &encoded)
}

fn validate_sorted_unique_encoded(
    file: &str,
    field: &str,
    values: &[String],
) -> Result<(), String> {
    if !values
        .windows(2)
        .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
    {
        return Err(format!(
            "{file}: field '{field}' expected duplicate-free canonical-byte order, actual {values:?}"
        ));
    }
    Ok(())
}

fn validate_sorted_unique_strings(
    file: &str,
    field: &str,
    values: &[String],
) -> Result<(), String> {
    if !values
        .windows(2)
        .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
    {
        return Err(format!(
            "{file}: field '{field}' expected duplicate-free lexical order, actual {values:?}"
        ));
    }
    Ok(())
}

fn validate_field_path(file: &str, field: &str, path: &[String]) -> Result<(), String> {
    if path.is_empty() {
        return Err(format!(
            "{file}: field '{field}' expected non-empty field path"
        ));
    }
    for segment in path {
        validate_production_id(file, field, segment)?;
    }
    Ok(())
}

fn validate_eval_id(file: &str, field: &str, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let valid = (1..=128).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b':' | b'-'));
    if !valid {
        return Err(format!(
            "{file}: field '{field}' expected identifier [A-Za-z0-9][A-Za-z0-9._:-]* length 1..128, actual '{value}'"
        ));
    }
    Ok(())
}

fn validate_production_id(file: &str, field: &str, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let valid = (1..=64).contains(&bytes.len())
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
    if !valid {
        return Err(format!(
            "{file}: field '{field}' expected production identifier [A-Za-z_][A-Za-z0-9_]{{0,63}}, actual '{value}'"
        ));
    }
    Ok(())
}

fn validate_derived_policy_id(file: &str, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let valid = (1..=32).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !matches!(value, "explicit" | "global" | "na" | "none");
    if !valid {
        return Err(format!(
            "{file}: derived policy ID expected lowercase run-safe ID excluding explicit/global/na/none, actual '{value}'"
        ));
    }
    Ok(())
}

fn validate_sha(file: &str, field: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!(
            "{file}: field '{field}' expected 64 lowercase hexadecimal characters, actual '{value}'"
        ));
    }
    Ok(())
}

fn validate_relative_path(file: &str, field: &str, value: &str) -> Result<(), String> {
    let path = Path::new(value);
    let safe = !value.is_empty()
        && !value.contains('\\')
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !safe {
        return Err(format!(
            "{file}: field '{field}' expected safe contained relative path without '.', '..', or backslash, actual '{value}'"
        ));
    }
    Ok(())
}

fn normalize(value: &str) -> String {
    super::v3_seed::normalize(value)
}

fn boundary_match(value: &str, start: usize, end: usize) -> bool {
    fn alnum(character: Option<char>) -> bool {
        character.is_some_and(char::is_alphanumeric)
    }
    let before = value[..start].chars().next_back();
    let first = value[start..end].chars().next();
    let last = value[start..end].chars().next_back();
    let after = value[end..].chars().next();
    (start == 0 || alnum(before) != alnum(first))
        && (end == value.len() || alnum(last) != alnum(after))
}

fn read(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    fs::read(root.join(relative)).map_err(|error| {
        format!(
            "failed to read '{}': {error}",
            root.join(relative).display()
        )
    })
}

fn parse_rows<T: serde::de::DeserializeOwned>(
    file: &str,
    values: Vec<Value>,
) -> Result<Vec<T>, String> {
    values
        .into_iter()
        .enumerate()
        .map(|(offset, value)| from_value(&format!("{file}: row {}", offset + 1), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    struct TestCollection {
        root: PathBuf,
    }

    impl TestCollection {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "vectorkit-v3-malformed-{}-{}",
                std::process::id(),
                TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            copy_directory(&fixture_root(), &root);
            Self { root }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }

        fn mutate_json(&self, relative: &str, mutate: impl FnOnce(&mut Value)) {
            let mut value: Value =
                serde_json::from_slice(&fs::read(self.path(relative)).expect("fixture file reads"))
                    .expect("fixture JSON parses");
            mutate(&mut value);
            write_json(&self.path(relative), &value);
            if relative != "collection.json" {
                refresh_file_index(&self.root, relative);
            }
        }

        fn mutate_jsonl(&self, relative: &str, mutate: impl FnOnce(&mut Vec<Value>)) {
            let bytes = fs::read(self.path(relative)).expect("fixture file reads");
            let mut rows = if bytes.is_empty() {
                Vec::new()
            } else {
                std::str::from_utf8(&bytes)
                    .unwrap()
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect::<Vec<_>>()
            };
            mutate(&mut rows);
            let mut output = Vec::new();
            for row in rows {
                output.extend_from_slice(canonical_json(&row).unwrap().as_bytes());
                output.push(b'\n');
            }
            fs::write(self.path(relative), output).unwrap();
            refresh_file_index(&self.root, relative);
        }

        fn write_raw(&self, relative: &str, bytes: &[u8], refresh: bool) {
            fs::write(self.path(relative), bytes).unwrap();
            if refresh {
                refresh_file_index(&self.root, relative);
            }
        }
    }

    impl Drop for TestCollection {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn copy_directory(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_directory(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    fn write_json(path: &Path, value: &Value) {
        let mut bytes = canonical_json(value).unwrap().into_bytes();
        bytes.push(b'\n');
        fs::write(path, bytes).unwrap();
    }

    fn refresh_file_index(root: &Path, relative: &str) {
        let bytes = fs::read(root.join(relative)).unwrap();
        let collection_path = root.join("collection.json");
        let mut collection: Value =
            serde_json::from_slice(&fs::read(&collection_path).unwrap()).unwrap();
        let row = collection["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["path"] == relative)
            .unwrap();
        row["bytes"] = serde_json::json!(bytes.len());
        row["sha256"] = serde_json::json!(sha256(&bytes));
        write_json(&collection_path, &collection);
    }

    fn assert_rejected(expected: &str, mutate: impl FnOnce(&TestCollection)) {
        let collection = TestCollection::new();
        mutate(&collection);
        let error = validate(&collection.root).unwrap_err();
        assert!(
            error.contains(expected),
            "expected error containing {expected:?}, actual {error:?}"
        );
    }

    #[test]
    fn checked_in_v3_collection_passes_and_has_fifteen_runs() {
        let validated = validate(&fixture_root()).unwrap();
        assert_eq!(validated.runs.len(), 15);
        assert_eq!(
            population_hash(&validated.populations.q),
            "91be2f127eff88b3d41229df2904cb3b7203992673711e3ee960ade05c35496d"
        );
        assert_eq!(
            population_hash(&validated.populations.retrieval),
            "c373605c9580a90c0194ed28f5e07debfef5f8315547e9af5eb2cae963bfd4e3"
        );
        let identities = validated
            .runs
            .iter()
            .map(|run| (run.run_id.as_str(), run.logical_run_sha256.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            identities,
            [
                (
                    "v3-a-whole-semantic-f32-na-cfg-984e4c3bf991",
                    "bf237c1a474816a1f8c8dcb0580694c19ccd53cb5420c99b0419c3dd8bba2711"
                ),
                (
                    "v3-b-whole-semantic-i8-na-cfg-e9898ca6ef53",
                    "e0b946e2b8c926badacc6f6fa104d52c33f72f6e8408820f969b59f5d6a6261b"
                ),
                (
                    "v3-c-whole-weighted-i8-na-cfg-81e0395aa8e0",
                    "df48c1d3a962997bf21f037c6eae1905ed423576933da54dde749b9170af0b21"
                ),
                (
                    "v3-d-selection-none-none-explicit-cfg-13feb2a18ac3",
                    "1bedbc6a99c164ed8ab69287192bf7287577eeb278406b9475cf3232bb2b0bde"
                ),
                (
                    "v3-d-selection-none-none-team-cfg-7278e2315c8f",
                    "2c7850eb3ca1c9258765ff9b7dd338d00387e3132b6a4e5380bbac072d38c1aa"
                ),
                (
                    "v3-d-selection-none-none-topic-cfg-bf6bed5c72e7",
                    "03e34447316a451bb023fb82635d0c91dee8f343e37eab909697528e2095302a"
                ),
                (
                    "v3-e-graph-semantic-f32-explicit-cfg-d2855327ee28",
                    "fd70339f21946498b010c4d26e719158212a9de0a2e745fcbc4d75b3c0ccdb25"
                ),
                (
                    "v3-e-graph-semantic-f32-team-cfg-9d005ed09abd",
                    "ffdf1b57a1cab91c5e3ecb0f7841a3ca69f8db8f58531c1c4f943ec85a3a7a02"
                ),
                (
                    "v3-e-graph-semantic-f32-topic-cfg-dd783bc155d4",
                    "665dc02290fb825c82a55c728febd3bb8c1e98e9c7cc1fd475481aa0b9cccdd8"
                ),
                (
                    "v3-f-graph-semantic-i8-explicit-cfg-9199f34e596a",
                    "1825b9e865bdd436095e5d98984a1ef9faf83dbe02ffa3268e04d463a5fd4de2"
                ),
                (
                    "v3-f-graph-semantic-i8-team-cfg-c9fe28bfe8a2",
                    "9e3b11888396550e38aafcec9baffdd970c588a838c561cecb3655e66b4b3f77"
                ),
                (
                    "v3-f-graph-semantic-i8-topic-cfg-748772f67f91",
                    "da4bbb529aaf3ba23fa09177f62a7f760f018438d499dae00641fa2720622cd8"
                ),
                (
                    "v3-g-graph-weighted-i8-explicit-cfg-f5f6dfcae573",
                    "91a780087bce21816e0a71017146d19fdc87e1b0d38b3fea2a02e36254bec0aa"
                ),
                (
                    "v3-g-graph-weighted-i8-team-cfg-0562c721d6e7",
                    "0f0022104a1921d80f09e302e653a1877ef502d363f70a9dc46dc7c0c0bbcf7a"
                ),
                (
                    "v3-g-graph-weighted-i8-topic-cfg-36c6887ab88d",
                    "1a6c8c0e321bd3b92194ede4257f041eaddcdf2e9e4388bbebb3ad9b006218c2"
                ),
            ]
        );
    }

    #[test]
    fn malformed_layout_and_file_index_are_rejected() {
        assert_rejected("unexpected", |collection| {
            fs::write(collection.path("unexpected.txt"), b"unexpected\n").unwrap();
        });
        assert_rejected("expected sha256", |collection| {
            let mut bytes = fs::read(collection.path("records.jsonl")).unwrap();
            bytes[0] = b'[';
            collection.write_raw("records.jsonl", &bytes, false);
        });
    }

    #[test]
    fn malformed_text_and_closed_json_schemas_are_rejected() {
        assert_rejected("canonical byte mismatch", |collection| {
            collection.write_raw("graph-schema.json", b"{ \"version\": 1 }\n", true);
        });
        assert_rejected("closed-schema error", |collection| {
            collection.mutate_jsonl("records.jsonl", |rows| {
                rows[0]["unknown"] = serde_json::json!(true);
            });
        });
        assert_rejected("terminating LF", |collection| {
            let mut bytes = fs::read(collection.path("queries.jsonl")).unwrap();
            bytes.pop();
            collection.write_raw("queries.jsonl", &bytes, true);
        });
    }

    #[test]
    fn malformed_records_and_graph_schema_are_rejected() {
        assert_rejected("incorrect record order", |collection| {
            collection.mutate_jsonl("records.jsonl", |rows| rows.swap(0, 1));
        });
        assert_rejected("invalid tagged type", |collection| {
            collection.mutate_jsonl("records.jsonl", |rows| {
                rows[0]["fields"]["title"]["type"] = serde_json::json!("bytes");
            });
        });
        assert_rejected("unknown node type", |collection| {
            collection.mutate_json("graph-schema.json", |schema| {
                schema["relationships"][0]["target_node_type"] = serde_json::json!("Missing");
            });
        });
    }

    #[test]
    fn malformed_queries_filters_seeds_and_traversals_are_rejected() {
        assert_rejected("duplicate-free lexical order", |collection| {
            collection.mutate_jsonl("queries.jsonl", |rows| {
                rows[0]["tasks"] = serde_json::json!(["retrieval", "retrieval"]);
            });
        });
        assert_rejected("homogeneous metadata types", |collection| {
            collection.mutate_jsonl("queries.jsonl", |rows| {
                rows[1]["metadata_filter"] = serde_json::json!({
                    "field":"tenant","op":"in","values":[
                        {"type":"integer","value":1},
                        {"type":"string","value":"red"}
                    ]
                });
            });
        });
        assert_rejected("expected node_ids or equals", |collection| {
            collection.mutate_jsonl("queries.jsonl", |rows| {
                rows[1]["explicit_seed"] = serde_json::json!({"kind":"gold_document"});
            });
        });
        assert_rejected("unknown relationship_type", |collection| {
            collection.mutate_jsonl("queries.jsonl", |rows| {
                rows[1]["traversal"]["steps"][0]["relationship_type"] =
                    serde_json::json!("missing");
            });
        });
    }

    #[test]
    fn malformed_judgments_paths_and_exclusions_are_rejected() {
        assert_rejected("expected exactly", |collection| {
            let bytes = fs::read(collection.path("qrels.tsv")).unwrap();
            let malformed = std::str::from_utf8(&bytes).unwrap().replacen(
                "qa 0 alpha 2\n",
                "qa  0 alpha 2\n",
                1,
            );
            collection.write_raw("qrels.tsv", malformed.as_bytes(), true);
        });
        assert_rejected("duplicate-free canonical-byte order", |collection| {
            collection.mutate_jsonl("evidence-judgments.jsonl", |rows| {
                rows[0]["evidence_sets"] =
                    serde_json::json!([["alpha", "beta"], ["alpha", "beta"]]);
            });
        });
        assert_rejected("invalid expected-path record reference", |collection| {
            collection.mutate_jsonl("expected-paths.jsonl", |rows| {
                rows[1]["expected_paths"][0][0]["target_node"]["source"]["record_id"] =
                    serde_json::json!("missing");
            });
        });
        assert_rejected("illegal global versus lane exclusion", |collection| {
            collection.mutate_jsonl("exclusions.jsonl", |rows| {
                rows[0]["lane"] = serde_json::json!("explicit");
            });
        });
    }

    #[test]
    fn malformed_embeddings_counts_and_manifests_are_rejected() {
        assert_rejected("expected finite dimension 3", |collection| {
            collection.mutate_jsonl("corpus-embeddings.f32.jsonl", |rows| {
                rows[0]["values"] = serde_json::json!([1, 0]);
            });
            let digest = sha256(&fs::read(collection.path("corpus-embeddings.f32.jsonl")).unwrap());
            collection.mutate_json("manifests/embedding.json", |manifest| {
                let output = manifest["outputs"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|row| row["path"] == "corpus-embeddings.f32.jsonl")
                    .unwrap();
                output["sha256"] = serde_json::json!(digest);
            });
        });
        assert_rejected("counts.records", |collection| {
            collection.mutate_json("collection.json", |value| {
                value["counts"]["records"] = serde_json::json!(999);
            });
        });
        assert_rejected("transformation DAG outputs", |collection| {
            collection.mutate_json("manifests/chunking.json", |value| {
                value["outputs"] = serde_json::json!([]);
            });
        });
        assert_rejected("judgment_inputs_sha256 expected null", |collection| {
            collection.mutate_json("manifests/graph-construction.json", |value| {
                value["parameters"]["judgment_inputs_sha256"] = serde_json::json!("0".repeat(64));
            });
        });
    }

    #[test]
    fn malformed_seed_resolution_populations_and_split_lock_are_rejected() {
        assert_rejected("resolver query 'qd'", |collection| {
            collection.mutate_json("manifests/seed-policy.json", |value| {
                let policy = value["parameters"]["derived_policies"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|policy| policy["policy_id"] == "topic")
                    .unwrap();
                let aliases = policy["aliases"].as_array_mut().unwrap();
                aliases.remove(0);
                policy["alias_table_sha256"] = serde_json::json!(sha256(
                    canonical_json(&Value::Array(aliases.clone()))
                        .unwrap()
                        .as_bytes()
                ));
            });
        });
        assert_rejected("declared_population_sha256", |collection| {
            collection.mutate_json("manifests/seed-policy.json", |value| {
                value["parameters"]["derived_policies"][0]["declared_population_sha256"] =
                    serde_json::json!("0".repeat(64));
            });
        });
        assert_rejected("source_inventory_sha256 expected", |collection| {
            collection.mutate_json("manifests/split.json", |value| {
                value["parameters"]["source_inventory_sha256"] = serde_json::json!("0".repeat(64));
            });
        });
        assert_rejected("test_lock_sha256 expected", |collection| {
            collection.mutate_json("manifests/split.json", |value| {
                value["parameters"]["test_lock_sha256"] = serde_json::json!("0".repeat(64));
            });
        });
    }
}
