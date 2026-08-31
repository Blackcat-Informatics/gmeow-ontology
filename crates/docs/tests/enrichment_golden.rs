// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixed-subject render goldens for the max-info enrichment surfaces.
//!
//! These lock the *new* enrichment renderers on a deterministic subject so a
//! future refactor cannot silently blank or garble them:
//! - `competency_index`, `grammar_page`, `loss_ledger_rows`, `pipeline_dag_svg`
//!   are source-lane pages rendered straight off the cached model;
//! - `fixture_do_dont_pair` extracts the term-page "Conformance examples"
//!   section for the first fixture-referenced term;
//! - `term_diagnostics_section` and `term_entailments` exercise the B1 / B3
//!   *carrier* render paths by attaching a synthetic digest / a synthetic
//!   `ExecutableDocsData` (the pipeline supplies the real ones), so the "you
//!   might hit" and "Inferred facts" panels render with real, hand-built content
//!   rather than depending on a live reasoner/validator pass.

use gmeow_docs::exec::{Entailment, ExecutableDocsData};
use gmeow_docs::render::{Page, term_slug, to_markdown, to_markdown_exec};
use gmeow_docs::svg;
use gmeow_docs::{DiagnosticsDigest, DocDiagFinding, DocTerm, DocsModel, TermLossDigest};

mod common;

/// Return the `## <heading>` section of a rendered Markdown page — from the exact
/// `## <heading>` line up to (excluding) the next `## ` heading or end of doc.
/// Hard-fails if the section is absent, so a golden that stops rendering the
/// section reds rather than silently snapshotting empty.
fn section(md: &str, heading: &str) -> String {
    let marker = format!("## {heading}");
    let mut out: Vec<&str> = Vec::new();
    let mut found = false;
    for line in md.lines() {
        if line == marker {
            found = true;
            out.push(line);
            continue;
        }
        if found {
            if line.starts_with("## ") {
                break;
            }
            out.push(line);
        }
    }
    assert!(found, "section `## {heading}` not found in rendered page");
    // Trim a trailing blank line so the golden is a tight block.
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// The first documented term by stable (curie, iri) sort — a fully deterministic
/// subject for the synthetic-carrier goldens (the section renders for any term,
/// so the choice only needs to be stable).
fn first_term(model: &DocsModel) -> &DocTerm {
    model
        .terms
        .iter()
        .min_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)))
        .expect("model has at least one term")
}

/// The first term (by stable curie/iri sort) referenced by a conformance fixture,
/// so its term page renders a non-empty "Conformance examples" Do/Don't block.
fn first_fixture_term(model: &DocsModel) -> &DocTerm {
    let mut candidates: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|t| {
            model
                .fixtures
                .iter()
                .any(|f| f.terms_referenced.iter().any(|c| c == &t.curie))
        })
        .collect();
    candidates.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    candidates
        .first()
        .copied()
        .expect("at least one term is referenced by a conformance fixture")
}

#[test]
fn competency_index_markdown_golden() {
    // The competency index carries every CQ; lock its deterministic head — the
    // page heading plus the first competency's question/rationale/query block.
    let model = common::cached_model();
    let md = to_markdown(&model, &Page::CompetencyIndex);
    let head: String = md.lines().take(30).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn grammar_page_markdown_golden() {
    // The first notation grammar by slug sort — a deterministic representative.
    // Lock its head (title + license + the EBNF fence opening + first productions).
    let model = common::cached_model();
    let mut grammars = model.grammars.clone();
    grammars.sort_by(|a, b| a.slug.cmp(&b.slug));
    let slug = grammars
        .first()
        .expect("at least one authored EBNF grammar exists")
        .slug
        .clone();
    let md = to_markdown(&model, &Page::Grammar(slug));
    let head: String = md.lines().take(24).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn loss_ledger_rows_markdown_golden() {
    // The projection-loss ledger page carries the compiler-emitted whole-program
    // rows plus the authored worked-example rows (A4). Lock the head, which pins
    // the row table shape.
    let model = common::cached_model();
    let md = to_markdown(&model, &Page::LogicLossLedger);
    let head: String = md.lines().take(30).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn pipeline_dag_svg_golden() {
    // The pipeline DAG is large (a node per stage); lock its structural head (the
    // SVG open tag, title, marker defs, and the first node), mirroring the
    // slice-dependency SVG golden. Byte-determinism is asserted alongside.
    let model = common::cached_model();
    let svg_doc = svg::pipeline_dag_svg(&model);
    assert_eq!(
        svg_doc,
        svg::pipeline_dag_svg(&model),
        "the pipeline DAG SVG must be byte-deterministic"
    );
    let head: String = svg_doc.lines().take(12).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn fixture_do_dont_pair_markdown_golden() {
    // The term-page "Conformance examples" Do/Don't block for the first
    // fixture-referenced term — the A1 render path (violation-code join + rationale).
    let model = common::cached_model();
    let term = first_fixture_term(&model);
    let md = to_markdown(&model, &Page::Term(term_slug(term)));
    insta::assert_snapshot!(section(&md, "Conformance examples"));
}

#[test]
fn term_diagnostics_section_markdown_golden() {
    // The B1 carrier render path: attach a SYNTHETIC diagnostics digest (the
    // pipeline folds the real one from stage-validate + stage-compile-logic) so the
    // "Diagnostics you might hit" panel renders with hand-built findings — the
    // by-severity roll-up plus both the deep-linked (help_uri Some) and plain
    // (help_uri None) code-display branches.
    let mut model = common::cached_model();
    let iri = first_term(&model).iri.clone();

    let mut digest = DiagnosticsDigest::default();
    digest.by_term.insert(
        iri.clone(),
        vec![
            DocDiagFinding {
                code: "shacl.MinCountConstraintComponent".to_string(),
                severity: "error".to_string(),
                category: "Constraint".to_string(),
                message: "a required property is missing on this focus node".to_string(),
                slice_iri: None,
                help_uri: Some(
                    "https://blackcatinformatics.ca/gmeow/docs/integrity-constraints/index.md#shacl-mincountconstraintcomponent"
                        .to_string(),
                ),
            },
            DocDiagFinding {
                code: "logic-compile.LOSSY_DROP".to_string(),
                severity: "warning".to_string(),
                category: "Projection".to_string(),
                message: "an axiom shape is dropped under the OWL-DL projection".to_string(),
                slice_iri: None,
                help_uri: None,
            },
        ],
    );
    digest.total = 2;
    model.attach_diagnostics(digest);

    let slug = term_slug(first_term(&model));
    let md = to_markdown(&model, &Page::Term(slug));
    insta::assert_snapshot!(section(&md, "Diagnostics you might hit"));
}

#[test]
fn term_entailments_markdown_golden() {
    // The B3 carrier render path: feed a SYNTHETIC `ExecutableDocsData` keyed by
    // the term's exact IRI (the pipeline parses the real entailments from
    // stage-reason's materialized explanations — reason-once) so the "Inferred
    // facts" panel renders with a hand-built derivation (firing rule + concluded
    // axiom + premises).
    let model = common::cached_model();
    let term = first_term(&model);

    let mut exec = ExecutableDocsData::default();
    exec.term_entailments.insert(
        term.iri.clone(),
        vec![Entailment {
            rule: "logic:SubClassTransitivity".to_string(),
            conclusion: format!("{} rdfs:subClassOf gmeow:Entity", term.curie),
            premises: vec![
                format!("{} rdfs:subClassOf gmeow:Continuant", term.curie),
                "gmeow:Continuant rdfs:subClassOf gmeow:Entity".to_string(),
            ],
        }],
    );

    let md = to_markdown_exec(&model, &Page::Term(term_slug(term)), &exec);
    insta::assert_snapshot!(section(&md, "Inferred facts"));
}

#[test]
fn diagnostics_and_loss_empty_vs_absent_render_distinction() {
    // Locks the empty-vs-absent invariant on the RENDER side (the missing-upstream
    // HARD FAIL is locked pipeline-side by the `*_digest_from_upstream` tests): a
    // term page rendered with NO digest attached omits the carrier section entirely,
    // while the SAME page rendered with a `Some(empty)` digest renders the honest
    // "no diagnostics" / "carried exactly by every projection" text — never a hard
    // fail, never conflated with the section being absent because nothing was
    // attached. A refactor that collapses attached-but-empty into not-attached reds
    // this test.
    // The `body_*` UI keys are pub(crate), so match the English chrome literals the
    // renderer resolves (these are pinned by the i18n single-source key-count gate).
    const DIAG_HEADING: &str = "Diagnostics you might hit";
    const LOSS_HEADING: &str = "How this term degrades under projection";
    const DIAG_NONE: &str = "No diagnostics recorded against this term in the current build.";
    const LOSS_NONE: &str = "Carried exactly by every projection";

    let base = common::cached_model();
    let slug = term_slug(first_term(&base));

    // No digest attached ⇒ neither carrier section is present at all.
    let none_page = to_markdown(&base, &Page::Term(slug.clone()));
    assert!(
        !none_page.contains(&format!("## {DIAG_HEADING}")),
        "the diagnostics section must be ABSENT when no digest is attached"
    );
    assert!(
        !none_page.contains(&format!("## {LOSS_HEADING}")),
        "the projection-degradation section must be ABSENT when no digest is attached"
    );

    // Attach EMPTY (but present) digests — the clean-repo case.
    let mut empty = common::cached_model();
    empty.attach_diagnostics(DiagnosticsDigest::default());
    empty.attach_term_loss(TermLossDigest::default());
    let empty_page = to_markdown(&empty, &Page::Term(slug));

    // Both sections now render, each with its honest empty-state line (present, not absent).
    let diag = section(&empty_page, DIAG_HEADING);
    assert!(
        diag.contains(DIAG_NONE),
        "an attached-but-empty diagnostics digest must render the honest 'no diagnostics' \
         text, got:\n{diag}"
    );
    let loss = section(&empty_page, LOSS_HEADING);
    assert!(
        loss.contains(LOSS_NONE),
        "an attached-but-empty loss digest must render the honest 'carried exactly' \
         text, got:\n{loss}"
    );
}

#[test]
fn term_entailments_absent_without_exec_data() {
    // Empty-vs-absent, B3 side: with the DEFAULT (empty) `ExecutableDocsData` the
    // "Inferred facts" panel is simply ABSENT — never a fabricated "no
    // entailments" claim. This locks the genuine layering seam (see exec.rs) so a
    // refactor cannot make the model-only render emit a vacuous panel.
    let model = common::cached_model();
    let term = first_term(&model);
    let md = to_markdown_exec(
        &model,
        &Page::Term(term_slug(term)),
        &ExecutableDocsData::default(),
    );
    assert!(
        !md.contains("## Inferred facts"),
        "the entailments panel must be absent (not empty) in a model-only render"
    );
}
