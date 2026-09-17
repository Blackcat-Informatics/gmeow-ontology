// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Every committed `corpus.json` under `cases/bench/` passes the license audit,
/// the loader loads ≥1 case per corpus, and every loaded case carries a non-empty
/// hand-derived golden with a positive row count. Ordering is deterministic.
#[test]
fn bench_corpus_loads_audited_cases_with_nonempty_goldens() {
    let cases = load_bench_corpora().expect("bench corpus must load");
    assert!(!cases.is_empty(), "the bench corpus must contain cases");

    // Deterministic order: (corpus, name) is non-decreasing.
    let keys: Vec<(String, String)> = cases
        .iter()
        .map(|c| (c.corpus.clone(), c.name.clone()))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "bench cases must be sorted by (corpus, name)");

    // ≥1 case per committed corpus.
    let corpora: std::collections::BTreeSet<&str> =
        cases.iter().map(|c| c.corpus.as_str()).collect();
    for want in ["chasebench-mini", "relational-core-mini"] {
        assert!(
            corpora.contains(want),
            "expected a loaded case from corpus {want:?}; loaded corpora: {corpora:?}"
        );
        let n = cases.iter().filter(|c| c.corpus == want).count();
        assert!(n >= 1, "corpus {want:?} must contribute >= 1 case, got {n}");
    }

    // Every golden is non-empty with a positive, hand-derived row count, and the
    // artifacts are non-empty.
    for c in &cases {
        assert!(
            !c.golden.is_empty(),
            "{}/{}: empty golden",
            c.corpus,
            c.name
        );
        assert!(
            c.golden.values().all(|g| g.rows > 0),
            "{}/{}: golden must carry a positive row count",
            c.corpus,
            c.name
        );
        assert!(
            !c.rules.trim().is_empty(),
            "{}/{}: empty rule text",
            c.corpus,
            c.name
        );
        assert!(
            !c.engines.is_empty(),
            "{}/{}: empty engine list",
            c.corpus,
            c.name
        );
        // Every retained case carries a world-scoped N-Quads EDB.
        match c.fragment {
            Fragment::Incremental | Fragment::IncrementalGrounding => {
                assert!(
                    !c.edb_nq.trim().is_empty(),
                    "{}/{}: empty incremental input.nq",
                    c.corpus,
                    c.name
                );
                assert!(
                    !c.delta_nq.trim().is_empty(),
                    "{}/{}: empty incremental delta.nq",
                    c.corpus,
                    c.name
                );
            }
            _ => {
                assert!(
                    !c.edb_nq.trim().is_empty(),
                    "{}/{}: empty input.nq",
                    c.corpus,
                    c.name
                );
                assert!(
                    c.delta_nq.is_empty(),
                    "{}/{}: a non-incremental case carries no delta.nq",
                    c.corpus,
                    c.name
                );
            }
        }
    }

    // The required goal-directed backward native case is present with a captured
    // full-answer digest, over the parseable query surface.
    let backward: Vec<&BenchCase> = cases
        .iter()
        .filter(|c| c.fragment == Fragment::Backward)
        .collect();
    assert!(
        !backward.is_empty(),
        "the bench corpus must include >= 1 backward native case"
    );
    for c in &backward {
        assert!(
            c.engines.iter().any(|e| e == "native"),
            "{}/{}: backward case must list the native engine",
            c.corpus,
            c.name
        );
        assert!(
            c.golden.values().all(|g| g.digest.is_some()),
            "{}/{}: backward case must carry a captured answer-set digest",
            c.corpus,
            c.name
        );
        // Confirm the query text parses on the native production surface.
        gmeow_logic::query_ir::parse_query_program(&c.rules).unwrap_or_else(|e| {
            panic!(
                "{}/{}: backward query text must parse as a QProgram: {e}",
                c.corpus, c.name
            )
        });
    }

    // The existential (chasebench) cases parse into typed rules and RUN through
    // the value-inventing chase router (the golden itself is authored by hand,
    // not echoed from this run).
    for c in cases.iter().filter(|c| c.fragment == Fragment::Existential) {
        let dataset = purrdf::parse_dataset(c.edb_nq.as_bytes(), "application/n-quads", None)
            .unwrap_or_else(|e| panic!("{}/{}: EDB must parse: {e}", c.corpus, c.name));
        let rules = c.existential_rules().unwrap_or_else(|e| {
            panic!(
                "{}/{}: existential TGD fixture must parse: {e}",
                c.corpus, c.name
            )
        });
        gmeow_logic::materialize::materialize_existential_rules(
            dataset.as_ref(),
            &rules,
            gmeow_logic::materialize::MaterializationLimits::default(),
        )
        .unwrap_or_else(|e| {
            panic!(
                "{}/{}: existential typed rules must run: {e}",
                c.corpus, c.name
            )
        });
    }

    for c in cases.iter().filter(|c| {
        matches!(
            c.fragment,
            Fragment::Forward | Fragment::Incremental | Fragment::IncrementalGrounding
        )
    }) {
        c.canonical_program().unwrap_or_else(|e| {
            panic!(
                "{}/{}: forward fixture must lower to canonical IR: {e}",
                c.corpus, c.name
            )
        });
    }
}
