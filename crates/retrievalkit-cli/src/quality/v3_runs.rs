use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::v3_canonical::{canonical_json, sha256};
use super::v3_population::{population_hash, Populations};
use super::v3_schema::{Collection, Query};

#[derive(Debug, Clone)]
pub(super) struct RunIdentity {
    pub run_id: String,
    pub configuration: Value,
    pub configuration_preimage: String,
    pub logical_run_sha256: String,
    pub declared: BTreeSet<String>,
    pub execution: BTreeSet<String>,
}

impl RunIdentity {
    pub(super) fn declared_hash(&self) -> String {
        population_hash(&self.declared)
    }

    pub(super) fn execution_hash(&self) -> String {
        population_hash(&self.execution)
    }
}

#[derive(Debug, Clone)]
pub(super) struct RunContext {
    pub graph_schema_sha256: String,
    pub seed_policy_sha256: String,
    pub implementation_revision: Value,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct HybridConfiguration {
    pub fusion_alpha: f64,
    pub vector_candidate_limit: usize,
    pub keyword_candidate_limit: usize,
}

impl HybridConfiguration {
    pub(super) const fn phase_1_default() -> Self {
        Self {
            fusion_alpha: 0.6,
            vector_candidate_limit: 8,
            keyword_candidate_limit: 8,
        }
    }

    pub(super) fn validate(self, searchable_chunks: usize) -> Result<Self, String> {
        let alpha_f32 = self.fusion_alpha as f32;
        if !self.fusion_alpha.is_finite()
            || !alpha_f32.is_finite()
            || !(0.0..=1.0).contains(&alpha_f32)
        {
            return Err("weighted-hybrid fusion alpha must be a finite F32 in [0,1]".to_owned());
        }
        for (name, value) in [
            ("vector", self.vector_candidate_limit),
            ("keyword", self.keyword_candidate_limit),
        ] {
            if value == 0 || value > searchable_chunks {
                return Err(format!(
                    "weighted-hybrid {name} candidate limit must be in 1..={searchable_chunks}, actual {value}"
                ));
            }
        }
        Ok(self)
    }
}

pub(super) fn canonical_runs(
    collection: &Collection,
    queries: &[Query],
    populations: &Populations,
    context: &RunContext,
) -> Result<Vec<RunIdentity>, String> {
    canonical_runs_with_hybrid_configuration(
        collection,
        queries,
        populations,
        context,
        HybridConfiguration::phase_1_default(),
    )
}

pub(super) fn canonical_runs_with_hybrid_configuration(
    collection: &Collection,
    queries: &[Query],
    populations: &Populations,
    context: &RunContext,
    hybrid: HybridConfiguration,
) -> Result<Vec<RunIdentity>, String> {
    let hybrid = hybrid.validate(collection.counts.chunks)?;
    let by_id = queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut specs = vec![
        (
            "a",
            "whole",
            "semantic",
            "f32",
            "none",
            populations.retrieval.clone(),
            populations.retrieval.clone(),
        ),
        (
            "b",
            "whole",
            "semantic",
            "i8",
            "none",
            populations.retrieval.clone(),
            populations.retrieval.clone(),
        ),
        (
            "c",
            "whole",
            "weighted",
            "i8",
            "none",
            populations.retrieval.clone(),
            populations.retrieval.clone(),
        ),
    ];
    let mut lanes = Vec::new();
    if !populations.explicit.is_empty() {
        lanes.push("explicit".to_owned());
    }
    lanes.extend(populations.derived_declared.keys().cloned());
    for lane in lanes {
        let (declared, successful) = if lane == "explicit" {
            (populations.explicit.clone(), populations.explicit.clone())
        } else {
            (
                populations.derived_declared[&lane].clone(),
                populations.successful(&lane),
            )
        };
        specs.push((
            "d",
            "selection",
            "none",
            "none",
            Box::leak(lane.clone().into_boxed_str()),
            declared.clone(),
            successful.clone(),
        ));
        let retrieval_declared = Populations::intersection(&declared, &populations.retrieval);
        if !retrieval_declared.is_empty() {
            let retrieval_execution =
                Populations::intersection(&successful, &populations.retrieval);
            for (letter, mode, encoding) in [
                ("e", "semantic", "f32"),
                ("f", "semantic", "i8"),
                ("g", "weighted", "i8"),
            ] {
                specs.push((
                    letter,
                    "graph",
                    mode,
                    encoding,
                    Box::leak(lane.clone().into_boxed_str()),
                    retrieval_declared.clone(),
                    retrieval_execution.clone(),
                ));
            }
        }
    }

    let quantization = quantization_policy();
    let quantization_hash = sha256(canonical_json(&quantization)?.as_bytes());
    let normalization = normalization_policy();
    let bm25 = bm25_policy();
    let mut runs = Vec::new();
    let mut ids = BTreeSet::new();
    for (letter, scope, mode, encoding, lane, declared, execution) in specs {
        let retrieval = letter != "d";
        let weighted = matches!(letter, "c" | "g");
        let quantized = matches!(letter, "b" | "c" | "f" | "g");
        let graph = matches!(letter, "d" | "e" | "f" | "g");
        let traversal_hash = if graph {
            Some(traversal_hash(&declared, &by_id)?)
        } else {
            None
        };
        let configuration = json!({
            "bm25_policy": if weighted { bm25.clone() } else { Value::Null },
            "candidate_limits": if weighted { json!({"keyword":hybrid.keyword_candidate_limit,"vector":hybrid.vector_candidate_limit}) } else { json!({"keyword":null,"vector":null}) },
            "collection_id": collection.collection_id,
            "collection_version": collection.collection_version,
            "corpus_id": collection.corpus_id,
            "evaluation_depth": collection.evaluation_depth,
            "fusion_alpha": if weighted { json!(hybrid.fusion_alpha) } else { Value::Null },
            "graph_schema_sha256": if graph { json!(context.graph_schema_sha256) } else { Value::Null },
            "implementation_revision": context.implementation_revision,
            "metadata_filter_policy_id":"v3-query-filter-ast-v1",
            "metric": if retrieval { json!("cosine") } else { Value::Null },
            "normalization": if retrieval { json!("unit_l2") } else { Value::Null },
            "normalization_policy": if retrieval { normalization.clone() } else { Value::Null },
            "quantization_policy_sha256": if quantized { json!(quantization_hash) } else { Value::Null },
            "retrieval_mode":mode,
            "run_letter":letter,
            "schema_version":3,
            "scope":scope,
            "seed_lane":lane,
            "seed_policy_sha256": if graph { json!(context.seed_policy_sha256) } else { Value::Null },
            "top_k":collection.top_k,
            "traversal_policy_sha256":traversal_hash,
            "vector_encoding":encoding
        });
        let configuration_preimage = canonical_json(&configuration)?;
        let configuration_hash = sha256(configuration_preimage.as_bytes());
        let seed = if lane == "none" { "na" } else { lane };
        let run_id = format!(
            "v3-{letter}-{scope}-{mode}-{encoding}-{seed}-cfg-{}",
            &configuration_hash[..12]
        );
        if run_id.len() > 96
            || !run_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(format!(
                "run configuration produced invalid run ID '{run_id}'"
            ));
        }
        if !ids.insert(run_id.clone()) {
            return Err(format!("run-ID collision for '{run_id}'"));
        }
        let mut logical = configuration.clone();
        logical
            .as_object_mut()
            .expect("configuration is an object")
            .remove("implementation_revision");
        let logical_run_sha256 = sha256(canonical_json(&logical)?.as_bytes());
        runs.push(RunIdentity {
            run_id,
            configuration,
            configuration_preimage,
            logical_run_sha256,
            declared,
            execution,
        });
    }
    runs.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    Ok(runs)
}

fn traversal_hash(
    population: &BTreeSet<String>,
    queries: &BTreeMap<&str, &Query>,
) -> Result<String, String> {
    let mut values = Vec::new();
    for query_id in population {
        let query = queries
            .get(query_id.as_str())
            .ok_or_else(|| format!("run population references unknown query '{query_id}'"))?;
        values.push(json!({"query_id":query_id,"traversal":query.traversal}));
    }
    Ok(sha256(canonical_json(&Value::Array(values))?.as_bytes()))
}

pub(super) fn quantization_policy() -> Value {
    json!({"arithmetic":"ieee754_f32_each_operation","clamp_max":127,"clamp_min":-128,"dot_accumulator":"signed_i32_exact","encoding_expression":"value_times_reciprocal_scale","kind":"symmetric_per_vector_i8","rounding":"half_away_from_zero","scale_divisor":127,"score_expression":"f32_i32_dot_times_query_scale_times_chunk_scale","zero_vector_scale":0})
}

pub(super) fn normalization_policy() -> Value {
    json!({"arithmetic":"ieee754_f32_each_operation","input":"source_f32","inverse_norm":"sqrt_then_reciprocal","kind":"unit_l2_before_encoding","reduction":"index_order_left_to_right","sqrt":"correctly_rounded_f32","zero_vector":"unchanged"})
}

pub(super) fn bm25_policy() -> Value {
    json!({"b":0.75,"k1":1.2,"lowercase":"rust_str_to_lowercase","stop_words":[],"tokenizer_id":"unicode-segmentation-unicode_words","tokenizer_library_sha256":"c6f5d3c3b1bf09027a88a6bc961fc00497d651009560b5463668dc81b0fa87a8","tokenizer_version":"1.13.3","unicode_lowercase_tables_sha256":"480dea577027cc707c769048f775be3aafff871a74c41efcbe0eff8314f269fc","unicode_version":"17.0.0"})
}
