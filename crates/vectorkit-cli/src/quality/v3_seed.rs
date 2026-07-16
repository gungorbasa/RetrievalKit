use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use icu_casemap::CaseMapper;
use icu_normalizer::ComposingNormalizer;
use serde::Serialize;
use serde_json::{json, Value};
use vectorkit_core::{ChunkIdentity, ChunkKey, FieldName, RecordId};
use vectorkit_graph::{FieldPath, GraphScalar, NodeId, NodeType, Seed};

use super::v3_canonical::{canonical_json, sha256};
use super::v3_schema::{NodeIdentity, NodeSource};
use super::v3_validation::ValidatedCollection;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ResolvedSeed {
    pub production: Seed,
    pub canonical: Value,
    pub provenance: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum DerivedSeedOutcome {
    Resolved(ResolvedSeed),
    Excluded(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SeedResolutionSet {
    pub explicit: BTreeMap<String, ResolvedSeed>,
    pub derived: BTreeMap<(String, String), DerivedSeedOutcome>,
    pub diagnostics: Vec<SeedResolutionDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct SeedResolutionDiagnostic {
    alias_table_sha256: String,
    candidate_seeds: Vec<Value>,
    failure_reason: Option<&'static str>,
    matched_aliases: Vec<Value>,
    normalization_version: String,
    policy_id: String,
    policy_sha256: String,
    policy_version: String,
    query_id: String,
    selected_seed: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedScalar {
    value: char,
    original_start: usize,
    original_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedText {
    scalars: Vec<NormalizedScalar>,
}

pub(super) fn resolve_seeds(validated: &ValidatedCollection) -> Result<SeedResolutionSet, String> {
    let manifest: Value = serde_json::from_slice(&validated.bytes["manifests/seed-policy.json"])
        .map_err(|error| format!("V3 seed resolver: decode seed policy: {error}"))?;
    let parameters = &manifest["parameters"];
    let normalization_version = parameters["normalization"]["normalization_version"]
        .as_str()
        .ok_or_else(|| "V3 seed resolver: normalization version is missing".to_owned())?;

    let provenance = parameters["explicit_policy"]["provenance"]
        .as_array()
        .ok_or_else(|| "V3 seed resolver: explicit provenance is missing".to_owned())?
        .iter()
        .map(|row| {
            let query_id = row["query_id"]
                .as_str()
                .ok_or_else(|| "V3 seed resolver: explicit query ID is missing".to_owned())?;
            Ok((query_id, row))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut explicit = BTreeMap::new();
    for query in validated
        .queries
        .iter()
        .filter(|query| query.explicit_seed.is_some())
    {
        let canonical = query.explicit_seed.clone().unwrap();
        let row = provenance.get(query.query_id.as_str()).ok_or_else(|| {
            format!(
                "V3 seed resolver: query '{}' lacks explicit provenance",
                query.query_id
            )
        })?;
        explicit.insert(
            query.query_id.clone(),
            ResolvedSeed {
                production: production_seed(&canonical)?,
                canonical,
                provenance: json!({
                    "kind":"explicit",
                    "source_id":row["source_id"],
                    "transformation_id":row["transformation_id"]
                }),
            },
        );
    }

    let exclusions = validated
        .exclusions
        .iter()
        .filter(|row| {
            matches!(
                row.reason.as_str(),
                "derived_seed_no_match" | "derived_seed_ambiguous"
            )
        })
        .map(|row| {
            (
                (row.lane.as_str(), row.query_id.as_str()),
                row.reason.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let policies = parameters["derived_policies"]
        .as_array()
        .ok_or_else(|| "V3 seed resolver: derived policies are missing".to_owned())?;
    let mut derived = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for policy in policies {
        let policy_id = string_field(policy, "policy_id")?;
        let policy_version = string_field(policy, "policy_version")?;
        let alias_table_sha256 = string_field(policy, "alias_table_sha256")?;
        let policy_sha256 = sha256(canonical_json(policy)?.as_bytes());
        let aliases = policy["aliases"]
            .as_array()
            .ok_or_else(|| format!("V3 seed resolver: policy '{policy_id}' aliases are missing"))?;
        for query in validated
            .queries
            .iter()
            .filter(|query| query.derived_seed_policy_id.as_deref() == Some(policy_id))
        {
            let resolution = resolve_aliases(aliases, &query.text)?;
            let declared_failure = exclusions
                .get(&(policy_id, query.query_id.as_str()))
                .copied();
            if resolution.failure_reason != declared_failure {
                return Err(format!(
                    "V3 seed resolver: query '{}' policy '{}' calculated failure {:?}, frozen exclusion {:?}",
                    query.query_id, policy_id, resolution.failure_reason, declared_failure
                ));
            }
            let outcome = if let Some(reason) = resolution.failure_reason {
                DerivedSeedOutcome::Excluded(reason)
            } else {
                let canonical = resolution
                    .selected_seed
                    .clone()
                    .expect("successful resolution selects one seed");
                DerivedSeedOutcome::Resolved(ResolvedSeed {
                    production: production_seed(&canonical)?,
                    canonical,
                    provenance: json!({
                        "alias_table_sha256":alias_table_sha256,
                        "kind":"derived",
                        "matched_aliases":resolution.matched_aliases,
                        "normalization_version":normalization_version,
                        "policy_id":policy_id,
                        "policy_sha256":policy_sha256,
                        "policy_version":policy_version
                    }),
                })
            };
            derived.insert((policy_id.to_owned(), query.query_id.clone()), outcome);
            diagnostics.push(SeedResolutionDiagnostic {
                alias_table_sha256: alias_table_sha256.to_owned(),
                candidate_seeds: resolution.candidate_seeds,
                failure_reason: resolution.failure_reason,
                matched_aliases: resolution.matched_aliases,
                normalization_version: normalization_version.to_owned(),
                policy_id: policy_id.to_owned(),
                policy_sha256: policy_sha256.clone(),
                policy_version: policy_version.to_owned(),
                query_id: query.query_id.clone(),
                selected_seed: resolution.selected_seed,
            });
        }
    }
    diagnostics.sort_by(|left, right| {
        left.policy_id
            .as_bytes()
            .cmp(right.policy_id.as_bytes())
            .then_with(|| left.query_id.as_bytes().cmp(right.query_id.as_bytes()))
    });
    Ok(SeedResolutionSet {
        explicit,
        derived,
        diagnostics,
    })
}

#[derive(Debug, Clone)]
struct AliasResolution {
    candidate_seeds: Vec<Value>,
    failure_reason: Option<&'static str>,
    matched_aliases: Vec<Value>,
    selected_seed: Option<Value>,
}

fn resolve_aliases(aliases: &[Value], query: &str) -> Result<AliasResolution, String> {
    let normalized_query = normalize_with_offsets(query);
    let mut matches = Vec::new();
    for alias in aliases {
        let normalized_alias = string_field(alias, "normalized_alias")?;
        let alias_scalars = normalized_alias.chars().collect::<Vec<_>>();
        if alias_scalars.is_empty() || alias_scalars.len() > normalized_query.scalars.len() {
            continue;
        }
        for start in 0..=normalized_query.scalars.len() - alias_scalars.len() {
            let end = start + alias_scalars.len();
            if normalized_query.scalars[start..end]
                .iter()
                .map(|scalar| scalar.value)
                .eq(alias_scalars.iter().copied())
                && boundary_match(&normalized_query.scalars, start, end)
            {
                let original_start = normalized_query.scalars[start..end]
                    .iter()
                    .map(|scalar| scalar.original_start)
                    .min()
                    .unwrap();
                let original_end = normalized_query.scalars[start..end]
                    .iter()
                    .map(|scalar| scalar.original_end)
                    .max()
                    .unwrap();
                matches.push(json!({
                    "alias":alias["alias"],
                    "normalized_end":end,
                    "normalized_start":start,
                    "original_end":original_end,
                    "original_start":original_start,
                    "seed":alias["seed"],
                    "source":alias["source"]
                }));
            }
        }
    }
    let longest = matches
        .iter()
        .map(|matched| {
            aliases
                .iter()
                .find(|alias| {
                    alias["alias"] == matched["alias"]
                        && alias["seed"] == matched["seed"]
                        && alias["source"] == matched["source"]
                })
                .and_then(|alias| alias["normalized_alias"].as_str())
                .map_or(0, |alias| alias.chars().count())
        })
        .max()
        .unwrap_or(0);
    matches.retain(|matched| {
        aliases.iter().any(|alias| {
            alias["alias"] == matched["alias"]
                && alias["seed"] == matched["seed"]
                && alias["source"] == matched["source"]
                && alias["normalized_alias"]
                    .as_str()
                    .is_some_and(|value| value.chars().count() == longest)
        })
    });
    matches.sort_by(compare_matches);
    let mut encoded_matches = BTreeSet::new();
    for matched in &matches {
        if !encoded_matches.insert(canonical_json(matched)?) {
            return Err("V3 seed resolver: duplicate retained alias match".to_owned());
        }
    }
    let candidate_map = matches
        .iter()
        .map(|matched| Ok((canonical_json(&matched["seed"])?, matched["seed"].clone())))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let candidate_seeds = candidate_map.values().cloned().collect::<Vec<_>>();
    let (failure_reason, selected_seed) = if matches.is_empty() {
        (Some("derived_seed_no_match"), None)
    } else if candidate_seeds.len() > 1 {
        (Some("derived_seed_ambiguous"), None)
    } else {
        (None, candidate_seeds.first().cloned())
    };
    Ok(AliasResolution {
        candidate_seeds,
        failure_reason,
        matched_aliases: matches,
        selected_seed,
    })
}

fn compare_matches(left: &Value, right: &Value) -> Ordering {
    let numeric = |value: &Value, field: &str| value[field].as_u64().unwrap();
    numeric(left, "normalized_start")
        .cmp(&numeric(right, "normalized_start"))
        .then_with(|| numeric(left, "normalized_end").cmp(&numeric(right, "normalized_end")))
        .then_with(|| numeric(left, "original_start").cmp(&numeric(right, "original_start")))
        .then_with(|| numeric(left, "original_end").cmp(&numeric(right, "original_end")))
        .then_with(|| string_bytes(left, "alias").cmp(string_bytes(right, "alias")))
        .then_with(|| canonical_bytes(&left["seed"]).cmp(&canonical_bytes(&right["seed"])))
        .then_with(|| {
            string_bytes(&left["source"], "record_id")
                .cmp(string_bytes(&right["source"], "record_id"))
        })
        .then_with(|| {
            canonical_bytes(&left["source"]["field"])
                .cmp(&canonical_bytes(&right["source"]["field"]))
        })
}

fn canonical_bytes(value: &Value) -> Vec<u8> {
    canonical_json(value).unwrap().into_bytes()
}

fn string_bytes<'a>(value: &'a Value, field: &str) -> &'a [u8] {
    value[field].as_str().unwrap().as_bytes()
}

fn normalize_with_offsets(value: &str) -> NormalizedText {
    let normalizer = ComposingNormalizer::new_nfc();
    let case_mapper = CaseMapper::new();
    let mut prefix = String::new();
    let mut previous = Vec::<NormalizedScalar>::new();
    for (original_index, character) in value.chars().enumerate() {
        prefix.push(character);
        let transformed = case_mapper
            .fold_string(&normalizer.normalize(&prefix))
            .chars()
            .collect::<Vec<_>>();
        let previous_chars = previous
            .iter()
            .map(|scalar| scalar.value)
            .collect::<Vec<_>>();
        let common_prefix = previous_chars
            .iter()
            .zip(&transformed)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix_limit = previous_chars.len().min(transformed.len()) - common_prefix;
        let common_suffix = previous_chars
            .iter()
            .rev()
            .zip(transformed.iter().rev())
            .take(suffix_limit)
            .take_while(|(left, right)| left == right)
            .count();
        let changed_start = previous[common_prefix..previous.len() - common_suffix]
            .iter()
            .map(|scalar| scalar.original_start)
            .min()
            .unwrap_or(original_index);
        let mut next = previous[..common_prefix].to_vec();
        next.extend(
            transformed[common_prefix..transformed.len() - common_suffix]
                .iter()
                .map(|character| NormalizedScalar {
                    value: *character,
                    original_start: changed_start,
                    original_end: original_index + 1,
                }),
        );
        if common_suffix > 0 {
            next.extend_from_slice(&previous[previous.len() - common_suffix..]);
        }
        previous = next;
    }

    let mut collapsed = Vec::<NormalizedScalar>::new();
    for mut scalar in previous {
        if scalar.value.is_whitespace() {
            scalar.value = ' ';
            if let Some(last) = collapsed.last_mut().filter(|last| last.value == ' ') {
                last.original_start = last.original_start.min(scalar.original_start);
                last.original_end = last.original_end.max(scalar.original_end);
                continue;
            }
        }
        collapsed.push(scalar);
    }
    while collapsed.first().is_some_and(|scalar| scalar.value == ' ') {
        collapsed.remove(0);
    }
    while collapsed.last().is_some_and(|scalar| scalar.value == ' ') {
        collapsed.pop();
    }
    NormalizedText { scalars: collapsed }
}

pub(super) fn normalize(value: &str) -> String {
    normalize_with_offsets(value)
        .scalars
        .iter()
        .map(|scalar| scalar.value)
        .collect()
}

fn boundary_match(value: &[NormalizedScalar], start: usize, end: usize) -> bool {
    let alnum = |character: Option<char>| character.is_some_and(char::is_alphanumeric);
    (start == 0 || alnum(Some(value[start - 1].value)) != alnum(Some(value[start].value)))
        && (end == value.len()
            || alnum(Some(value[end - 1].value)) != alnum(Some(value[end].value)))
}

pub(super) fn production_seed(value: &Value) -> Result<Seed, String> {
    match value["kind"].as_str() {
        Some("node_ids") => {
            let nodes = value["nodes"]
                .as_array()
                .ok_or_else(|| "V3 seed resolver: node_ids seed lacks nodes".to_owned())?
                .iter()
                .map(|node| {
                    let identity: NodeIdentity =
                        serde_json::from_value(node.clone()).map_err(|error| {
                            format!("V3 seed resolver: invalid node identity: {error}")
                        })?;
                    production_node_id(&identity)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Seed::NodeIds(nodes))
        }
        Some("equals") => {
            let node_type = NodeType::new(string_field(value, "node_type")?.to_owned())
                .map_err(|error| format!("V3 seed resolver: {error}"))?;
            let field = value["field"]
                .as_array()
                .ok_or_else(|| "V3 seed resolver: equals seed lacks field".to_owned())?
                .iter()
                .map(|segment| {
                    FieldName::new(
                        segment
                            .as_str()
                            .ok_or_else(|| {
                                "V3 seed resolver: field segment is not a string".to_owned()
                            })?
                            .to_owned(),
                    )
                    .map_err(|error| format!("V3 seed resolver: {error}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let values = value["values"]
                .as_array()
                .ok_or_else(|| "V3 seed resolver: equals seed lacks values".to_owned())?
                .iter()
                .map(production_scalar)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Seed::Equals {
                node_type,
                field: FieldPath::new(field)
                    .map_err(|error| format!("V3 seed resolver: {error}"))?,
                values,
            })
        }
        actual => Err(format!(
            "V3 seed resolver: unsupported seed kind {actual:?}"
        )),
    }
}

fn production_node_id(identity: &NodeIdentity) -> Result<NodeId, String> {
    let node_type = NodeType::new(identity.node_type.clone())
        .map_err(|error| format!("V3 seed resolver: {error}"))?;
    match &identity.source {
        NodeSource::Record { record_id } => Ok(NodeId::record(
            node_type,
            RecordId::new(record_id.clone())
                .map_err(|error| format!("V3 seed resolver: {error}"))?,
        )),
        NodeSource::Chunk {
            record_id,
            chunk_key,
        } => Ok(NodeId::chunk(
            node_type,
            ChunkIdentity::new(
                RecordId::new(record_id.clone())
                    .map_err(|error| format!("V3 seed resolver: {error}"))?,
                ChunkKey::new(chunk_key.clone())
                    .map_err(|error| format!("V3 seed resolver: {error}"))?,
            ),
        )),
    }
}

fn production_scalar(value: &Value) -> Result<GraphScalar, String> {
    match value["type"].as_str() {
        Some("string") => value["value"]
            .as_str()
            .map(|value| GraphScalar::String(value.to_owned())),
        Some("integer") => value["value"].as_i64().map(GraphScalar::I64),
        Some("boolean") => value["value"].as_bool().map(GraphScalar::Bool),
        actual => {
            return Err(format!(
                "V3 seed resolver: unsupported graph scalar type {actual:?}"
            ));
        }
    }
    .ok_or_else(|| "V3 seed resolver: graph scalar value is invalid".to_owned())
}

fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("V3 seed resolver: field '{field}' is missing"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::quality::v3_validation::validate;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    #[test]
    fn resolves_all_frozen_explicit_topic_and_team_seed_outcomes() {
        let validated = validate(&fixture_root()).unwrap();
        let resolutions = resolve_seeds(&validated).unwrap();

        assert_eq!(resolutions.explicit.len(), 3);
        assert_eq!(resolutions.diagnostics.len(), 6);
        assert!(matches!(
            resolutions
                .derived
                .get(&("topic".to_owned(), "qd".to_owned())),
            Some(DerivedSeedOutcome::Resolved(_))
        ));
        assert_eq!(
            resolutions
                .derived
                .get(&("topic".to_owned(), "qf".to_owned())),
            Some(&DerivedSeedOutcome::Excluded("derived_seed_no_match"))
        );
        assert_eq!(
            resolutions
                .derived
                .get(&("topic".to_owned(), "qg".to_owned())),
            Some(&DerivedSeedOutcome::Excluded("derived_seed_ambiguous"))
        );
        assert!(matches!(
            resolutions
                .derived
                .get(&("team".to_owned(), "qi".to_owned())),
            Some(DerivedSeedOutcome::Resolved(_))
        ));
    }

    #[test]
    fn full_casefold_nfc_whitespace_and_offsets_are_scalar_based() {
        let normalized = normalize_with_offsets("  STRAße\u{2003}Cafe\u{301}  ");
        assert_eq!(
            normalized
                .scalars
                .iter()
                .map(|scalar| scalar.value)
                .collect::<String>(),
            "strasse café"
        );
        assert_eq!(normalized.scalars[4].original_start, 6);
        assert_eq!(normalized.scalars[5].original_start, 6);
        assert_eq!(normalized.scalars[7].original_start, 8);
        assert_eq!(normalized.scalars[7].original_end, 9);
        let last = normalized.scalars.last().unwrap();
        assert_eq!((last.original_start, last.original_end), (12, 14));
    }

    #[test]
    fn rejects_boundaries_and_retains_only_longest_alias_length() {
        let aliases = vec![
            alias("Alpha", "alpha", "alpha"),
            alias("Alpha Beta", "alpha beta", "beta"),
        ];
        let boundary = resolve_aliases(&aliases[..1], "alphabet soup").unwrap();
        assert_eq!(boundary.failure_reason, Some("derived_seed_no_match"));

        let longest = resolve_aliases(&aliases, "Alpha Beta architecture").unwrap();
        assert_eq!(longest.failure_reason, None);
        assert_eq!(
            longest.selected_seed.unwrap()["nodes"][0]["source"]["record_id"],
            "beta"
        );
        assert_eq!(longest.matched_aliases.len(), 1);
    }

    #[test]
    fn repeated_identical_seed_resolves_once_and_match_order_is_deterministic() {
        let aliases = vec![alias("Alpha", "alpha", "alpha")];
        let resolution = resolve_aliases(&aliases, "Alpha and Alpha").unwrap();
        assert_eq!(resolution.failure_reason, None);
        assert_eq!(resolution.candidate_seeds.len(), 1);
        assert_eq!(resolution.matched_aliases.len(), 2);
        assert_eq!(resolution.matched_aliases[0]["normalized_start"], 0);
        assert_eq!(resolution.matched_aliases[1]["normalized_start"], 10);
    }

    #[test]
    fn competing_longest_aliases_are_ambiguous_without_shorter_fallback() {
        let aliases = vec![
            alias("Shared", "shared", "east"),
            alias("Shared", "shared", "west"),
            alias("Share", "share", "east"),
        ];
        let resolution = resolve_aliases(&aliases, "Shared policy").unwrap();
        assert_eq!(resolution.failure_reason, Some("derived_seed_ambiguous"));
        assert_eq!(resolution.candidate_seeds.len(), 2);
        assert_eq!(resolution.matched_aliases.len(), 2);
    }

    #[test]
    fn converts_node_and_equals_seeds_to_production_types() {
        let node = json!({"kind":"node_ids","nodes":[{"node_type":"Topic","source":{"kind":"record","record_id":"alpha"}}]});
        assert!(matches!(production_seed(&node).unwrap(), Seed::NodeIds(_)));
        let equals = json!({"field":["title"],"kind":"equals","node_type":"Topic","values":[{"type":"string","value":"Alpha"}]});
        assert!(matches!(
            production_seed(&equals).unwrap(),
            Seed::Equals { .. }
        ));
    }

    fn alias(raw: &str, normalized: &str, record_id: &str) -> Value {
        json!({
            "alias":raw,
            "normalized_alias":normalized,
            "seed":{"kind":"node_ids","nodes":[{"node_type":"Topic","source":{"kind":"record","record_id":record_id}}]},
            "source":{"field":["title"],"record_id":record_id}
        })
    }
}
