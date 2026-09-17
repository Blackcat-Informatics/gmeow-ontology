// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-1 **teachability gate** (the EPIC scenario-7 teachability contract).
//!
//! Teachability is defined verbatim as "a fresh model given ONLY the generated ~500-token
//! primer achieves a GATED AST-validity rate on HELD-OUT emission tasks." A budget-compliance
//! check ALONE is NOT teachability — these tests operationalize it WITHOUT a live LLM via two
//! independent, falsifiable legs over the held-out corpus
//! (`slices/grounding/lang/examples/gmn-heldout-emission-tasks.ttl`):
//!
//! * **primer-completeness** — every record sigil / operator glyph / repair record a held-out
//!   task exercises is TAUGHT by the graph-derived primer (its glyph + fixity + alias, or its
//!   repair card, is present). Drop a needed row from the primer and this reds.
//! * **AST-validity** — every held-out task document re-parses through `gmn1_read` against the
//!   shipped dictionary at the gated rate (1.0). Perturb a document and this reds.
//!
//! Budget compliance is asserted SEPARATELY (`primer_fits_the_500_token_budget`) — it is a
//! necessary property of the shipped card, not the teachability check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_docs::gmn1_primer::teachability::{ARTIFACT, Observations};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must be resolvable")
}

fn observations() -> &'static Observations {
    static SELECTED: OnceLock<(String, gmeow_errors::Result<Observations>)> = OnceLock::new();
    let selector = std::env::var("GMEOW_TEST_FIXTURE_MANIFEST_SHA256")
        .expect("teachability requires the producer-selected corpus identity");
    let (identity, observed) = SELECTED.get_or_init(|| {
        let observed =
            gmeow_bundle_import::load_authenticated_corpus_artifact(&repo_root(), ARTIFACT)
                .and_then(|bytes| serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from));
        (selector.clone(), observed)
    });
    assert_eq!(
        identity, &selector,
        "teachability observations cannot cross corpus identities"
    );
    observed
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated teachability observation: {error}"))
}

/// The three repair sigils that map to a repair CARD (`gmeow:GmnErr` / `GmnPatch` / `GmnRetract`)
/// rather than to a `gmeow:GmnSigilRole` individual.
fn repair_card_curie(sigil: &str) -> Option<&'static str> {
    match sigil {
        "@err" => Some("gmeow:GmnErr"),
        "@patch" => Some("gmeow:GmnPatch"),
        "@retract" => Some("gmeow:GmnRetract"),
        _ => None,
    }
}

// ── Budget compliance (SEPARATE from teachability) ───────────────────────────────────────

#[test]
fn primer_fits_the_500_token_budget() {
    let observed = observations();
    let primer = &observed.primer;
    let tokens = primer.token_count();
    assert!(
        tokens <= gmeow_docs::llms::GMN1_PRIMER_TOKEN_BUDGET,
        "the GMN-1 primer costs {tokens} tokens, over the {}-token budget",
        gmeow_docs::llms::GMN1_PRIMER_TOKEN_BUDGET
    );
    assert!(
        primer.fits_budget(),
        "fits_budget must agree with token_count"
    );
}

// ── Leg 1 (graph-derived): the primer is a projection, not hand-authored prose ──────────

#[test]
fn gmn1_primer_fits_budget_and_is_graph_derived() {
    let observed = observations();
    let primer = &observed.primer;

    // (a) Budget — the same SEPARATE compliance property, re-asserted here so this test is a
    // complete standalone witness of the shipped card.
    assert!(
        primer.token_count() <= gmeow_docs::llms::GMN1_PRIMER_TOKEN_BUDGET,
        "primer over budget: {} tokens",
        primer.token_count()
    );

    // (b) It cites the GMN core CURIEs — the repair records, the record sigils, and operator
    // targets — so a fresh model sees the whole record grammar + repair loop.
    let cited = primer.cited_curies();
    for core in [
        "gmeow:GmnErr",
        "gmeow:GmnPatch",
        "gmeow:GmnRetract",
        "gmeow:gmnSigilClaim",
        "gmeow:gmnSigilLogic",
    ] {
        assert!(
            cited.contains(core),
            "primer must cite {core}; cited: {cited:?}"
        );
    }
    assert!(
        cited
            .iter()
            .any(|c| c.starts_with("logic:") || c.starts_with("math:")),
        "primer must cite at least one operator target: {cited:?}"
    );

    // (c) NO hand-authored prose: every content line of the rendered card is either the shared
    // structural skeleton (`# `/`## `/`> `/`- ` frame + the fixed prose line) OR a graph-derived
    // row body. A fabricated sentence would be neither and would fail this subset check.
    let bodies = primer.graph_line_bodies();
    for raw in primer.resource_text().lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Structural skeleton the shared llms.rs builder emits (not term content).
        if line.starts_with("# ")
            || line.starts_with("## ")
            || line.starts_with("> ")
            || line.contains("teachability primer — a graph-derived projection")
        {
            continue;
        }
        let body = line.strip_prefix("- ").unwrap_or(line);
        assert!(
            bodies.contains(body),
            "primer line does not trace to a graph source (hand-authored prose?): {body:?}"
        );
    }

    // (d) Every operator row is genuinely graph-shaped: glyph + non-empty fixity + non-empty
    // alias + a grounding-namespace target CURIE.
    for (glyph, (fixity, alias)) in primer.operator_index() {
        assert!(!glyph.is_empty(), "operator row has an empty glyph");
        assert!(!fixity.is_empty(), "operator {glyph} has no fixity");
        assert!(!alias.is_empty(), "operator {glyph} has no alias");
    }
}

// ── Leg 2 (the load-bearing teachability leg): primer-completeness over held-out constructs ─

#[test]
fn primer_covers_every_heldout_construct() {
    let observed = observations();
    let primer = &observed.primer;
    let operator_index: BTreeMap<String, (String, String)> = primer.operator_index();
    let sigil_glyphs = primer.sigil_glyphs();
    let cited = primer.cited_curies();

    let tasks = &observed.tasks;

    // Sanity: the corpus must genuinely exercise a spread of constructs, or the gate is vacuous.
    let all_sigils: BTreeSet<_> = tasks
        .iter()
        .flat_map(|t| t.sigils.iter().cloned())
        .collect();
    let all_ops: BTreeSet<_> = tasks
        .iter()
        .flat_map(|t| t.operator_glyphs.iter().cloned())
        .collect();
    assert!(
        all_sigils.len() >= 10,
        "held-out corpus must exercise the full record-sigil surface; saw {all_sigils:?}"
    );
    assert!(
        all_ops.len() >= 4,
        "held-out corpus must exercise several operator glyphs; saw {all_ops:?}"
    );
    assert!(
        ["@err", "@patch", "@retract"]
            .iter()
            .all(|r| all_sigils.contains(*r)),
        "held-out corpus must exercise the whole @err/@patch/@retract repair loop; saw {all_sigils:?}"
    );

    for task in tasks {
        for sigil in &task.sigils {
            if let Some(card) = repair_card_curie(sigil) {
                // A repair sigil is TAUGHT by its repair card being present in the primer.
                assert!(
                    cited.contains(card),
                    "task {:?} exercises {sigil}, but the primer omits its repair card {card}",
                    task.label
                );
            } else {
                // A record sigil is TAUGHT by its glyph appearing in the primer's sigil table.
                assert!(
                    sigil_glyphs.contains(sigil),
                    "task {:?} exercises record sigil {sigil}, absent from the primer's sigil table {sigil_glyphs:?}",
                    task.label
                );
            }
        }
        for glyph in &task.operator_glyphs {
            // An operator is TAUGHT only when its glyph carries a non-empty fixity AND alias —
            // the exact "glyph + fixity + alias present" completeness contract.
            let (fixity, alias) = operator_index.get(glyph).unwrap_or_else(|| {
                panic!(
                    "task {:?} uses operator {glyph}, absent from the primer glyph table",
                    task.label
                )
            });
            assert!(
                !fixity.is_empty() && !alias.is_empty(),
                "task {:?} uses operator {glyph}, but the primer omits its fixity/alias",
                task.label
            );
        }
    }

    // Non-circularity: the primer embeds NO held-out task document verbatim — the tasks combine
    // taught constructs in whole documents the primer never shows, so completeness is not the
    // trivial "the primer copied the answer" outcome.
    let rendered = primer.resource_text();
    for task in tasks {
        let record = task
            .document
            .lines()
            .find(|l| !l.starts_with("@gmn{"))
            .unwrap_or("");
        assert!(
            !record.is_empty() && !rendered.contains(record),
            "primer must not embed the held-out task record verbatim (circular): {record:?}"
        );
    }
}

// ── Leg 3 (teachability outcome): held-out AST-validity meets the gate ───────────────────

#[test]
fn heldout_ast_validity_rate_meets_gate() {
    let observed = observations();

    let tasks = &observed.tasks;
    let total = tasks.len();
    assert!(total >= 12, "held-out corpus is too small to gate: {total}");

    let mut valid = 0usize;
    for task in tasks {
        match &task.ast_error {
            None => valid += 1,
            Some(e) => panic!(
                "held-out task {:?} must be a conformant GMN-1 document, but gmn1_read rejected it: {e}\n{}",
                task.label, task.document
            ),
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let rate = valid as f64 / total as f64;
    // The gate: EVERY held-out task parses. This is the deterministic, LLM-free stand-in for
    // "a fresh model achieves a gated AST-validity rate" — the corpus is the ground truth a
    // primer-taught model must reach.
    assert!(
        (rate - 1.0).abs() < f64::EPSILON,
        "held-out AST-validity rate {rate} ({valid}/{total}) is below the 1.0 gate"
    );
}
