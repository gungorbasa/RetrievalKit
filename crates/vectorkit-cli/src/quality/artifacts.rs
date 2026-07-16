use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Serialize;

use super::{deduplicate_documents, validate_trec_id, Fixture, FixtureQuery, RankedDocument};

const STANDARD_CUTOFFS: [usize; 6] = [1, 3, 5, 10, 100, 1000];

#[derive(Debug, Clone, Serialize)]
pub(super) struct ArtifactRun {
    pub(super) run_id: String,
    mode: String,
    encoding: String,
    depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    vector_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keyword_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alpha: Option<f32>,
    queries: Vec<ArtifactQueryResult>,
}

impl ArtifactRun {
    pub(super) fn new(
        run_id: String,
        mode: &str,
        encoding: &str,
        depth: usize,
        query_hits: impl IntoIterator<Item = (String, Vec<RankedDocument>)>,
    ) -> Self {
        let mut queries = query_hits
            .into_iter()
            .map(|(query_id, hits)| {
                let (hits, duplicate_documents_collapsed) = deduplicate_documents(hits);
                ArtifactQueryResult {
                    query_id,
                    hits,
                    duplicate_documents_collapsed,
                }
            })
            .collect::<Vec<_>>();
        queries.sort_by(|left, right| left.query_id.cmp(&right.query_id));
        Self {
            run_id,
            mode: mode.to_owned(),
            encoding: encoding.to_owned(),
            depth,
            vector_candidates: None,
            keyword_candidates: None,
            alpha: None,
            queries,
        }
    }

    pub(super) fn with_candidate_limits(mut self, vector: usize, keyword: usize) -> Self {
        self.vector_candidates = Some(vector);
        self.keyword_candidates = Some(keyword);
        self
    }

    pub(super) fn with_keyword_candidates(mut self, keyword: usize) -> Self {
        self.keyword_candidates = Some(keyword);
        self
    }

    pub(super) fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = Some(alpha);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
struct ArtifactQueryResult {
    query_id: String,
    hits: Vec<RankedDocument>,
    duplicate_documents_collapsed: usize,
}

#[derive(Debug, Serialize)]
struct MetricsArtifact<'a> {
    schema_version: u32,
    fixture_id: &'a str,
    relevance_threshold: u8,
    ndcg_gain: &'static str,
    product_cutoffs: [usize; 3],
    beir_cutoffs: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    f32_exact_reference: Option<ReferenceComparison>,
    runs: Vec<RunMetrics>,
}

#[derive(Debug, Serialize)]
struct RustResultsArtifact<'a> {
    schema_version: u32,
    fixture_id: &'a str,
    runs: &'a [ArtifactRun],
}

#[derive(Debug, Serialize)]
struct RunMetrics {
    run_id: String,
    duplicate_documents_collapsed: usize,
    aggregate: StandardMetrics,
    queries: Vec<QueryMetrics>,
}

#[derive(Debug, Serialize)]
struct ReferenceComparison {
    reference_run_id: String,
    candidate_run_id: String,
    aggregate: RankingAgreement,
    queries: Vec<QueryRankingAgreement>,
}

#[derive(Debug, Default, Serialize)]
struct RankingAgreement {
    recall_at_10: f64,
    recall_at_depth: f64,
    top_1_agreement: f64,
    ordered_result_agreement_at_10: f64,
    ordered_result_agreement: f64,
}

#[derive(Debug, Serialize)]
struct QueryRankingAgreement {
    query_id: String,
    #[serde(flatten)]
    agreement: RankingAgreement,
}

#[derive(Debug, Clone, Default, Serialize)]
struct StandardMetrics {
    ndcg_at_5: f64,
    ndcg_at_10: f64,
    recall_at_5: f64,
    recall_at_10: f64,
    success_at_1: f64,
    precision_at_5: f64,
    mrr_at_10: f64,
    average_precision: f64,
    judged_at_5: f64,
    judged_at_10: f64,
    ndcg_at: BTreeMap<usize, f64>,
    recall_at: BTreeMap<usize, f64>,
    precision_at: BTreeMap<usize, f64>,
}

impl StandardMetrics {
    fn add_assign(&mut self, other: &Self) {
        self.ndcg_at_5 += other.ndcg_at_5;
        self.ndcg_at_10 += other.ndcg_at_10;
        self.recall_at_5 += other.recall_at_5;
        self.recall_at_10 += other.recall_at_10;
        self.success_at_1 += other.success_at_1;
        self.precision_at_5 += other.precision_at_5;
        self.mrr_at_10 += other.mrr_at_10;
        self.average_precision += other.average_precision;
        self.judged_at_5 += other.judged_at_5;
        self.judged_at_10 += other.judged_at_10;
        add_maps(&mut self.ndcg_at, &other.ndcg_at);
        add_maps(&mut self.recall_at, &other.recall_at);
        add_maps(&mut self.precision_at, &other.precision_at);
    }

    fn divide_by(&mut self, divisor: f64) {
        self.ndcg_at_5 /= divisor;
        self.ndcg_at_10 /= divisor;
        self.recall_at_5 /= divisor;
        self.recall_at_10 /= divisor;
        self.success_at_1 /= divisor;
        self.precision_at_5 /= divisor;
        self.mrr_at_10 /= divisor;
        self.average_precision /= divisor;
        self.judged_at_5 /= divisor;
        self.judged_at_10 /= divisor;
        divide_map(&mut self.ndcg_at, divisor);
        divide_map(&mut self.recall_at, divisor);
        divide_map(&mut self.precision_at, divisor);
    }
}

#[derive(Debug, Serialize)]
struct QueryMetrics {
    query_id: String,
    #[serde(flatten)]
    metrics: StandardMetrics,
}

#[derive(Debug, Serialize)]
struct Manifest<'a> {
    schema_version: u32,
    fixture_id: &'a str,
    fixture_schema_version: u32,
    evaluation_depth: usize,
    deterministic_score: &'static str,
    relevance_threshold: u8,
    ndcg_gain: &'static str,
    product_cutoffs: [usize; 3],
    beir_cutoffs: Vec<usize>,
    files: Vec<&'static str>,
    run_ids: Vec<&'a str>,
    model: &'a super::ModelInfo,
    embedding_provenance: &'a super::EmbeddingProvenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    dataset_provenance: Option<&'a super::DatasetProvenance>,
}

pub(super) fn write(path: &Path, fixture: &Fixture, runs: &[ArtifactRun]) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create artifact directory '{}': {error}",
            path.display()
        )
    })?;
    let runs_path = path.join("runs");
    if runs_path.exists() {
        fs::remove_dir_all(&runs_path).map_err(|error| {
            format!(
                "failed to replace artifact run directory '{}': {error}",
                runs_path.display()
            )
        })?;
    }
    fs::create_dir_all(&runs_path)
        .map_err(|error| format!("failed to create '{}': {error}", runs_path.display()))?;

    write_text(&path.join("qrels.tsv"), &qrels_text(fixture)?)?;
    let mut run_ids = BTreeSet::new();
    for run in runs {
        validate_trec_id(&run.run_id, "run ID")?;
        if !run_ids.insert(run.run_id.as_str()) {
            return Err(format!("duplicate artifact run ID '{}'", run.run_id));
        }
        write_text(
            &runs_path.join(format!("{}.trec", run.run_id)),
            &trec_run_text(run)?,
        )?;
    }

    write_json(
        &path.join("rust-results.json"),
        &RustResultsArtifact {
            schema_version: 1,
            fixture_id: &fixture.fixture_id,
            runs,
        },
    )?;
    let metrics = MetricsArtifact {
        schema_version: 1,
        fixture_id: &fixture.fixture_id,
        relevance_threshold: 1,
        ndcg_gain: "2^relevance-1",
        product_cutoffs: [1, 5, 10],
        beir_cutoffs: standard_cutoffs(fixture.evaluation_depth),
        f32_exact_reference: compare_f32_with_exact_reference(runs)?,
        runs: runs
            .iter()
            .map(|run| evaluate_run(fixture, run))
            .collect::<Result<Vec<_>, _>>()?,
    };
    write_json(&path.join("metrics.json"), &metrics)?;
    let manifest = Manifest {
        schema_version: 1,
        fixture_id: &fixture.fixture_id,
        fixture_schema_version: fixture.schema_version,
        evaluation_depth: fixture.evaluation_depth,
        deterministic_score: "evaluation_depth-rank+1",
        relevance_threshold: 1,
        ndcg_gain: "2^relevance-1",
        product_cutoffs: [1, 5, 10],
        beir_cutoffs: standard_cutoffs(fixture.evaluation_depth),
        files: vec![
            "qrels.tsv",
            "runs/*.trec",
            "rust-results.json",
            "metrics.json",
        ],
        run_ids: runs.iter().map(|run| run.run_id.as_str()).collect(),
        model: &fixture.model,
        embedding_provenance: &fixture.embedding_provenance,
        dataset_provenance: fixture.dataset_provenance.as_ref(),
    };
    write_json(&path.join("manifest.json"), &manifest)
}

fn qrels_text(fixture: &Fixture) -> Result<String, String> {
    let mut queries = fixture.queries.iter().collect::<Vec<_>>();
    queries.sort_by(|left, right| left.id.cmp(&right.id));
    let mut output = String::new();
    for query in queries {
        validate_trec_id(&query.id, "query ID")?;
        for (document_id, grade) in &query.relevance {
            validate_trec_id(document_id, "document ID")?;
            output.push_str(&format!("{} 0 {} {}\n", query.id, document_id, grade));
        }
    }
    Ok(output)
}

fn trec_run_text(run: &ArtifactRun) -> Result<String, String> {
    let mut output = String::new();
    for query in &run.queries {
        validate_trec_id(&query.query_id, "query ID")?;
        let mut documents = BTreeSet::new();
        for (offset, hit) in query.hits.iter().enumerate() {
            validate_trec_id(&hit.document_id, "document ID")?;
            if !documents.insert(hit.document_id.as_str()) {
                return Err(format!(
                    "run '{}' contains duplicate document '{}' for query '{}'",
                    run.run_id, hit.document_id, query.query_id
                ));
            }
            let rank = offset + 1;
            let score = run.depth.saturating_sub(offset);
            output.push_str(&format!(
                "{} Q0 {} {} {} {}\n",
                query.query_id, hit.document_id, rank, score, run.run_id
            ));
        }
    }
    Ok(output)
}

fn evaluate_run(fixture: &Fixture, run: &ArtifactRun) -> Result<RunMetrics, String> {
    let queries_by_id = fixture
        .queries
        .iter()
        .map(|query| (query.id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut aggregate = StandardMetrics::default();
    let mut queries = Vec::with_capacity(run.queries.len());
    for result in &run.queries {
        let query = queries_by_id.get(result.query_id.as_str()).ok_or_else(|| {
            format!(
                "run '{}' references unknown query '{}'",
                run.run_id, result.query_id
            )
        })?;
        let metrics = query_metrics(query, &result.hits, run.depth);
        aggregate.add_assign(&metrics);
        queries.push(QueryMetrics {
            query_id: result.query_id.clone(),
            metrics,
        });
    }
    if !queries.is_empty() {
        aggregate.divide_by(queries.len() as f64);
    }
    Ok(RunMetrics {
        run_id: run.run_id.clone(),
        duplicate_documents_collapsed: run
            .queries
            .iter()
            .map(|query| query.duplicate_documents_collapsed)
            .sum(),
        aggregate,
        queries,
    })
}

fn compare_f32_with_exact_reference(
    runs: &[ArtifactRun],
) -> Result<Option<ReferenceComparison>, String> {
    let Some(reference) = runs.iter().find(|run| run.mode == "exact_reference") else {
        return Ok(None);
    };
    let Some(candidate) = runs
        .iter()
        .find(|run| run.mode == "vector" && run.encoding == "f32")
    else {
        return Ok(None);
    };
    let reference_queries = reference
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut aggregate = RankingAgreement::default();
    let mut queries = Vec::with_capacity(candidate.queries.len());
    for candidate_query in &candidate.queries {
        let reference_query = reference_queries
            .get(candidate_query.query_id.as_str())
            .ok_or_else(|| {
                format!(
                    "exact reference is missing query '{}'",
                    candidate_query.query_id
                )
            })?;
        let reference_ids = reference_query
            .hits
            .iter()
            .map(|hit| hit.document_id.as_str())
            .collect::<Vec<_>>();
        let candidate_ids = candidate_query
            .hits
            .iter()
            .map(|hit| hit.document_id.as_str())
            .collect::<Vec<_>>();
        let reference_set = reference_ids.iter().copied().collect::<BTreeSet<_>>();
        let reference_at_10 = reference_ids
            .iter()
            .take(10)
            .copied()
            .collect::<BTreeSet<_>>();
        let recall_at_10 = if reference_at_10.is_empty() {
            1.0
        } else {
            candidate_ids
                .iter()
                .take(10)
                .filter(|document_id| reference_at_10.contains(**document_id))
                .count() as f64
                / reference_at_10.len() as f64
        };
        let recall_at_depth = if reference_set.is_empty() {
            1.0
        } else {
            candidate_ids
                .iter()
                .filter(|document_id| reference_set.contains(**document_id))
                .count() as f64
                / reference_set.len() as f64
        };
        let agreement = RankingAgreement {
            recall_at_10,
            recall_at_depth,
            top_1_agreement: f64::from(candidate_ids.first() == reference_ids.first()),
            ordered_result_agreement_at_10: f64::from(
                candidate_ids
                    .iter()
                    .take(10)
                    .eq(reference_ids.iter().take(10)),
            ),
            ordered_result_agreement: f64::from(candidate_ids == reference_ids),
        };
        aggregate.recall_at_10 += agreement.recall_at_10;
        aggregate.recall_at_depth += agreement.recall_at_depth;
        aggregate.top_1_agreement += agreement.top_1_agreement;
        aggregate.ordered_result_agreement_at_10 += agreement.ordered_result_agreement_at_10;
        aggregate.ordered_result_agreement += agreement.ordered_result_agreement;
        queries.push(QueryRankingAgreement {
            query_id: candidate_query.query_id.clone(),
            agreement,
        });
    }
    if !queries.is_empty() {
        let divisor = queries.len() as f64;
        aggregate.recall_at_10 /= divisor;
        aggregate.recall_at_depth /= divisor;
        aggregate.top_1_agreement /= divisor;
        aggregate.ordered_result_agreement_at_10 /= divisor;
        aggregate.ordered_result_agreement /= divisor;
    }
    Ok(Some(ReferenceComparison {
        reference_run_id: reference.run_id.clone(),
        candidate_run_id: candidate.run_id.clone(),
        aggregate,
        queries,
    }))
}

fn query_metrics(query: &FixtureQuery, hits: &[RankedDocument], depth: usize) -> StandardMetrics {
    let cutoffs = standard_cutoffs(depth);
    StandardMetrics {
        ndcg_at_5: ndcg(query, hits, 5),
        ndcg_at_10: ndcg(query, hits, 10),
        recall_at_5: recall(query, hits, 5),
        recall_at_10: recall(query, hits, 10),
        success_at_1: success(query, hits, 1),
        precision_at_5: precision(query, hits, 5),
        mrr_at_10: reciprocal_rank(query, hits, 10),
        average_precision: average_precision(query, hits),
        judged_at_5: judged(query, hits, 5),
        judged_at_10: judged(query, hits, 10),
        ndcg_at: cutoffs
            .iter()
            .map(|cutoff| (*cutoff, ndcg(query, hits, *cutoff)))
            .collect(),
        recall_at: cutoffs
            .iter()
            .map(|cutoff| (*cutoff, recall(query, hits, *cutoff)))
            .collect(),
        precision_at: cutoffs
            .iter()
            .map(|cutoff| (*cutoff, precision(query, hits, *cutoff)))
            .collect(),
    }
}

fn standard_cutoffs(depth: usize) -> Vec<usize> {
    STANDARD_CUTOFFS
        .into_iter()
        .filter(|cutoff| *cutoff <= depth)
        .collect()
}

fn relevant_documents(query: &FixtureQuery) -> BTreeSet<&str> {
    query
        .relevance
        .iter()
        .filter(|(_, grade)| **grade >= 1)
        .map(|(document_id, _)| document_id.as_str())
        .collect()
}

fn recall(query: &FixtureQuery, hits: &[RankedDocument], cutoff: usize) -> f64 {
    let relevant = relevant_documents(query);
    if relevant.is_empty() {
        return 0.0;
    }
    hits.iter()
        .take(cutoff)
        .filter(|hit| relevant.contains(hit.document_id.as_str()))
        .count() as f64
        / relevant.len() as f64
}

fn precision(query: &FixtureQuery, hits: &[RankedDocument], cutoff: usize) -> f64 {
    if cutoff == 0 {
        return 0.0;
    }
    let relevant = relevant_documents(query);
    hits.iter()
        .take(cutoff)
        .filter(|hit| relevant.contains(hit.document_id.as_str()))
        .count() as f64
        / cutoff as f64
}

fn success(query: &FixtureQuery, hits: &[RankedDocument], cutoff: usize) -> f64 {
    let relevant = relevant_documents(query);
    f64::from(
        hits.iter()
            .take(cutoff)
            .any(|hit| relevant.contains(hit.document_id.as_str())),
    )
}

fn reciprocal_rank(query: &FixtureQuery, hits: &[RankedDocument], cutoff: usize) -> f64 {
    let relevant = relevant_documents(query);
    hits.iter()
        .take(cutoff)
        .position(|hit| relevant.contains(hit.document_id.as_str()))
        .map(|offset| 1.0 / (offset + 1) as f64)
        .unwrap_or(0.0)
}

fn average_precision(query: &FixtureQuery, hits: &[RankedDocument]) -> f64 {
    let relevant = relevant_documents(query);
    if relevant.is_empty() {
        return 0.0;
    }
    let mut found = 0usize;
    let sum = hits
        .iter()
        .enumerate()
        .filter_map(|(offset, hit)| {
            if relevant.contains(hit.document_id.as_str()) {
                found += 1;
                Some(found as f64 / (offset + 1) as f64)
            } else {
                None
            }
        })
        .sum::<f64>();
    sum / relevant.len() as f64
}

fn judged(query: &FixtureQuery, hits: &[RankedDocument], cutoff: usize) -> f64 {
    let denominator = cutoff.min(hits.len());
    if denominator == 0 {
        return 0.0;
    }
    hits.iter()
        .take(cutoff)
        .filter(|hit| query.relevance.contains_key(&hit.document_id))
        .count() as f64
        / denominator as f64
}

fn ndcg(query: &FixtureQuery, hits: &[RankedDocument], cutoff: usize) -> f64 {
    let dcg = hits
        .iter()
        .take(cutoff)
        .enumerate()
        .map(|(offset, hit)| {
            gain(
                query.relevance.get(&hit.document_id).copied().unwrap_or(0),
                offset,
            )
        })
        .sum::<f64>();
    let mut grades = query.relevance.values().copied().collect::<Vec<_>>();
    grades.sort_unstable_by(|left, right| right.cmp(left));
    let ideal = grades
        .into_iter()
        .take(cutoff)
        .enumerate()
        .map(|(offset, grade)| gain(grade, offset))
        .sum::<f64>();
    if ideal == 0.0 {
        0.0
    } else {
        dcg / ideal
    }
}

fn gain(grade: u8, offset: usize) -> f64 {
    (2f64.powi(i32::from(grade)) - 1.0) / (offset as f64 + 2.0).log2()
}

fn add_maps(target: &mut BTreeMap<usize, f64>, source: &BTreeMap<usize, f64>) {
    for (key, value) in source {
        *target.entry(*key).or_default() += value;
    }
}

fn divide_map(values: &mut BTreeMap<usize, f64>, divisor: f64) {
    for value in values.values_mut() {
        *value /= divisor;
    }
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize '{}': {error}", path.display()))?;
    text.push('\n');
    write_text(path, &text)
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, text)
        .map_err(|error| format!("failed to write '{}': {error}", temporary.display()))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("failed to replace '{}': {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_metrics_match_hand_calculated_binary_example() {
        let query = FixtureQuery {
            id: "q".to_owned(),
            category: "test".to_owned(),
            text: "query".to_owned(),
            embedding: vec![1.0],
            relevance: BTreeMap::from([
                ("a".to_owned(), 1),
                ("b".to_owned(), 1),
                ("c".to_owned(), 0),
            ]),
            filter: None,
            forbidden_document_ids: Vec::new(),
            required_text: None,
        };
        let hits = vec![ranked("c", 0), ranked("a", 1), ranked("x", 2)];
        let metrics = query_metrics(&query, &hits, 10);
        assert_eq!(metrics.recall_at_5, 0.5);
        assert_eq!(metrics.precision_at_5, 0.2);
        assert_eq!(metrics.mrr_at_10, 0.5);
        assert_eq!(metrics.average_precision, 0.25);
        assert!((metrics.judged_at_5 - (2.0 / 3.0)).abs() < 1e-12);
    }

    #[test]
    fn rank_scores_are_unique_even_when_raw_scores_tie() {
        let run = ArtifactRun::new(
            "run".to_owned(),
            "vector",
            "f32",
            3,
            [("q".to_owned(), vec![ranked("z", 0), ranked("a", 1)])],
        );
        assert_eq!(
            trec_run_text(&run).unwrap(),
            "q Q0 z 1 3 run\nq Q0 a 2 2 run\n"
        );
    }

    #[test]
    fn document_deduplication_keeps_the_highest_ranked_chunk_and_reports_it() {
        let run = ArtifactRun::new(
            "run".to_owned(),
            "vector",
            "f32",
            3,
            [(
                "q".to_owned(),
                vec![ranked("same", 7), ranked("other", 8), ranked("same", 9)],
            )],
        );
        assert_eq!(run.queries[0].duplicate_documents_collapsed, 1);
        assert_eq!(run.queries[0].hits[0].chunk_id, 7);
        assert_eq!(run.queries[0].hits.len(), 2);
    }

    fn ranked(document_id: &str, chunk_id: u64) -> RankedDocument {
        RankedDocument {
            document_id: document_id.to_owned(),
            chunk_id,
            score: 1.0,
        }
    }
}
