use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vectorkit_core::{
    ChunkIdentity, ChunkKey, CorpusId, ExactVectorIndex, FieldName, Filter, HybridQuery,
    IndexConfig, KeywordQuery, Metadata, Record, RecordChunkInput, RecordId, RecordType,
    RecordValue, RetrievalConfiguration, RetrievalDatabase, SearchQuery, VectorEncoding,
    VectorMetric,
};
use vectorkit_graph::{
    Cardinality, ChunkNodeSchema, Direction, DuplicateReferencePolicy, FieldPath,
    GraphRetrievalDatabase, GraphSchema, MissingTargetPolicy, NodeId, NodeType, QueryLimits,
    RecordNodeSchema, RelationshipSchema, RelationshipType, Seed, Traverse,
};

mod device;
mod measurement;

pub use device::run_device_query_session_json;

const DIMENSION: usize = 384;
const CHUNKS_PER_RECORD: usize = 4;
const DELETED_CHUNKS_PER_10K: usize = 100;
const TOP_K: usize = 10;
const QUERY_CATEGORIES: [&str; 8] = [
    "semantic",
    "exact_name",
    "hybrid",
    "metadata_filter",
    "graph_1hop",
    "graph_2hop",
    "graph_3hop",
    "graph_filter",
];
const FIXTURE_MAGIC: &[u8] = b"VECTORKIT-PHASE4-V1\0";
const POLICY_DESCRIPTOR: &str = concat!(
    "vectorkit-phase4-device-v1;dimension=384;chunks_per_record=4;",
    "deleted_chunks_per_10k=100;seed=0x9e3779b97f4a7c15;",
    "normalized_xorshift64star_f32_le;records_active_then_deleted;",
    "refs=next+links(+7,+13,+13)+optional_except_mod5;",
    "metadata=tenant_mod4+category_mod8+ordinal;distractor=target_xor_low_bits;",
    "queries=semantic,exact_name,hybrid,metadata_filter,graph_1hop,",
    "graph_2hop,graph_3hop,graph_filter;top_k=10;",
    "result_policy=f32_i8_target_top1_and_stable_graph_selection"
);
const DEFAULT_DEVICE_SAFE_MEMORY_MIB: u64 = 1_536;
const DEFAULT_DEVICE_SAFE_STORAGE_MIB: u64 = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkloadClass {
    Supported,
    Stress,
}

impl WorkloadClass {
    fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported_product",
            Self::Stress => "stress",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkloadSpec {
    id: &'static str,
    active_chunks: usize,
    class: WorkloadClass,
}

impl WorkloadSpec {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "10k-384d-v3" => Ok(Self::new(value, 10_000, WorkloadClass::Supported)),
            "25k-384d-v3" => Ok(Self::new(value, 25_000, WorkloadClass::Supported)),
            "50k-384d-v3" => Ok(Self::new(value, 50_000, WorkloadClass::Supported)),
            "100k-384d-v3-stress" => Ok(Self::new(value, 100_000, WorkloadClass::Stress)),
            _ => Err(format!(
                "unknown Phase 4 workload '{value}'; expected 10k-384d-v3, 25k-384d-v3, 50k-384d-v3, or 100k-384d-v3-stress"
            )),
        }
    }

    fn new(id: &str, active_chunks: usize, class: WorkloadClass) -> Self {
        let id = match id {
            "10k-384d-v3" => "10k-384d-v3",
            "25k-384d-v3" => "25k-384d-v3",
            "50k-384d-v3" => "50k-384d-v3",
            "100k-384d-v3-stress" => "100k-384d-v3-stress",
            _ => unreachable!(),
        };
        Self {
            id,
            active_chunks,
            class,
        }
    }

    fn active_records(self) -> usize {
        self.active_chunks / CHUNKS_PER_RECORD
    }

    fn deleted_chunks(self) -> usize {
        self.active_chunks / 10_000 * DELETED_CHUNKS_PER_10K
    }

    fn deleted_records(self) -> usize {
        self.deleted_chunks() / CHUNKS_PER_RECORD
    }

    fn nodes(self) -> usize {
        self.active_records() + self.active_chunks
    }

    fn edges(self) -> usize {
        // Owns/owned-by: two per chunk. Next/previous: two per record.
        // Links/linked-by: four per record after duplicate-reference removal.
        // Optional/optional-by: two for the four records out of every five that set it.
        self.active_chunks * 2
            + self.active_records() * 6
            + (self.active_records() - self.active_records().div_ceil(5)) * 2
    }

    fn validate(self) -> Result<(), String> {
        if !self.active_chunks.is_multiple_of(CHUNKS_PER_RECORD)
            || !self.deleted_chunks().is_multiple_of(CHUNKS_PER_RECORD)
        {
            return Err("Phase 4 workload counts must align to four chunks per record".to_owned());
        }
        if self.id == "100k-384d-v3-stress"
            && (self.active_chunks != 100_000 || self.class != WorkloadClass::Stress)
        {
            return Err(
                "100K workload must contain exactly 100,000 active chunks and be labeled stress"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClosedManifest {
    schema_version: u32,
    workload_id: String,
    classification: String,
    supported_v1_capacity_changed: bool,
    policy_sha256: String,
    dimension: usize,
    source_embedding_encoding: String,
    retrieval_configurations: Vec<String>,
    top_k: usize,
    active_records: usize,
    deleted_records: usize,
    generated_records: usize,
    active_chunks: usize,
    deleted_chunks: usize,
    generated_chunks: usize,
    graph_nodes: usize,
    graph_edges: usize,
    query_categories: Vec<String>,
    result_policy: String,
    fixture: ArtifactDigest,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDigest {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct GenerationResponse {
    ok: bool,
    workload_id: String,
    classification: String,
    fixture_path: String,
    fixture_bytes: u64,
    fixture_sha256: String,
    manifest_path: String,
    manifest_bytes: u64,
    manifest_sha256: String,
    repeated_generation_required: bool,
}

#[derive(Debug, Serialize)]
struct ValidationResponse {
    ok: bool,
    workload_id: String,
    classification: String,
    active_records: usize,
    deleted_records: usize,
    active_chunks: usize,
    deleted_chunks: usize,
    graph_nodes: usize,
    graph_edges: usize,
    fixture_sha256: String,
    manifest_sha256: String,
}

pub fn run_cli(args: &[String]) -> Result<String, String> {
    let (action, rest) = args.split_first().ok_or_else(|| usage().to_owned())?;
    match action.as_str() {
        "generate" => {
            let workload = WorkloadSpec::parse(required_flag(rest, "--workload")?)?;
            let output = PathBuf::from(required_flag(rest, "--output")?);
            reject_unknown_flags(rest, &["--workload", "--output"])?;
            pretty(&generate_fixture(workload, &output)?)
        }
        "validate" => {
            let manifest = PathBuf::from(required_flag(rest, "--manifest")?);
            reject_unknown_flags(rest, &["--manifest"])?;
            pretty(&validate_fixture(&manifest)?)
        }
        "mac-correctness" => {
            let workload = WorkloadSpec::parse(required_flag(rest, "--workload")?)?;
            let output = PathBuf::from(required_flag(rest, "--output")?);
            reject_unknown_flags(rest, &["--workload", "--output"])?;
            pretty(&run_mac_correctness(workload, &output)?)
        }
        "preflight" => {
            let manifest = PathBuf::from(required_flag(rest, "--manifest")?);
            let persisted_report = optional_flag(rest, "--mac-report").map(PathBuf::from);
            let safe_memory_mib = optional_flag(rest, "--safe-memory-mib")
                .map(parse_u64)
                .transpose()?
                .unwrap_or(DEFAULT_DEVICE_SAFE_MEMORY_MIB);
            let safe_storage_mib = optional_flag(rest, "--safe-storage-mib")
                .map(parse_u64)
                .transpose()?
                .unwrap_or(DEFAULT_DEVICE_SAFE_STORAGE_MIB);
            reject_unknown_flags(
                rest,
                &[
                    "--manifest",
                    "--mac-report",
                    "--safe-memory-mib",
                    "--safe-storage-mib",
                ],
            )?;
            pretty(&preflight(
                &manifest,
                persisted_report.as_deref(),
                safe_memory_mib,
                safe_storage_mib,
            )?)
        }
        "measure-stages" => {
            let workload = WorkloadSpec::parse(required_flag(rest, "--workload")?)?;
            let input = PathBuf::from(required_flag(rest, "--input")?);
            let output = PathBuf::from(required_flag(rest, "--output")?);
            reject_unknown_flags(rest, &["--workload", "--input", "--output"])?;
            pretty(&measurement::run(workload, &input, &output)?)
        }
        _ => Err(usage().to_owned()),
    }
}

fn usage() -> &'static str {
    "usage: vectorkit bench phase4 <generate|validate|mac-correctness|measure-stages|preflight> ..."
}

fn required_flag<'a>(args: &'a [String], flag: &str) -> Result<&'a str, String> {
    optional_flag(args, flag).ok_or_else(|| format!("missing required argument '{flag}'"))
}

fn optional_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|value| value == flag)
        .and_then(|offset| args.get(offset + 1))
        .map(String::as_str)
}

fn reject_unknown_flags(args: &[String], expected: &[&str]) -> Result<(), String> {
    if !args.len().is_multiple_of(2) {
        return Err("Phase 4 arguments must be flag/value pairs".to_owned());
    }
    for pair in args.chunks_exact(2) {
        if !expected.contains(&pair[0].as_str()) {
            return Err(format!("unknown Phase 4 argument '{}'", pair[0]));
        }
    }
    Ok(())
}

fn parse_u64(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("'{value}' must be a non-negative integer"))
}

fn pretty<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

fn generate_fixture(spec: WorkloadSpec, output: &Path) -> Result<GenerationResponse, String> {
    spec.validate()?;
    fs::create_dir_all(output)
        .map_err(|error| format!("failed to create '{}': {error}", output.display()))?;
    let fixture_path = output.join("fixture.bin");
    let manifest_path = output.join("manifest.json");
    let fixture_file = File::create(&fixture_path)
        .map_err(|error| format!("failed to create '{}': {error}", fixture_path.display()))?;
    let mut writer = HashWriter::new(BufWriter::new(fixture_file));
    write_fixture(spec, &mut writer)?;
    let (mut fixture_file, fixture_bytes, fixture_sha256) = writer.finish()?;
    fixture_file
        .flush()
        .map_err(|error| format!("failed to flush fixture: {error}"))?;

    let manifest = ClosedManifest {
        schema_version: 1,
        workload_id: spec.id.to_owned(),
        classification: spec.class.label().to_owned(),
        supported_v1_capacity_changed: false,
        policy_sha256: sha256_hex(POLICY_DESCRIPTOR.as_bytes()),
        dimension: DIMENSION,
        source_embedding_encoding: "f32_little_endian".to_owned(),
        retrieval_configurations: vec!["f32".to_owned(), "i8".to_owned()],
        top_k: TOP_K,
        active_records: spec.active_records(),
        deleted_records: spec.deleted_records(),
        generated_records: spec.active_records() + spec.deleted_records(),
        active_chunks: spec.active_chunks,
        deleted_chunks: spec.deleted_chunks(),
        generated_chunks: spec.active_chunks + spec.deleted_chunks(),
        graph_nodes: spec.nodes(),
        graph_edges: spec.edges(),
        query_categories: QUERY_CATEGORIES.iter().map(|value| (*value).to_owned()).collect(),
        result_policy: "F32 and I8 must return the declared stable target identity at rank 1; graph selections, filters, paths, persistence, and reload identities must be exact and encoding-independent".to_owned(),
        fixture: ArtifactDigest {
            path: "fixture.bin".to_owned(),
            bytes: fixture_bytes,
            sha256: fixture_sha256.clone(),
        },
    };
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("failed to encode manifest: {error}"))?;
    manifest_bytes.push(b'\n');
    fs::write(&manifest_path, &manifest_bytes)
        .map_err(|error| format!("failed to write '{}': {error}", manifest_path.display()))?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);

    Ok(GenerationResponse {
        ok: true,
        workload_id: spec.id.to_owned(),
        classification: spec.class.label().to_owned(),
        fixture_path: fixture_path.display().to_string(),
        fixture_bytes,
        fixture_sha256,
        manifest_path: manifest_path.display().to_string(),
        manifest_bytes: manifest_bytes.len() as u64,
        manifest_sha256,
        repeated_generation_required: true,
    })
}

fn write_fixture(spec: WorkloadSpec, writer: &mut impl Write) -> Result<(), String> {
    writer.write_all(FIXTURE_MAGIC).map_err(io_string)?;
    write_string(writer, spec.id)?;
    write_string(writer, spec.class.label())?;
    write_string(writer, &sha256_hex(POLICY_DESCRIPTOR.as_bytes()))?;
    write_u64(writer, spec.active_records() as u64)?;
    write_u64(writer, spec.deleted_records() as u64)?;
    write_u64(writer, spec.active_chunks as u64)?;
    write_u64(writer, spec.deleted_chunks() as u64)?;
    write_u64(writer, spec.nodes() as u64)?;
    write_u64(writer, spec.edges() as u64)?;
    write_u32(writer, DIMENSION as u32)?;
    write_u32(writer, QUERY_CATEGORIES.len() as u32)?;
    for category in QUERY_CATEGORIES {
        write_string(writer, category)?;
    }
    for record_index in 0..spec.active_records() {
        write_fixture_record(writer, spec, record_index, false)?;
    }
    for deleted_index in 0..spec.deleted_records() {
        write_fixture_record(writer, spec, deleted_index, true)?;
    }
    Ok(())
}

fn write_fixture_record(
    writer: &mut impl Write,
    spec: WorkloadSpec,
    record_index: usize,
    deleted: bool,
) -> Result<(), String> {
    let id = record_id(record_index, deleted);
    write_string(writer, &id)?;
    writer.write_all(&[u8::from(deleted)]).map_err(io_string)?;
    write_u64(writer, record_index as u64)?;
    let refs = reference_indices(record_index, spec.active_records());
    write_u32(writer, refs.len() as u32)?;
    for target in refs {
        write_string(writer, &record_id(target, false))?;
    }
    write_u32(writer, CHUNKS_PER_RECORD as u32)?;
    for chunk_index in 0..CHUNKS_PER_RECORD {
        write_string(writer, &chunk_key(chunk_index))?;
        write_string(writer, &chunk_text(record_index, chunk_index, deleted))?;
        write_u32(writer, (record_index % 4) as u32)?;
        write_u32(
            writer,
            ((record_index * CHUNKS_PER_RECORD + chunk_index) % 8) as u32,
        )?;
        for value in source_embedding(record_index, chunk_index, deleted) {
            writer.write_all(&value.to_le_bytes()).map_err(io_string)?;
        }
    }
    Ok(())
}

struct HashWriter<W> {
    inner: W,
    hash: Sha256,
    bytes: u64,
}

impl<W> HashWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hash: Sha256::new(),
            bytes: 0,
        }
    }
}

impl<W: Write> HashWriter<W> {
    fn finish(self) -> Result<(W, u64, String), String> {
        Ok((
            self.inner,
            self.bytes,
            format!("{:x}", self.hash.finalize()),
        ))
    }
}

impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.hash.update(&buffer[..written]);
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn write_string(writer: &mut impl Write, value: &str) -> Result<(), String> {
    write_u32(writer, value.len() as u32)?;
    writer.write_all(value.as_bytes()).map_err(io_string)
}

fn write_u32(writer: &mut impl Write, value: u32) -> Result<(), String> {
    writer.write_all(&value.to_le_bytes()).map_err(io_string)
}

fn write_u64(writer: &mut impl Write, value: u64) -> Result<(), String> {
    writer.write_all(&value.to_le_bytes()).map_err(io_string)
}

fn io_string(error: std::io::Error) -> String {
    error.to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_fixture(manifest_path: &Path) -> Result<ValidationResponse, String> {
    let manifest_bytes = fs::read(manifest_path)
        .map_err(|error| format!("failed to read '{}': {error}", manifest_path.display()))?;
    let manifest: ClosedManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("invalid closed manifest: {error}"))?;
    let spec = WorkloadSpec::parse(&manifest.workload_id)?;
    spec.validate()?;
    validate_manifest_fields(&manifest, spec)?;

    let fixture_path = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&manifest.fixture.path);
    let metadata = fs::metadata(&fixture_path)
        .map_err(|error| format!("failed to inspect '{}': {error}", fixture_path.display()))?;
    if metadata.len() != manifest.fixture.bytes {
        return Err(format!(
            "fixture byte size {} does not match closed manifest {}",
            metadata.len(),
            manifest.fixture.bytes
        ));
    }
    let actual_fixture_sha = sha256_file(&fixture_path)?;
    if actual_fixture_sha != manifest.fixture.sha256 {
        return Err("fixture SHA-256 does not match closed manifest".to_owned());
    }
    independently_validate_fixture_bytes(&fixture_path, spec)?;
    Ok(ValidationResponse {
        ok: true,
        workload_id: spec.id.to_owned(),
        classification: spec.class.label().to_owned(),
        active_records: spec.active_records(),
        deleted_records: spec.deleted_records(),
        active_chunks: spec.active_chunks,
        deleted_chunks: spec.deleted_chunks(),
        graph_nodes: spec.nodes(),
        graph_edges: spec.edges(),
        fixture_sha256: actual_fixture_sha,
        manifest_sha256: sha256_hex(&manifest_bytes),
    })
}

fn validate_manifest_fields(manifest: &ClosedManifest, spec: WorkloadSpec) -> Result<(), String> {
    let expected_categories = QUERY_CATEGORIES
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    if manifest.schema_version != 1
        || manifest.classification != spec.class.label()
        || manifest.supported_v1_capacity_changed
        || manifest.policy_sha256 != sha256_hex(POLICY_DESCRIPTOR.as_bytes())
        || manifest.dimension != DIMENSION
        || manifest.source_embedding_encoding != "f32_little_endian"
        || manifest.retrieval_configurations != ["f32", "i8"]
        || manifest.top_k != TOP_K
        || manifest.active_records != spec.active_records()
        || manifest.deleted_records != spec.deleted_records()
        || manifest.generated_records != spec.active_records() + spec.deleted_records()
        || manifest.active_chunks != spec.active_chunks
        || manifest.deleted_chunks != spec.deleted_chunks()
        || manifest.generated_chunks != spec.active_chunks + spec.deleted_chunks()
        || manifest.graph_nodes != spec.nodes()
        || manifest.graph_edges != spec.edges()
        || manifest.query_categories != expected_categories
        || manifest.fixture.path != "fixture.bin"
    {
        return Err(
            "closed manifest does not match the immutable Phase 4 workload policy".to_owned(),
        );
    }
    if spec.id == "100k-384d-v3-stress" && manifest.classification != "stress" {
        return Err("100K results must be labeled stress".to_owned());
    }
    if spec.id == "100k-384d-v3-stress"
        && (manifest.classification.contains("supported")
            || manifest
                .result_policy
                .to_ascii_lowercase()
                .contains("marketing"))
    {
        return Err("100K manifest cannot be a supported-product or marketing result".to_owned());
    }
    Ok(())
}

fn independently_validate_fixture_bytes(path: &Path, spec: WorkloadSpec) -> Result<(), String> {
    let mut reader = BufReader::new(
        File::open(path).map_err(|error| format!("failed to open fixture: {error}"))?,
    );
    let mut magic = vec![0u8; FIXTURE_MAGIC.len()];
    reader.read_exact(&mut magic).map_err(io_string)?;
    if magic != FIXTURE_MAGIC {
        return Err("fixture magic/version mismatch".to_owned());
    }
    expect_string(&mut reader, spec.id, "workload ID")?;
    expect_string(&mut reader, spec.class.label(), "classification")?;
    expect_string(
        &mut reader,
        &sha256_hex(POLICY_DESCRIPTOR.as_bytes()),
        "policy SHA-256",
    )?;
    expect_u64(&mut reader, spec.active_records() as u64, "active records")?;
    expect_u64(
        &mut reader,
        spec.deleted_records() as u64,
        "deleted records",
    )?;
    expect_u64(&mut reader, spec.active_chunks as u64, "active chunks")?;
    expect_u64(&mut reader, spec.deleted_chunks() as u64, "deleted chunks")?;
    expect_u64(&mut reader, spec.nodes() as u64, "graph nodes")?;
    expect_u64(&mut reader, spec.edges() as u64, "graph edges")?;
    expect_u32(&mut reader, DIMENSION as u32, "dimension")?;
    expect_u32(
        &mut reader,
        QUERY_CATEGORIES.len() as u32,
        "query category count",
    )?;
    for category in QUERY_CATEGORIES {
        expect_string(&mut reader, category, "query category")?;
    }
    for record_index in 0..spec.active_records() {
        validate_fixture_record(&mut reader, spec, record_index, false)?;
    }
    for deleted_index in 0..spec.deleted_records() {
        validate_fixture_record(&mut reader, spec, deleted_index, true)?;
    }
    let mut trailing = [0u8; 1];
    if reader.read(&mut trailing).map_err(io_string)? != 0 {
        return Err("fixture contains trailing bytes".to_owned());
    }
    Ok(())
}

fn validate_fixture_record(
    reader: &mut impl Read,
    spec: WorkloadSpec,
    record_index: usize,
    deleted: bool,
) -> Result<(), String> {
    expect_string(reader, &record_id(record_index, deleted), "record ID")?;
    let mut state = [0u8; 1];
    reader.read_exact(&mut state).map_err(io_string)?;
    if state[0] != u8::from(deleted) {
        return Err("fixture active/deleted state mismatch".to_owned());
    }
    expect_u64(reader, record_index as u64, "record ordinal")?;
    let references = reference_indices(record_index, spec.active_records());
    expect_u32(reader, references.len() as u32, "reference count")?;
    for target in references {
        expect_string(reader, &record_id(target, false), "reference target")?;
    }
    expect_u32(reader, CHUNKS_PER_RECORD as u32, "chunks per record")?;
    for chunk_index in 0..CHUNKS_PER_RECORD {
        expect_string(reader, &chunk_key(chunk_index), "chunk key")?;
        expect_string(
            reader,
            &chunk_text(record_index, chunk_index, deleted),
            "chunk text",
        )?;
        expect_u32(reader, (record_index % 4) as u32, "tenant bucket")?;
        expect_u32(
            reader,
            ((record_index * CHUNKS_PER_RECORD + chunk_index) % 8) as u32,
            "category bucket",
        )?;
        let expected = source_embedding(record_index, chunk_index, deleted);
        for expected_value in expected {
            let mut bytes = [0u8; 4];
            reader.read_exact(&mut bytes).map_err(io_string)?;
            if bytes != expected_value.to_le_bytes() {
                return Err(format!(
                    "source embedding mismatch for {}:{}",
                    record_id(record_index, deleted),
                    chunk_key(chunk_index)
                ));
            }
        }
    }
    Ok(())
}

fn read_u32(reader: &mut impl Read) -> Result<u32, String> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes).map_err(io_string)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> Result<u64, String> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes).map_err(io_string)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_string(reader: &mut impl Read) -> Result<String, String> {
    let len = read_u32(reader)? as usize;
    if len > 1_048_576 {
        return Err("fixture string exceeds validation bound".to_owned());
    }
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes).map_err(io_string)?;
    String::from_utf8(bytes).map_err(|_| "fixture string is not UTF-8".to_owned())
}

fn expect_string(reader: &mut impl Read, expected: &str, label: &str) -> Result<(), String> {
    let actual = read_string(reader)?;
    if actual != expected {
        return Err(format!("fixture {label} mismatch"));
    }
    Ok(())
}

fn expect_u32(reader: &mut impl Read, expected: u32, label: &str) -> Result<(), String> {
    if read_u32(reader)? != expected {
        return Err(format!("fixture {label} mismatch"));
    }
    Ok(())
}

fn expect_u64(reader: &mut impl Read, expected: u64, label: &str) -> Result<(), String> {
    if read_u64(reader)? != expected {
        return Err(format!("fixture {label} mismatch"));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut reader = BufReader::new(
        File::open(path)
            .map_err(|error| format!("failed to open '{}': {error}", path.display()))?,
    );
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer).map_err(io_string)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn record_id(index: usize, deleted: bool) -> String {
    if deleted {
        format!("deleted-{index:08}")
    } else {
        format!("record-{index:08}")
    }
}

fn chunk_key(index: usize) -> String {
    format!("chunk-{index:02}")
}

fn chunk_text(record_index: usize, chunk_index: usize, deleted: bool) -> String {
    let state = if deleted { "deleted" } else { "active" };
    format!(
        "{state} identity{record_index:08} section{chunk_index:02} tenant{} category{} deterministic local retrieval distractor{}",
        record_index % 4,
        (record_index * CHUNKS_PER_RECORD + chunk_index) % 8,
        record_index ^ 0x55aa
    )
}

fn reference_indices(record_index: usize, active_records: usize) -> Vec<usize> {
    let mut references = vec![
        (record_index + 1) % active_records,
        (record_index + 7) % active_records,
        (record_index + 13) % active_records,
        (record_index + 13) % active_records,
    ];
    if !record_index.is_multiple_of(5) {
        references.push((record_index + 3) % active_records);
    }
    references
}

fn source_embedding(record_index: usize, chunk_index: usize, deleted: bool) -> Vec<f32> {
    let vector_id = ((record_index as u64) << 3) | ((chunk_index as u64) << 1) | u64::from(deleted);
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ vector_id.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let mut values = Vec::with_capacity(DIMENSION);
    for _ in 0..DIMENSION {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let bits = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        values.push(((bits >> 40) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0);
    }
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    for value in &mut values {
        *value /= norm;
    }
    values
}

#[derive(Debug, Serialize, Deserialize)]
struct MacCorrectnessReport {
    schema_version: u32,
    workload_id: String,
    classification: String,
    status: String,
    host: HostInfo,
    active_records: usize,
    deleted_records: usize,
    active_chunks: usize,
    deleted_chunks: usize,
    graph_nodes: usize,
    graph_edges: usize,
    configurations: Vec<EncodingCorrectnessReport>,
    device_preflight: DevicePreflight,
    supported_v1_capacity_changed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct HostInfo {
    required: String,
    detected: String,
    required_host_match: bool,
    architecture: String,
    release_build: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncodingCorrectnessReport {
    encoding: String,
    status: String,
    persisted_total_bytes: u64,
    persisted_retrieval_bytes: u64,
    persisted_graph_bytes: u64,
    persisted_components: PersistedComponents,
    loaded_payload_estimate_bytes: u64,
    estimated_peak_memory_bytes: u64,
    stages: Vec<StageResult>,
    checks: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StageResult {
    stage: String,
    elapsed_nanoseconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedComponents {
    corpus_chunks_bytes: u64,
    vectors_quantization_bytes: u64,
    lexical_bm25_bytes: u64,
    graph_schema_bytes: u64,
    manifest_validation_bytes: u64,
    complete_directory_bytes: u64,
    component_sum_matches_directory: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DevicePreflight {
    safe_memory_budget_bytes: u64,
    safe_storage_budget_bytes: u64,
    estimated_peak_memory_bytes: u64,
    persisted_f32_bytes: u64,
    persisted_i8_bytes: u64,
    safe_to_attempt: bool,
    unsafe_result_status: String,
    required_process_protocol: String,
}

fn run_mac_correctness(spec: WorkloadSpec, output: &Path) -> Result<MacCorrectnessReport, String> {
    spec.validate()?;
    let detected = detected_mac_model();
    let host_match = detected.contains("M1 Max");
    if !host_match {
        return Err(format!(
            "Phase 4a correctness requires Apple M1 Max; detected '{detected}'"
        ));
    }
    if cfg!(debug_assertions) {
        return Err("Phase 4a correctness must run with an optimized release build".to_owned());
    }
    fs::create_dir_all(output)
        .map_err(|error| format!("failed to create '{}': {error}", output.display()))?;

    let mut configurations = Vec::new();
    for (label, encoding) in [
        ("f32", VectorEncoding::F32),
        ("i8", VectorEncoding::I8ScalarQuantized),
    ] {
        configurations.push(run_encoding_correctness(spec, label, encoding, output)?);
    }
    let preflight = preflight_from_rows(
        &configurations,
        DEFAULT_DEVICE_SAFE_MEMORY_MIB,
        DEFAULT_DEVICE_SAFE_STORAGE_MIB,
    )?;
    let report = MacCorrectnessReport {
        schema_version: 1,
        workload_id: spec.id.to_owned(),
        classification: spec.class.label().to_owned(),
        status: "passed".to_owned(),
        host: HostInfo {
            required: "Apple M1 Max".to_owned(),
            detected,
            required_host_match: host_match,
            architecture: std::env::consts::ARCH.to_owned(),
            release_build: !cfg!(debug_assertions),
        },
        active_records: spec.active_records(),
        deleted_records: spec.deleted_records(),
        active_chunks: spec.active_chunks,
        deleted_chunks: spec.deleted_chunks(),
        graph_nodes: spec.nodes(),
        graph_edges: spec.edges(),
        configurations,
        device_preflight: preflight,
        supported_v1_capacity_changed: false,
    };
    let report_path = output.join("mac-correctness-report.json");
    let mut bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to encode Mac correctness report: {error}"))?;
    bytes.push(b'\n');
    fs::write(&report_path, bytes)
        .map_err(|error| format!("failed to write '{}': {error}", report_path.display()))?;
    Ok(report)
}

fn run_encoding_correctness(
    spec: WorkloadSpec,
    label: &str,
    encoding: VectorEncoding,
    output: &Path,
) -> Result<EncodingCorrectnessReport, String> {
    let mut stages = Vec::new();
    let started = Instant::now();
    let retrieval = build_retrieval_database(spec, encoding)?;
    stages.push(stage("build_corpus_and_retrieval", started));

    let started = Instant::now();
    let database = GraphRetrievalDatabase::build(retrieval, phase4_graph_schema()?)
        .map_err(|error| format!("failed to build Phase 4 graph: {error}"))?;
    validate_database_shape(&database, spec, encoding)?;
    stages.push(stage("build_graph", started));

    let started = Instant::now();
    let checks = validate_database_behavior(&database, spec)?;
    stages.push(stage("correctness_queries", started));

    let persisted = output.join(label);
    if persisted.exists() {
        return Err(format!(
            "refusing to overwrite existing Phase 4 persistence directory '{}'",
            persisted.display()
        ));
    }
    let started = Instant::now();
    let graph_sizes = database
        .save_to_dir(&persisted)
        .map_err(|error| format!("failed to persist {label} database: {error}"))?;
    stages.push(stage("save", started));

    let started = Instant::now();
    GraphRetrievalDatabase::validate_dir(&persisted)
        .map_err(|error| format!("independent persisted validation failed for {label}: {error}"))?;
    stages.push(stage("read_only_validation", started));

    let loaded_payload_estimate = database
        .retrieval()
        .as_compatibility_index()
        .size_estimate()
        .total_bytes() as u64;
    drop(database);

    let started = Instant::now();
    let loaded = GraphRetrievalDatabase::load_from_dir(&persisted)
        .map_err(|error| format!("failed to reload {label} database: {error}"))?;
    stages.push(stage("cold_load", started));
    let started = Instant::now();
    validate_database_shape(&loaded, spec, encoding)?;
    let reloaded_checks = validate_database_behavior(&loaded, spec)?;
    if checks != reloaded_checks {
        return Err(format!("{label} reload correctness check set changed"));
    }
    stages.push(stage("cold_load_replay", started));
    drop(loaded);

    let started = Instant::now();
    let warm_loaded = GraphRetrievalDatabase::load_from_dir(&persisted)
        .map_err(|error| format!("failed to warm-reload {label} database: {error}"))?;
    stages.push(stage("warm_load", started));
    let started = Instant::now();
    validate_database_shape(&warm_loaded, spec, encoding)?;
    let warm_checks = validate_database_behavior(&warm_loaded, spec)?;
    if checks != warm_checks {
        return Err(format!("{label} warm reload correctness check set changed"));
    }
    stages.push(stage("warm_load_replay", started));

    let persisted_total_bytes = directory_size(&persisted)?;
    let retrieval_directory = active_capability_directory(&persisted)?.join("retrieval");
    let persisted_retrieval_bytes = directory_size(&retrieval_directory)?;
    let persisted_graph_bytes = graph_sizes.schema_bytes + graph_sizes.graph_bytes;
    let retrieval_sizes = ExactVectorIndex::persisted_file_sizes(&retrieval_directory)
        .map_err(|error| format!("failed to account {label} persisted components: {error}"))?;
    let corpus_chunks_bytes = retrieval_sizes
        .chunks_bytes
        .saturating_add(retrieval_sizes.records_bytes)
        .saturating_add(retrieval_sizes.tombstones_bytes);
    let vectors_quantization_bytes = retrieval_sizes.vectors_bytes;
    let lexical_bm25_bytes = retrieval_sizes.bm25_bytes;
    let graph_schema_bytes = persisted_graph_bytes;
    let known_components = corpus_chunks_bytes
        .saturating_add(vectors_quantization_bytes)
        .saturating_add(lexical_bm25_bytes)
        .saturating_add(graph_schema_bytes);
    let manifest_validation_bytes = persisted_total_bytes
        .checked_sub(known_components)
        .ok_or_else(|| "persisted component accounting exceeds complete directory".to_owned())?;
    let component_sum = known_components.saturating_add(manifest_validation_bytes);
    let persisted_components = PersistedComponents {
        corpus_chunks_bytes,
        vectors_quantization_bytes,
        lexical_bm25_bytes,
        graph_schema_bytes,
        manifest_validation_bytes,
        complete_directory_bytes: persisted_total_bytes,
        component_sum_matches_directory: component_sum == persisted_total_bytes,
    };
    let estimated_peak_memory_bytes = estimate_peak_memory(
        loaded_payload_estimate,
        persisted_graph_bytes,
        spec.active_chunks as u64,
    );
    Ok(EncodingCorrectnessReport {
        encoding: label.to_owned(),
        status: "passed".to_owned(),
        persisted_total_bytes,
        persisted_retrieval_bytes,
        persisted_graph_bytes,
        persisted_components,
        loaded_payload_estimate_bytes: loaded_payload_estimate,
        estimated_peak_memory_bytes,
        stages,
        checks,
    })
}

fn build_retrieval_database(
    spec: WorkloadSpec,
    encoding: VectorEncoding,
) -> Result<RetrievalDatabase, String> {
    let config = IndexConfig::new(DIMENSION, VectorMetric::Cosine).with_vector_encoding(encoding);
    let mut database = RetrievalDatabase::new(
        RetrievalConfiguration::semantic(config),
        CorpusId::new(spec.id).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to create retrieval database: {error}"))?;
    for record_index in 0..spec.active_records() {
        upsert_generated_record(&mut database, spec, record_index, false)?;
    }
    for deleted_index in 0..spec.deleted_records() {
        let id = upsert_generated_record(&mut database, spec, deleted_index, true)?;
        let removed = database.delete_record(&id);
        if removed != CHUNKS_PER_RECORD {
            return Err(format!(
                "deleted record '{}' removed {removed} chunks, expected {CHUNKS_PER_RECORD}",
                id.as_str()
            ));
        }
    }
    Ok(database)
}

fn upsert_generated_record(
    database: &mut RetrievalDatabase,
    spec: WorkloadSpec,
    record_index: usize,
    deleted: bool,
) -> Result<RecordId, String> {
    let id = RecordId::new(record_id(record_index, deleted)).map_err(|error| error.to_string())?;
    let active_records = spec.active_records();
    let fields = BTreeMap::from([
        (
            FieldName::new("name").map_err(|error| error.to_string())?,
            RecordValue::String(format!("identity{record_index:08}")),
        ),
        (
            FieldName::new("next").map_err(|error| error.to_string())?,
            RecordValue::String(record_id((record_index + 1) % active_records, false)),
        ),
        (
            FieldName::new("links").map_err(|error| error.to_string())?,
            RecordValue::List(vec![
                RecordValue::String(record_id((record_index + 7) % active_records, false)),
                RecordValue::String(record_id((record_index + 13) % active_records, false)),
                RecordValue::String(record_id((record_index + 13) % active_records, false)),
            ]),
        ),
        (
            FieldName::new("optional").map_err(|error| error.to_string())?,
            if record_index.is_multiple_of(5) {
                RecordValue::Null
            } else {
                RecordValue::String(record_id((record_index + 3) % active_records, false))
            },
        ),
    ]);
    let record = Record {
        id: id.clone(),
        record_type: RecordType::new("Item").map_err(|error| error.to_string())?,
        fields,
        content: None,
    };
    let inherited_metadata = BTreeMap::from([
        (
            "tenant".to_owned(),
            format!("tenant-{}", record_index % 4).into(),
        ),
        ("record_ordinal".to_owned(), (record_index as i64).into()),
    ]);
    let chunks = (0..CHUNKS_PER_RECORD)
        .map(|chunk_index| {
            Ok(RecordChunkInput {
                key: ChunkKey::new(chunk_key(chunk_index)).map_err(|error| error.to_string())?,
                text: chunk_text(record_index, chunk_index, deleted),
                embedding: source_embedding(record_index, chunk_index, deleted),
                metadata: Metadata::from([
                    (
                        "category".to_owned(),
                        format!(
                            "category-{}",
                            (record_index * CHUNKS_PER_RECORD + chunk_index) % 8
                        )
                        .into(),
                    ),
                    ("chunk_ordinal".to_owned(), (chunk_index as i64).into()),
                ]),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    database
        .upsert_record(record, inherited_metadata, chunks)
        .map_err(|error| format!("failed to upsert '{}': {error}", id.as_str()))?;
    Ok(id)
}

fn phase4_graph_schema() -> Result<GraphSchema, String> {
    let node_type = NodeType::new("Item").map_err(|error| error.to_string())?;
    let relationship = |name: &str,
                        inverse: &str,
                        field: &str,
                        cardinality: Cardinality,
                        duplicate_references: DuplicateReferencePolicy|
     -> Result<RelationshipSchema, String> {
        Ok(RelationshipSchema {
            relationship_type: RelationshipType::new(name).map_err(|error| error.to_string())?,
            source_node_type: node_type.clone(),
            target_node_type: node_type.clone(),
            source_field: FieldPath::single(
                FieldName::new(field).map_err(|error| error.to_string())?,
            ),
            cardinality,
            missing_target: MissingTargetPolicy::Error,
            duplicate_references,
            allow_self_edge: false,
            inverse_relationship: Some(
                RelationshipType::new(inverse).map_err(|error| error.to_string())?,
            ),
        })
    };
    let schema = GraphSchema::new(vec![RecordNodeSchema {
        record_type: RecordType::new("Item").map_err(|error| error.to_string())?,
        node_type: node_type.clone(),
        queryable_fields: vec![FieldPath::single(
            FieldName::new("name").map_err(|error| error.to_string())?,
        )],
    }])
    .with_relationships(vec![
        relationship(
            "Next",
            "Previous",
            "next",
            Cardinality::One,
            DuplicateReferencePolicy::Error,
        )?,
        relationship(
            "Links",
            "LinkedBy",
            "links",
            Cardinality::Many,
            DuplicateReferencePolicy::Deduplicate,
        )?,
        relationship(
            "Optional",
            "OptionalBy",
            "optional",
            Cardinality::OptionalOne,
            DuplicateReferencePolicy::Error,
        )?,
    ])
    .with_chunk_nodes(ChunkNodeSchema {
        node_type: NodeType::new("Chunk").map_err(|error| error.to_string())?,
        owns_relationship: RelationshipType::new("Owns").map_err(|error| error.to_string())?,
        inverse_relationship: Some(
            RelationshipType::new("OwnedBy").map_err(|error| error.to_string())?,
        ),
    });
    schema.validate().map_err(|error| error.to_string())?;
    Ok(schema)
}

fn validate_database_shape(
    database: &GraphRetrievalDatabase,
    spec: WorkloadSpec,
    encoding: VectorEncoding,
) -> Result<(), String> {
    let corpus = database.corpus();
    if corpus.record_store().len() != spec.active_records()
        || corpus.active_chunk_count() != spec.active_chunks
        || corpus.tombstoned_chunk_count() != spec.deleted_chunks()
        || database.graph().node_count() != spec.nodes()
        || database.graph().edge_count() != spec.edges()
        || database.retrieval().retrieval().dimension() != DIMENSION
        || database.retrieval().retrieval().vector_encoding() != encoding
    {
        return Err(format!(
            "database shape mismatch: records {}, active/deleted chunks {}/{}, nodes/edges {}/{}",
            corpus.record_store().len(),
            corpus.active_chunk_count(),
            corpus.tombstoned_chunk_count(),
            database.graph().node_count(),
            database.graph().edge_count()
        ));
    }
    Ok(())
}

fn validate_database_behavior(
    database: &GraphRetrievalDatabase,
    spec: WorkloadSpec,
) -> Result<Vec<String>, String> {
    let target_record = spec.active_records() / 3;
    let target_embedding = source_embedding(target_record, 0, false);
    let expected_record = record_id(target_record, false);

    let semantic = database
        .semantic_search(&SearchQuery::new(target_embedding.clone(), TOP_K))
        .map_err(|error| format!("semantic query failed: {error}"))?;
    expect_top_identity(
        database,
        semantic.first().map(|hit| hit.chunk_id),
        target_record,
        0,
    )?;

    let keyword = database
        .retrieval()
        .as_compatibility_index()
        .keyword_search(&KeywordQuery::new(
            format!("identity{target_record:08}"),
            TOP_K,
        ))
        .map_err(|error| format!("exact-name query failed: {error}"))?;
    expect_top_identity(
        database,
        keyword.first().map(|hit| hit.chunk_id),
        target_record,
        0,
    )?;

    let hybrid = database
        .hybrid_search(&HybridQuery::new(
            format!("identity{target_record:08}"),
            target_embedding.clone(),
            TOP_K,
        ))
        .map_err(|error| format!("hybrid query failed: {error}"))?;
    expect_top_identity(
        database,
        hybrid.first().map(|hit| hit.chunk_id),
        target_record,
        0,
    )?;

    let tenant = format!("tenant-{}", target_record % 4);
    let filtered = database
        .semantic_search(
            &SearchQuery::new(target_embedding, TOP_K).with_filter(Filter::eq("tenant", tenant)),
        )
        .map_err(|error| format!("metadata-filter query failed: {error}"))?;
    expect_top_identity(
        database,
        filtered.first().map(|hit| hit.chunk_id),
        target_record,
        0,
    )?;

    for hops in 1..=3 {
        let result = next_hop_query(database, 0, hops)?;
        let expected = hops % spec.active_records();
        if result.matches.len() != 1
            || result.matches[0].node_id != record_node(expected)?
            || result.matches[0].depth != hops
            || result.matches[0].path.len() != hops
            || result.truncated.is_some()
        {
            return Err(format!("graph {hops}-hop selection/path mismatch"));
        }
    }

    let graph_filter_target = 4usize;
    let selection = next_hop_query(database, 0, graph_filter_target)?;
    let projection = database
        .project_candidate_identities(
            &selection,
            Some(&Filter::eq(
                "tenant",
                format!("tenant-{}", graph_filter_target % 4),
            )),
        )
        .map_err(|error| format!("graph/filter projection failed: {error}"))?;
    if projection.source_nodes != 1
        || projection.projected_chunks_before_filter != CHUNKS_PER_RECORD
        || projection.projected_chunks_after_filter != CHUNKS_PER_RECORD
        || projection.candidates
            != (0..CHUNKS_PER_RECORD)
                .map(|chunk_index| stable_identity(graph_filter_target, chunk_index))
                .collect::<Result<Vec<_>, _>>()?
    {
        return Err("graph selection/filter stable identities mismatch".to_owned());
    }
    let scoped = database
        .semantic_search_in_selection(
            &SearchQuery::new(source_embedding(graph_filter_target, 0, false), TOP_K),
            &selection,
        )
        .map_err(|error| format!("graph-scoped semantic query failed: {error}"))?;
    expect_top_identity(
        database,
        scoped.first().map(|hit| hit.chunk_id),
        graph_filter_target,
        0,
    )?;

    let deleted_id = record_id(0, true);
    for hit_record in semantic
        .iter()
        .map(|hit| hit.document_id.as_str())
        .chain(keyword.iter().map(|hit| hit.document_id.as_str()))
        .chain(hybrid.iter().map(|hit| hit.document_id.as_str()))
        .chain(filtered.iter().map(|hit| hit.document_id.as_str()))
        .chain(scoped.iter().map(|hit| hit.document_id.as_str()))
    {
        if hit_record == deleted_id {
            return Err("deleted identity leaked into a retrieval result".to_owned());
        }
    }
    if semantic.first().map(|hit| hit.document_id.as_str()) != Some(expected_record.as_str()) {
        return Err("semantic stable record identity mismatch".to_owned());
    }

    Ok(QUERY_CATEGORIES
        .iter()
        .map(|category| format!("{category}:passed"))
        .chain([
            "active_deleted_counts:passed".to_owned(),
            "stable_identities:passed".to_owned(),
            "persistence_reload_policy:passed".to_owned(),
        ])
        .collect())
}

fn expect_top_identity(
    database: &GraphRetrievalDatabase,
    chunk_id: Option<u64>,
    record_index: usize,
    chunk_index: usize,
) -> Result<(), String> {
    let chunk_id = chunk_id.ok_or_else(|| "query returned no top hit".to_owned())?;
    let identity = database
        .corpus()
        .chunk_identity(chunk_id)
        .ok_or_else(|| format!("top hit chunk {chunk_id} has no stable identity"))?;
    let expected = stable_identity(record_index, chunk_index)?;
    if identity != &expected {
        return Err(format!(
            "top stable identity mismatch: actual '{}:{}', expected '{}:{}'",
            identity.record_id.as_str(),
            identity.chunk_key.as_str(),
            expected.record_id.as_str(),
            expected.chunk_key.as_str()
        ));
    }
    Ok(())
}

fn stable_identity(record_index: usize, chunk_index: usize) -> Result<ChunkIdentity, String> {
    Ok(ChunkIdentity::new(
        RecordId::new(record_id(record_index, false)).map_err(|error| error.to_string())?,
        ChunkKey::new(chunk_key(chunk_index)).map_err(|error| error.to_string())?,
    ))
}

fn record_node(record_index: usize) -> Result<NodeId, String> {
    Ok(NodeId::record(
        NodeType::new("Item").map_err(|error| error.to_string())?,
        RecordId::new(record_id(record_index, false)).map_err(|error| error.to_string())?,
    ))
}

fn next_hop_query(
    database: &GraphRetrievalDatabase,
    seed_index: usize,
    hops: usize,
) -> Result<vectorkit_graph::GraphResult, String> {
    let query = next_hop_graph_query(seed_index, hops)?;
    database
        .graph_query(&query, None)
        .map_err(|error| format!("graph query failed: {error}"))
}

fn next_hop_graph_query(
    seed_index: usize,
    hops: usize,
) -> Result<vectorkit_graph::GraphQuery, String> {
    Ok(
        vectorkit_graph::GraphQuery::new(Seed::NodeIds(vec![record_node(seed_index)?]))
            .traverse(Traverse {
                relationship: RelationshipType::new("Next").map_err(|error| error.to_string())?,
                direction: Direction::Outgoing,
                min_hops: hops,
                max_hops: hops,
            })
            .with_limits(QueryLimits {
                max_hops: 8,
                max_visited: 128,
                max_results: 16,
                max_working_bytes: 1024 * 1024,
            }),
    )
}

fn stage(name: &str, started: Instant) -> StageResult {
    StageResult {
        stage: name.to_owned(),
        elapsed_nanoseconds: u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
    }
}

fn detected_mac_model() -> String {
    let output = std::process::Command::new("system_profiler")
        .args(["SPHardwareDataType", "-detailLevel", "mini"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines()
                .find_map(|line| line.trim().strip_prefix("Chip:"))
                .map(str::trim)
                .unwrap_or("unknown")
                .to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

fn directory_size(path: &Path) -> Result<u64, String> {
    if path.is_file() {
        return fs::metadata(path)
            .map(|metadata| metadata.len())
            .map_err(|error| format!("failed to inspect '{}': {error}", path.display()));
    }
    let mut total = 0u64;
    for entry in fs::read_dir(path)
        .map_err(|error| format!("failed to list '{}': {error}", path.display()))?
    {
        let entry = entry.map_err(io_string)?;
        total = total
            .checked_add(directory_size(&entry.path())?)
            .ok_or_else(|| "persisted directory size overflow".to_owned())?;
    }
    Ok(total)
}

fn active_capability_directory(root: &Path) -> Result<PathBuf, String> {
    let bytes = fs::read(root.join("manifest.json"))
        .map_err(|error| format!("failed to read graph manifest: {error}"))?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse graph manifest: {error}"))?;
    let snapshot = manifest
        .get("snapshot_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "graph manifest has no snapshot_id".to_owned())?;
    Ok(root.join(".snapshots").join(snapshot))
}

fn estimate_peak_memory(
    retrieval_loaded_bytes: u64,
    persisted_graph_bytes: u64,
    active_chunks: u64,
) -> u64 {
    // Save/reload validation temporarily holds two retrieval generations. Graph build and
    // validation hold nodes, edge payload, forward/reverse CSR, and materialized paths. The
    // fixed margin covers allocator metadata, Swift/FFI state, stage samples, and the process.
    retrieval_loaded_bytes
        .saturating_mul(2)
        .saturating_add(persisted_graph_bytes.saturating_mul(4))
        .saturating_add(active_chunks.saturating_mul(64))
        .saturating_add(96 * 1024 * 1024)
}

fn preflight_from_rows(
    rows: &[EncodingCorrectnessReport],
    safe_memory_mib: u64,
    safe_storage_mib: u64,
) -> Result<DevicePreflight, String> {
    let f32 = rows
        .iter()
        .find(|row| row.encoding == "f32")
        .ok_or_else(|| "preflight requires a persisted F32 row".to_owned())?;
    let i8 = rows
        .iter()
        .find(|row| row.encoding == "i8")
        .ok_or_else(|| "preflight requires a persisted I8 row".to_owned())?;
    let estimated_peak_memory_bytes = rows
        .iter()
        .map(|row| row.estimated_peak_memory_bytes)
        .max()
        .unwrap_or(0);
    let memory_budget = safe_memory_mib.saturating_mul(1024 * 1024);
    let storage_budget = safe_storage_mib.saturating_mul(1024 * 1024);
    let required_storage = f32
        .persisted_total_bytes
        .saturating_add(i8.persisted_total_bytes)
        .saturating_mul(2);
    Ok(DevicePreflight {
        safe_memory_budget_bytes: memory_budget,
        safe_storage_budget_bytes: storage_budget,
        estimated_peak_memory_bytes,
        persisted_f32_bytes: f32.persisted_total_bytes,
        persisted_i8_bytes: i8.persisted_total_bytes,
        safe_to_attempt: estimated_peak_memory_bytes <= memory_budget
            && required_storage <= storage_budget,
        unsafe_result_status: "not_run_memory_safety".to_owned(),
        required_process_protocol: "one workload/configuration per fresh release app process; 1 ms RSS sampling; thermal abort/repeat; five memory repetitions; three final sessions".to_owned(),
    })
}

fn preflight(
    manifest_path: &Path,
    mac_report_path: Option<&Path>,
    safe_memory_mib: u64,
    safe_storage_mib: u64,
) -> Result<DevicePreflight, String> {
    let validation = validate_fixture(manifest_path)?;
    if validation.classification != "stress" && validation.workload_id.contains("100k") {
        return Err("100K preflight requires stress classification".to_owned());
    }
    let report_path = mac_report_path.ok_or_else(|| {
        "100K device preflight requires --mac-report with measured persisted F32/I8 sizes"
            .to_owned()
    })?;
    let bytes = fs::read(report_path)
        .map_err(|error| format!("failed to read '{}': {error}", report_path.display()))?;
    let report: MacCorrectnessReport = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid Mac correctness report: {error}"))?;
    if report.workload_id != validation.workload_id
        || report.classification != validation.classification
        || report.status != "passed"
        || report.supported_v1_capacity_changed
    {
        return Err("Mac report does not match the validated workload/classification".to_owned());
    }
    preflight_from_rows(&report.configurations, safe_memory_mib, safe_storage_mib)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_workload_counts_and_classifications_are_exact() {
        for (id, chunks, class) in [
            ("10k-384d-v3", 10_000, WorkloadClass::Supported),
            ("25k-384d-v3", 25_000, WorkloadClass::Supported),
            ("50k-384d-v3", 50_000, WorkloadClass::Supported),
            ("100k-384d-v3-stress", 100_000, WorkloadClass::Stress),
        ] {
            let spec = WorkloadSpec::parse(id).unwrap();
            assert_eq!(spec.active_chunks, chunks);
            assert_eq!(spec.class, class);
            spec.validate().unwrap();
        }
    }

    #[test]
    fn rejects_adjacent_100k_active_chunk_counts() {
        for active_chunks in [99_999, 100_001] {
            let spec = WorkloadSpec {
                id: "100k-384d-v3-stress",
                active_chunks,
                class: WorkloadClass::Stress,
            };
            assert!(spec.validate().is_err());
        }
    }

    #[test]
    fn source_embeddings_are_byte_stable_and_unit_normalized() {
        let first = source_embedding(12_345, 2, false);
        let second = source_embedding(12_345, 2, false);
        assert_eq!(first, second);
        assert_eq!(first.len(), DIMENSION);
        let norm = first.iter().map(|value| value * value).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.000_01);
    }

    #[test]
    fn edge_formula_matches_policy() {
        let stress = WorkloadSpec::parse("100k-384d-v3-stress").unwrap();
        assert_eq!(stress.active_records(), 25_000);
        assert_eq!(stress.deleted_records(), 250);
        assert_eq!(stress.deleted_chunks(), 1_000);
        assert_eq!(stress.nodes(), 125_000);
        assert_eq!(stress.edges(), 390_000);
    }
}
