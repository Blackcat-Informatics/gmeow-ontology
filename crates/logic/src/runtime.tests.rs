// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn native_sources() -> (std::path::PathBuf, gmeow_build_inputs::NativeSources) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let sources =
        gmeow_build_inputs::NativeSources::collect(&root).expect("canonical source ownership");
    (root, sources)
}

#[test]
fn backward_source_partition_is_total() {
    let (_, actual) = native_sources();
    let covered: std::collections::BTreeMap<_, _> = NATIVE_SOURCE_FILES
        .iter()
        .map(|(name, digest)| ((*name).to_owned(), (*digest).to_owned()))
        .collect();
    assert_eq!(
        NATIVE_SOURCE_FILES.len(),
        covered.len(),
        "each source has exactly one commitment"
    );
    assert_eq!(
        actual.files, covered,
        "every declared production module is bound, including newly introduced subdirectories"
    );
    assert_eq!(actual.digest().unwrap(), NATIVE_SOURCE_CONTRACT);
    for (name, reason) in actual.excluded {
        assert!(!reason.is_empty(), "cfg exclusion {name} requires an owner");
        assert!(
            !covered.contains_key(&name),
            "test/doc modules cannot enter the native contract"
        );
    }
    for source in [
        "crates/logic/src/reason/session.rs",
        "crates/logic/src/physical/seminaive/joint/retained.rs",
        "crates/logic-compile/src/lib.rs",
    ] {
        assert!(
            covered.contains_key(source),
            "load-bearing canonical source {source} must be owned"
        );
    }
}

#[test]
fn every_covered_source_file_is_non_empty() {
    let (root, actual) = native_sources();
    for (name, commitment) in &actual.files {
        assert!(
            !std::fs::read(root.join(name)).unwrap().is_empty(),
            "semantic source {name} resolved empty"
        );
        assert!(
            is_hex64(commitment),
            "semantic source {name} has no whole-file commitment"
        );
    }
}

#[test]
fn external_backward_source_partition_is_total() {
    let (_, actual) = native_sources();
    let covered: std::collections::BTreeMap<_, _> = NATIVE_SOURCE_FILES
        .iter()
        .filter(|(name, _)| name.starts_with("crates/term-arena/"))
        .map(|(name, digest)| ((*name).to_owned(), (*digest).to_owned()))
        .collect();
    let arena: std::collections::BTreeMap<_, _> = actual
        .files
        .into_iter()
        .filter(|(name, _)| name.starts_with("crates/term-arena/"))
        .collect();
    assert!(
        !arena.is_empty(),
        "the relocated term arena remains a semantic owner"
    );
    assert_eq!(
        arena, covered,
        "every declared arena production module is pinned"
    );
}

#[test]
fn descriptor_hash_is_deterministic_hex() {
    let a = EngineContract::current();
    let b = EngineContract::current();
    assert_eq!(
        a.descriptor_hash, b.descriptor_hash,
        "descriptor must be stable"
    );
    assert!(
        is_hex64(&a.descriptor_hash),
        "descriptor must be blake3 hex"
    );
    assert!(is_hex64(&a.backward_source_hash));
}

#[test]
fn backward_and_forward_hashes_are_distinct_surfaces() {
    let c = EngineContract::current();
    assert_ne!(
        c.backward_source_hash, c.forward_contract_hash,
        "backward-dispatch and forward-chase surfaces must not alias"
    );
}

#[test]
fn profile_manifest_covers_every_semantic_profile() {
    let c = EngineContract::current();
    assert_eq!(
        c.profiles.len(),
        RUNTIME_PROFILES.len(),
        "manifest must list every profile"
    );
    // Each carries a resolved (non-"unknown") decidability class.
    for cap in &c.profiles {
        assert!(!cap.decidability_class.is_empty());
        assert_ne!(
            cap.decidability_class, "unknown",
            "profile {} has no decidability class",
            cap.profile
        );
    }
}

#[test]
fn assert_matches_accepts_self_and_rejects_drift() {
    let c = EngineContract::current();
    assert!(c.assert_matches(&c.descriptor_hash).is_ok());
    let err = c
        .assert_matches("deadbeef")
        .expect_err("a mismatched pin must be a typed hard failure, not silently accepted");
    assert_eq!(
        gmeow_errors::code::code_str(err.code()),
        crate::error::ContractDrift::CODE,
        "engine-contract drift must surface as the dedicated logic.contract-drift kind"
    );
}

#[test]
fn query_contract_hash_is_single_sourced_and_varies_by_budget() {
    let profile = crate::profile_gate::PROBABILISTIC_PROFILE;
    let budget_a = Budget {
        max_answers: Some(1),
        max_steps: None,
    };
    let budget_b = Budget {
        max_answers: Some(2),
        max_steps: None,
    };
    // Deterministic for identical inputs.
    assert_eq!(
        EngineContract::query_contract_hash(profile, &budget_a),
        EngineContract::query_contract_hash(profile, &budget_a),
    );
    // Delegates to the dispatch engine's own helper (single source of truth).
    assert_eq!(
        EngineContract::query_contract_hash(profile, &budget_a),
        crate::dispatch::query_contract_hash(profile, &budget_a),
    );
    // The per-query contract genuinely depends on the invocation, not only on source.
    assert_ne!(
        EngineContract::query_contract_hash(profile, &budget_a),
        EngineContract::query_contract_hash(profile, &budget_b),
        "different budgets must yield different per-query contracts"
    );
}

#[test]
fn to_nquads_projects_the_descriptor_into_the_graph() {
    let c = EngineContract::current();
    let graph = "https://example.org/consumer/ledger";
    let nquads = c.to_nquads(graph);
    assert!(!nquads.is_empty());
    // Deterministic.
    assert_eq!(nquads, c.to_nquads(graph));
    assert!(nquads.contains(&format!("<{LOGIC_NAMESPACE}EngineContract>")));
    assert!(nquads.contains(&c.descriptor_hash));
    for line in nquads.lines() {
        assert!(
            line.ends_with(&format!("<{graph}> .")),
            "every quad must land in the target graph: {line}"
        );
    }
}
