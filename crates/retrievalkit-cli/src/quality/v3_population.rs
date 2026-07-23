use std::collections::{BTreeMap, BTreeSet};

use super::v3_canonical::sha256;
use super::v3_schema::{Exclusion, Query};

pub(super) const NORMATIVE_AJ: &str = concat!(
    "{\"case_id\":\"A\",\"derived_policy\":null,\"derived_resolution\":null,\"evidence_judgment\":false,\"expected_path_lanes\":[],\"explicit_seed\":false,\"global_exclusion\":null,\"query_id\":\"qa\",\"tasks\":[\"retrieval\"]}\n",
    "{\"case_id\":\"B\",\"derived_policy\":null,\"derived_resolution\":null,\"evidence_judgment\":false,\"expected_path_lanes\":[],\"explicit_seed\":true,\"global_exclusion\":null,\"query_id\":\"qb\",\"tasks\":[\"retrieval\"]}\n",
    "{\"case_id\":\"C\",\"derived_policy\":null,\"derived_resolution\":null,\"evidence_judgment\":true,\"expected_path_lanes\":[\"explicit\"],\"explicit_seed\":true,\"global_exclusion\":null,\"query_id\":\"qc\",\"tasks\":[\"evidence\",\"path\"]}\n",
    "{\"case_id\":\"D\",\"derived_policy\":\"topic\",\"derived_resolution\":\"success\",\"evidence_judgment\":true,\"expected_path_lanes\":[],\"explicit_seed\":false,\"global_exclusion\":null,\"query_id\":\"qd\",\"tasks\":[\"evidence\",\"retrieval\"]}\n",
    "{\"case_id\":\"E\",\"derived_policy\":\"topic\",\"derived_resolution\":\"success\",\"evidence_judgment\":true,\"expected_path_lanes\":[\"topic\"],\"explicit_seed\":false,\"global_exclusion\":null,\"query_id\":\"qe\",\"tasks\":[\"evidence\",\"path\"]}\n",
    "{\"case_id\":\"F\",\"derived_policy\":\"topic\",\"derived_resolution\":\"no_match\",\"evidence_judgment\":false,\"expected_path_lanes\":[],\"explicit_seed\":false,\"global_exclusion\":null,\"query_id\":\"qf\",\"tasks\":[\"retrieval\"]}\n",
    "{\"case_id\":\"G\",\"derived_policy\":\"topic\",\"derived_resolution\":\"ambiguous\",\"evidence_judgment\":false,\"expected_path_lanes\":[],\"explicit_seed\":false,\"global_exclusion\":null,\"query_id\":\"qg\",\"tasks\":[\"retrieval\"]}\n",
    "{\"case_id\":\"H\",\"derived_policy\":\"topic\",\"derived_resolution\":\"success\",\"evidence_judgment\":true,\"expected_path_lanes\":[\"explicit\",\"topic\"],\"explicit_seed\":true,\"global_exclusion\":null,\"query_id\":\"qh\",\"tasks\":[\"evidence\",\"path\",\"retrieval\"]}\n",
    "{\"case_id\":\"I\",\"derived_policy\":\"team\",\"derived_resolution\":\"success\",\"evidence_judgment\":false,\"expected_path_lanes\":[],\"explicit_seed\":false,\"global_exclusion\":null,\"query_id\":\"qi\",\"tasks\":[\"path\",\"retrieval\"]}\n",
    "{\"case_id\":\"J\",\"derived_policy\":null,\"derived_resolution\":null,\"evidence_judgment\":false,\"expected_path_lanes\":[],\"explicit_seed\":false,\"global_exclusion\":\"no_relevant_documents\",\"query_id\":\"qj\",\"tasks\":[\"retrieval\"]}\n"
);

#[derive(Debug, Clone)]
pub(super) struct Populations {
    pub q: BTreeSet<String>,
    pub retrieval: BTreeSet<String>,
    pub explicit: BTreeSet<String>,
    pub derived_declared: BTreeMap<String, BTreeSet<String>>,
    pub derived_failed: BTreeMap<String, BTreeSet<String>>,
}

impl Populations {
    pub(super) fn derive(queries: &[Query], exclusions: &[Exclusion]) -> Result<Self, String> {
        let q = queries.iter().map(|query| query.query_id.clone()).collect();
        let retrieval = queries
            .iter()
            .filter(|query| query.tasks.iter().any(|task| task == "retrieval"))
            .map(|query| query.query_id.clone())
            .collect();
        let explicit = queries
            .iter()
            .filter(|query| query.explicit_seed.is_some())
            .map(|query| query.query_id.clone())
            .collect();
        let mut derived_declared: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for query in queries {
            if let Some(policy) = &query.derived_seed_policy_id {
                derived_declared
                    .entry(policy.clone())
                    .or_default()
                    .insert(query.query_id.clone());
            }
        }
        let mut derived_failed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for exclusion in exclusions {
            if matches!(
                exclusion.reason.as_str(),
                "derived_seed_no_match" | "derived_seed_ambiguous"
            ) {
                if !derived_declared
                    .get(&exclusion.lane)
                    .is_some_and(|ids| ids.contains(&exclusion.query_id))
                {
                    return Err(format!(
                        "exclusions.jsonl: query '{}' is excluded from undeclared derived lane '{}'",
                        exclusion.query_id, exclusion.lane
                    ));
                }
                derived_failed
                    .entry(exclusion.lane.clone())
                    .or_default()
                    .insert(exclusion.query_id.clone());
            }
        }
        for policy in derived_declared.keys() {
            derived_failed.entry(policy.clone()).or_default();
        }
        Ok(Self {
            q,
            retrieval,
            explicit,
            derived_declared,
            derived_failed,
        })
    }

    pub(super) fn successful(&self, policy: &str) -> BTreeSet<String> {
        self.derived_declared
            .get(policy)
            .into_iter()
            .flatten()
            .filter(|id| {
                !self
                    .derived_failed
                    .get(policy)
                    .is_some_and(|failed| failed.contains(*id))
            })
            .cloned()
            .collect()
    }

    pub(super) fn intersection(
        left: &BTreeSet<String>,
        right: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        left.intersection(right).cloned().collect()
    }
}

pub(super) fn population_hash(ids: &BTreeSet<String>) -> String {
    let mut bytes = Vec::new();
    for id in ids {
        bytes.extend_from_slice(id.as_bytes());
        bytes.push(b'\n');
    }
    sha256(&bytes)
}

pub(super) fn verify_normative_fixture() -> Result<(), String> {
    if NORMATIVE_AJ.len() != 2_135 {
        return Err(format!(
            "section 12.1 fixture expected 2135 bytes, actual {}",
            NORMATIVE_AJ.len()
        ));
    }
    let digest = sha256(NORMATIVE_AJ.as_bytes());
    let expected = "4d7b920b8ae591f0c05cd41abbc36c50210bbf23e6bfa0e09b4eebbffdea4f46";
    if digest != expected {
        return Err(format!(
            "section 12.1 fixture expected SHA-256 {expected}, actual {digest}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normative_fixture_has_exact_bytes_and_hash() {
        verify_normative_fixture().unwrap();
    }

    #[test]
    fn published_population_hashes_match() {
        let cases = [
            (
                ["qa", "qb", "qc", "qd", "qe", "qf", "qg", "qh", "qi"].as_slice(),
                "91be2f127eff88b3d41229df2904cb3b7203992673711e3ee960ade05c35496d",
            ),
            (
                ["qa", "qb", "qd", "qf", "qg", "qh", "qi"].as_slice(),
                "c373605c9580a90c0194ed28f5e07debfef5f8315547e9af5eb2cae963bfd4e3",
            ),
            (
                ["qb", "qc", "qh"].as_slice(),
                "533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5",
            ),
            (
                ["qd", "qe", "qf", "qg", "qh"].as_slice(),
                "a3b85dfbb4d7e5178e8cf34ab7c8d1474fbc03ceba933c731fbb83da012ad2f8",
            ),
            (
                ["qf", "qg"].as_slice(),
                "f1a82a3707574638a0dff6e16db2616c73c0692bcee0e55a21b565097d3267fb",
            ),
            (
                ["qd", "qe", "qh"].as_slice(),
                "be40e5a59829766e4ec9bc36e50f69f2c3f0b8c4f0e59fff0f253878622bac59",
            ),
            (
                ["qi"].as_slice(),
                "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
            ),
            (
                [].as_slice(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                ["qb", "qh"].as_slice(),
                "2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f",
            ),
            (
                ["qd", "qf", "qg", "qh"].as_slice(),
                "d9bd478b70d090c4b9543d346a42f300977480baf6f7d65f1c30e3608153a082",
            ),
            (
                ["qd", "qh"].as_slice(),
                "b64c45f1a2bef306eb3daca23aaa916bcbc151fef367325a7160e9520651f24e",
            ),
        ];
        for (ids, expected) in cases {
            let ids = ids.iter().map(|id| (*id).to_owned()).collect();
            assert_eq!(population_hash(&ids), expected);
        }
    }
}
