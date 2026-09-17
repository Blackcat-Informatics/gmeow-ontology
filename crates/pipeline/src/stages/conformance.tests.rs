// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn authenticated_conformance_artifact(path: &str) -> Vec<u8> {
    crate::fixture::authenticated_artifact(&repo_root(), "stage-conformance", path).unwrap_or_else(
        |error| panic!("load authenticated stage-conformance artifact {path}: {error}"),
    )
}

#[test]
fn agreement_tallies_are_deterministic_sorted_and_cover_lane_a() {
    // The producer receipt owns determinism. The test only inspects the exact
    // admitted tally product and never grades the external corpus.
    let a = authenticated_conformance_artifact(AGREEMENT_TALLIES_PATH);
    let records: BTreeMap<String, TallyRecord> =
        serde_json::from_slice(&a).expect("tally JSON parses");
    assert!(
        !records.is_empty(),
        "the committed corpus must grade something"
    );
    for (corpus, r) in &records {
        assert_eq!(
            r.cases,
            r.agree + r.corpus_only + r.dl_gap,
            "corpus {corpus}: cases must partition into agree/corpus-only/dl-gap"
        );
        assert!(
            matches!(
                r.lane.as_str(),
                "a" | "b" | "divergence" | "native-profiled"
            ),
            "corpus {corpus}: lane must be a recognized token, got {:?}",
            r.lane
        );
    }
    let native = records
        .get("w3c-owl2-full-native")
        .expect("selected native corpus tally");
    assert_eq!(
        native.cases, 30,
        "the two source-admission observations are outside semantic comparison counts"
    );
    assert_eq!(native.lane, "native-profiled");
    // Sorted keys: the serialized order must equal the BTreeMap key order.
    let keys: Vec<&String> = records.keys().collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "tally JSON keys must be sorted");
}

#[test]
fn authenticated_conformance_product_stays_in_its_named_graph() {
    let a = authenticated_conformance_artifact(CONFORMANCE_NQ_PATH);
    // Every emitted quad lands in the conformance graph (never elsewhere).
    let text = String::from_utf8(a).expect("utf-8");
    assert!(!text.is_empty(), "conformance product must not be empty");
    let kind = format!("<{}SourceAdmissionObservation>", gmeow_ns::LOGIC_NS);
    let subjects: std::collections::BTreeSet<_> = text
        .lines()
        .filter(|line| line.contains(&kind))
        .map(|line| {
            line.split_whitespace()
                .next()
                .expect("typed observation subject")
        })
        .collect();
    assert_eq!(
        subjects.len(),
        2,
        "both original bare-nil cases ship as explicit source observations"
    );
    for subject in &subjects {
        let rows: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with(&format!("{subject} ")))
            .collect();
        for required in [
            "sourceAdmissionSelected> \"false\"",
            "sourceAdmissionPublishedToken> \"inconsistent\"",
            "sourceAdmissionInputDigest>",
            "sourceAdmissionWorldKey>",
            "sourceAdmissionEvidence>",
        ] {
            assert!(
                rows.iter().any(|row| row.contains(required)),
                "{subject} must retain {required}"
            );
        }
        assert!(
            rows.iter()
                .all(|row| !row.contains("ConformanceComparison>")),
            "source admission carries no consistency comparison"
        );
    }
    for case in ["webont-i5-5-003", "webont-i5-5-004"] {
        assert!(
            text.contains(&format!("w3c-owl2-full-native/{case}")),
            "exact original source case retained"
        );
    }
    for line in text.lines() {
        assert!(
            line.ends_with(&format!(
                "<{}> .",
                gmeow_conformance::divergence::CONFORMANCE_GRAPH
            )),
            "line not in the conformance graph: {line}"
        );
    }
}

#[test]
fn combined_conformance_nq_carries_reified_capability_gaps() {
    // The committed `entailment-mini-divergence` corpus carries two structured
    // gap-shape cases (`multi-triple-conclusion` → vendoring-multi-goal,
    // `role-conclusion` → role-assertion). The FULL conformance NQ (the same
    // producer `run()` and `build_conformance_divergence` share) must reify both as
    // `gmeow:CapabilityGap` individuals pointing at the correct `gmeow:GapShape`
    // ontology individuals — the G3 fold this test guards.
    let a = authenticated_conformance_artifact(CONFORMANCE_NQ_PATH);
    let text = String::from_utf8(a).expect("utf-8");
    for line in text.lines() {
        assert!(
            line.ends_with(&format!(
                "<{}> .",
                gmeow_conformance::divergence::CONFORMANCE_GRAPH
            )),
            "line not in the conformance graph: {line}"
        );
    }
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    assert!(
        text.contains(&format!("<{GMEOW}CapabilityGap>")),
        "must reify at least one gmeow:CapabilityGap individual: {text}"
    );
    assert!(
        text.contains(&format!("<{GMEOW}GapShapeVendoringMultiGoal>")),
        "must carry the vendoring-multi-goal committed case's ontology individual: {text}"
    );
    assert!(
        text.contains(&format!("<{GMEOW}GapShapeRoleAssertion>")),
        "must carry the role-assertion committed case's ontology individual: {text}"
    );
}

#[test]
fn tptp_problem_divergence_folds_into_a_conformance_finding() {
    // Proves the `source/problem.p` (SZS) grading path AND the divergence fold
    // end-to-end: a TPTP case whose published SZS ground truth disagrees with the
    // frozen native verdict surfaces as a `gmeow:Finding` in the conformance graph
    // — never silently agreed away. The committed `tptp-mini` cases agree by
    // construction, so this synthetic case is the only always-on exercise of the
    // problem.p dispatch's divergence branch.
    let tmp = tempfile::tempdir().expect("tempdir");
    let ext = tmp.path().join("external");
    let case = ext.join("tptp-fold-probe").join("case1");
    std::fs::create_dir_all(case.join("source")).unwrap();
    std::fs::create_dir_all(case.join("expected")).unwrap();
    // Published SZS ground truth: Unsatisfiable → the runner's `inconsistent` bucket.
    std::fs::write(
        case.join("source").join("problem.p"),
        "% SZS status Unsatisfiable for fold-probe\nfof(a, axiom, p(x)).\n",
    )
    .unwrap();
    // Frozen native verdict: the EL/DL fragment could not decide it (an honest gap).
    let world = "https://gmeow.example/tptp-fold-probe/case1/w";
    std::fs::write(
        case.join("expected").join("verdicts.json"),
        format!("{{ \"{world}\": {{ \"status\": \"incomplete\" }} }}"),
    )
    .unwrap();

    std::fs::write(
        case.join("profile.json"),
        r#"{"verdict_mode":"consistency"}"#,
    )
    .unwrap();

    // The problem.p dispatch grades the case (published from SZS, native from the
    // frozen verdict) — it is NOT skipped.
    let (graded, admissions) =
        grade_external_cases(tmp.path(), &ext, &BTreeMap::new()).expect("grade");
    assert!(
        admissions.is_empty(),
        "semantic fixtures do not fabricate source-admission observations"
    );
    let [g] = graded.as_slice() else {
        panic!(
            "expected exactly one graded TPTP case, got {}",
            graded.len()
        );
    };
    assert_eq!(
        g.comparison.published, "inconsistent",
        "SZS Unsatisfiable projects to the inconsistent bucket"
    );
    assert_eq!(
        g.comparison.native, "incomplete",
        "the frozen native verdict is threaded through the problem.p path"
    );

    // The divergence folds into a gmeow:Finding, and every quad rides the
    // conformance graph.
    let nq = emit_divergence_nq(&g.corpus, std::slice::from_ref(&g.comparison));
    assert!(
        !nq.trim().is_empty(),
        "a native↔published divergence must emit at least one quad"
    );
    assert!(
        nq.contains("gmeow"),
        "the fold must emit a gmeow:Finding, got: {nq}"
    );
    for line in nq.lines() {
        assert!(
            line.ends_with(&format!(
                "<{}> .",
                gmeow_conformance::divergence::CONFORMANCE_GRAPH
            )),
            "divergence quad not in the conformance graph: {line}"
        );
    }
}

#[test]
fn ontouml_model_divergence_folds_into_a_conformance_finding() {
    // Proves the `source/model.ttl` (OntoUML foundation-discipline) grading path
    // AND the divergence fold end-to-end: a case whose documented anti-pattern the
    // frozen native disciplines did NOT reproduce surfaces as a `gmeow:Finding` in
    // the conformance graph — never silently agreed away. The committed
    // `ontouml-mini` cases agree by construction, so this synthetic case is the only
    // always-on exercise of the model.ttl dispatch's divergence branch.
    let tmp = tempfile::tempdir().expect("tempdir");
    let ext = tmp.path().join("external");
    let case = ext.join("ontouml-fold-probe").join("case1");
    std::fs::create_dir_all(case.join("source")).unwrap();
    std::fs::create_dir_all(case.join("expected")).unwrap();
    // Marks the case as an OntoUML case (content is not re-parsed by the fold).
    std::fs::write(case.join("source").join("model.ttl"), "# probe\n").unwrap();
    // Documented anti-pattern the disciplines were expected to reproduce.
    std::fs::write(
        case.join("profile.json"),
        "{ \"documented_antipattern\": \"RelComp\" }",
    )
    .unwrap();
    // Frozen native materialization fires a DIFFERENT discipline (FreeRole), so the
    // documented RelComp was not reproduced — a genuine divergence.
    let world = "https://gmeow.example/ontouml-fold-probe/case1/w";
    std::fs::write(
        case.join("expected").join("materialized.nq"),
        format!(
            "<{world}#C> <https://blackcatinformatics.ca/logic/violation> \
                 <https://blackcatinformatics.ca/logic/FreeRole> <{world}> .\n"
        ),
    )
    .unwrap();
    std::fs::write(
        case.join("expected").join("verdicts.json"),
        format!("{{ \"{world}\": {{ \"status\": \"consistent\" }} }}"),
    )
    .unwrap();

    let (graded, admissions) =
        grade_external_cases(tmp.path(), &ext, &BTreeMap::new()).expect("grade");
    assert!(
        admissions.is_empty(),
        "semantic fixtures do not fabricate source-admission observations"
    );
    let [g] = graded.as_slice() else {
        panic!(
            "expected exactly one graded OntoUML case, got {}",
            graded.len()
        );
    };
    assert_eq!(
        g.comparison.published, "RelComp",
        "the documented anti-pattern is the published verdict"
    );
    assert_eq!(
        g.comparison.native, "FreeRole",
        "the fired discipline set (not containing RelComp) is the native verdict"
    );

    let nq = emit_divergence_nq(&g.corpus, std::slice::from_ref(&g.comparison));
    assert!(
        !nq.trim().is_empty(),
        "a documented↔fired divergence must emit at least one quad"
    );
    assert!(
        nq.contains("gmeow"),
        "the fold must emit a gmeow:Finding, got: {nq}"
    );
    for line in nq.lines() {
        assert!(
            line.ends_with(&format!(
                "<{}> .",
                gmeow_conformance::divergence::CONFORMANCE_GRAPH
            )),
            "divergence quad not in the conformance graph: {line}"
        );
    }
}

#[test]
fn input_files_busts_cache_on_ontouml_case_files() {
    // An OntoUML case carries neither `source/manifest.ttl` nor `source/problem.p`,
    // so it must be caught by the `source/model.ttl` branch — otherwise the case is
    // dropped from the cache key and a `model.ttl` / `materialized.nq` / `profile.json`
    // edit would leave a stale `gmeow.gts` fold that the semantic drift gate cannot
    // see (both sides agree on the stale value). Regression guard for that omission.
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let case = root
        .join(EXTERNAL_ROOT)
        .join("ontouml-mini")
        .join("some-case");
    std::fs::create_dir_all(case.join("source")).unwrap();
    std::fs::create_dir_all(case.join("expected")).unwrap();
    let model = case.join("source").join("model.ttl");
    let materialized = case.join("expected").join("materialized.nq");
    let profile = case.join("profile.json");
    std::fs::write(&model, "# model\n").unwrap();
    std::fs::write(&materialized, "# golden\n").unwrap();
    std::fs::write(&profile, "{}\n").unwrap();

    let files = external_case_input_files(root).expect("external case input_files");
    for want in [&model, &materialized, &profile] {
        assert!(
            files.contains(want),
            "cache key must include {} (else an OntoUML case edit leaves a stale fold), got {files:?}",
            want.display()
        );
    }
}
