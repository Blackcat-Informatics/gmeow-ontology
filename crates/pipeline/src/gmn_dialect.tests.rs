// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

/// The live shape of the shipped `lang:` projection tree, as the predicate sees it.
fn live_like() -> BTreeSet<String> {
    set(&[
        "generated/projections/lang/ebnf/gmn.ebnf",
        "generated/projections/lang/ebnf/gts.ebnf",
        "generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf",
        "generated/projections/lang/gmn1/v1/lark/gmn.lark",
        "generated/projections/lang/gmn1/v1/token-metrics.ttl",
        "generated/projections/lang/gmn1/v1/gmn-grounding-glyphs.gmn",
        // Non-GMN neighbours that must NOT be swept in.
        "generated/projections/lang/tei/forms-and-sign-systems.sentSawHerDuck.tei.xml",
        "generated/projections/lang/conllu/forms-and-sign-systems.x.conllu",
        "generated/projections/lang/bcp47-tags.ttl",
    ])
}

#[test]
fn the_predicate_selects_the_dialect_surfaces_and_nothing_else() {
    let selected = gmn_dialect_paths(live_like().iter());
    assert!(selected.contains("generated/projections/lang/ebnf/gmn.ebnf"));
    assert!(selected.contains("generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf"));
    assert!(selected.contains("generated/projections/lang/gmn1/v1/lark/gmn.lark"));
    assert!(selected.contains("generated/projections/lang/gmn1/v1/token-metrics.ttl"));
    // The non-GMN lang projections are the discrimination witness: a predicate that
    // took the whole `lang:` tree would be a slice freeze wearing a dialect gate's
    // name.
    assert!(
        !selected.contains(
            "generated/projections/lang/tei/forms-and-sign-systems.sentSawHerDuck.tei.xml"
        )
    );
    assert!(
        !selected.contains("generated/projections/lang/conllu/forms-and-sign-systems.x.conllu")
    );
    assert!(!selected.contains("generated/projections/lang/bcp47-tags.ttl"));
    // …and a path outside the projection tree entirely is never a dialect path.
    assert!(!is_gmn_dialect_path("generated/medium/gmeow-core-v1.zdict"));
    assert!(!is_gmn_dialect_path("crates/lang-bridge/src/grammar.rs"));
}

/// A file NAMED `*.ebnf` is not an `ebnf/` family member by itself: the clause keys
/// on the directory, so a stray extension cannot satisfy it.
#[test]
fn the_family_clauses_key_on_the_directory_not_the_extension() {
    assert!(
        !Clause {
            id: ClauseId::LangEbnf,
            pattern: "",
            zero_reason: None,
        }
        .matches("generated/projections/lang/gmn.ebnf")
    );
    assert!(
        Clause {
            id: ClauseId::LangEbnf,
            pattern: "",
            zero_reason: None,
        }
        .matches("generated/projections/lang/ebnf/gmn.ebnf")
    );
}

/// Run one check and return its report.
fn run(check: impl FnOnce(&mut ModelFacingReport)) -> ModelFacingReport {
    let mut report = ModelFacingReport::default();
    check(&mut report);
    report
}

#[test]
fn clause_coverage_is_clean_on_a_live_like_tree() {
    let report = run(|r| check_clause_coverage(&gmn_dialect_paths(live_like().iter()), r));
    assert!(
        report.is_clean(),
        "every live clause must match and both declared-zero clauses stay empty: {report}"
    );
}

/// A live clause that matches nothing reds: the byte comparison would otherwise
/// pass by comparing an empty set.
#[test]
fn a_live_clause_matching_nothing_is_a_hard_fail() {
    let mut shrunk = live_like();
    shrunk.retain(|p| !p.contains("/lark/"));
    let report = run(|r| check_clause_coverage(&gmn_dialect_paths(shrunk.iter()), r));
    assert!(!report.is_clean(), "a dropped Lark grammar must red");
    assert!(report.to_string().contains("LangLark"), "{report}");
    assert!(report.to_string().contains("vacuous"), "{report}");
}

/// A DECLARED-zero clause that starts matching reds: the reason has expired and a
/// new model-facing artifact appeared.
#[test]
fn a_declared_zero_clause_that_starts_matching_is_a_hard_fail() {
    let mut grown = live_like();
    grown.insert("generated/projections/lang/abnf/gmn.abnf".to_string());
    let report = run(|r| check_clause_coverage(&gmn_dialect_paths(grown.iter()), r));
    assert!(!report.is_clean(), "a newly-emitted ABNF artifact must red");
    assert!(report.to_string().contains("LangAbnf"), "{report}");
    assert!(report.to_string().contains("expired"), "{report}");
}

fn projection(paths: &BTreeSet<String>) -> BTreeMap<String, Vec<u8>> {
    paths
        .iter()
        .map(|path| (path.clone(), path.as_bytes().to_vec()))
        .collect()
}

#[test]
fn artifact_invariance_passes_when_both_emissions_reconstruct_the_same_bytes() {
    let files = projection(&live_like());
    let mut report = ModelFacingReport::default();
    let compared = check_artifact_invariance(&files, &files, &mut report);
    assert!(
        report.is_clean(),
        "identical projections must agree: {report}"
    );
    assert!(compared.contains("generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf"));
    assert!(!compared.iter().any(|path| path.contains("/tei/")));
}

#[test]
fn artifact_invariance_reds_when_one_artifact_moves() {
    let dist = projection(&live_like());
    let mut baseline = dist.clone();
    baseline.insert(
        "generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf".to_string(),
        b"perturbed".to_vec(),
    );
    let report = run(|r| {
        check_artifact_invariance(&dist, &baseline, r);
    });
    assert!(!report.is_clean(), "a perturbed dialect artifact must red");
    assert!(report.to_string().contains("gmn.gbnf"), "{report}");
    assert!(report.to_string().contains("the same claim"), "{report}");
}

#[test]
fn artifact_invariance_reds_when_one_emission_drops_an_artifact() {
    let dist = projection(&live_like());
    let mut baseline = dist.clone();
    baseline.remove("generated/projections/lang/gmn1/v1/lark/gmn.lark");
    let report = run(|r| {
        check_artifact_invariance(&dist, &baseline, r);
    });
    assert!(!report.is_clean(), "a dropped dialect artifact must red");
    assert!(report.to_string().contains("gmn.lark"), "{report}");
}

/// Both spellings under the ONE `crates/lang-bridge/src/gmn` row.
#[test]
fn the_pin_covers_both_gmn_and_gmn1_module_spellings() {
    assert!(is_gmn_dialect_producer(
        "crates/lang-bridge/src/gmn_symbology.rs"
    ));
    assert!(is_gmn_dialect_producer(
        "crates/lang-bridge/src/gmn1_codec.rs"
    ));
    // …and does NOT cover the non-GMN lang emitters.
    assert!(!is_gmn_dialect_producer("crates/lang-bridge/src/tei.rs"));
    assert!(!is_gmn_dialect_producer("crates/lang-bridge/src/nif.rs"));
    assert!(!is_gmn_dialect_producer("crates/lang-bridge/src/conllu.rs"));
}

#[test]
fn the_census_completeness_check_accepts_the_pinned_producers() {
    let produced: BTreeSet<ProducedPath> = [
        ProducedPath {
            source: "crates/lang-bridge/src/registry.rs".to_string(),
            path: "generated/projections/lang/gmn1/v*/token-metrics.ttl".to_string(),
        },
        ProducedPath {
            source: "crates/lang-bridge/src/tei.rs".to_string(),
            path: "generated/projections/lang/tei/*.tei.xml".to_string(),
        },
    ]
    .into();
    let report = run(|r| check_producer_census_is_complete(&produced, r));
    assert!(
        report.is_clean(),
        "the pinned emitter and a non-GMN emitter both pass: {report}"
    );
}

/// The red fixture for the completeness companion: an unpinned file minting a
/// dialect path.
#[test]
fn the_census_completeness_check_reds_on_an_unpinned_dialect_producer() {
    let produced: BTreeSet<ProducedPath> = [ProducedPath {
        source: "crates/lang-bridge/src/tei.rs".to_string(),
        path: "generated/projections/lang/gmn1/v*/smuggled.gmn".to_string(),
    }]
    .into();
    let report = run(|r| check_producer_census_is_complete(&produced, r));
    assert!(!report.is_clean(), "an unpinned dialect producer must red");
    assert!(report.to_string().contains("tei.rs"), "{report}");
    assert!(
        report.to_string().contains("freezes the incompleteness"),
        "{report}"
    );
}
/// The dialect-content leg refuses a moved binding and accepts a pure relocation — the
/// two arms that make it an invariant rather than a tripwire.
#[test]
fn the_dialect_content_leg_reds_on_a_moved_cost_and_passes_on_a_relocation() {
    let base = BTreeMap::from([("¬".to_string(), 1)]);
    let report = run(|r| {
        check_dialect_content_invariance(&base, &BTreeMap::from([("¬".to_string(), 4)]), r);
    });
    assert!(!report.is_clean(), "a repriced glyph must red");
    assert!(report.to_string().contains("1 -> 4"), "{report}");

    let report = run(|r| check_dialect_content_invariance(&base, &base.clone(), r));
    assert!(
        report.is_clean(),
        "an unchanged surface must pass: {report}"
    );
}
