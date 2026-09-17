// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::ContextualScope;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn authenticated_surface() -> String {
    let bytes = crate::fixture::authenticated_artifact(
        &repo_root(),
        "stage-export-skos-surface",
        SKOS_SURFACE_PATH,
    )
    .expect("load the authenticated SKOS-surface product without rebuilding it");
    String::from_utf8(bytes).expect("authenticated SKOS surface is UTF-8")
}

#[test]
fn authenticated_skos_surface_is_nonempty_valid_turtle() {
    let ttl = authenticated_surface();
    let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("authenticated SKOS surface parses as valid Turtle");
    assert!(dataset.quad_count() > 0, "SKOS surface must not be empty");
}

/// Shift-left: drive the SAME native structural lint `make validate`/`make check`
/// runs over this generator's real output, so a missing/incorrect A-Box annotation
/// on the minted concept-scheme individual reds HERE (a fast `cargo nextest`) rather
/// than only at the next expensive gate (mirrors
/// `frame_shapes::tests::minted_shapes_satisfy_the_assertional_abox_contract`).
#[test]
fn skos_surface_lint_green() {
    use gmeow_validate::lint::{
        LintConfig, default_annotation_predicates, structural_lint_dataset,
    };

    let ttl = authenticated_surface();
    // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the kernel
    // slice; add it here so the graphBoxRole-typing check has its declaration to
    // resolve against. Self-describe it (`rdfs:isDefinedBy <ns>self`) so the bare
    // role individual is exempt from the per-term contract (as the kernel definition
    // is), keeping the whole report clean.
    let doc = format!(
        "{ttl}\n<{NS}boxABox> a <{NS}GraphBoxRole> ; \
             <http://www.w3.org/2000/01/rdf-schema#isDefinedBy> <{NS}self> .\n"
    );
    let native = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
        .expect("parse skos-surface into native dataset");

    let cfg = LintConfig {
        namespace: NS.to_string(),
        ontology_iri: NS.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: default_annotation_predicates().into_iter().collect(),
    };
    let report = structural_lint_dataset(&native, &cfg);
    assert!(
        report.errors().is_empty(),
        "the generated SKOS surface must be structural-lint-clean: {:?}",
        report.errors()
    );
}

#[test]
fn skos_surface_non_vacuous() {
    // A stable, GMEOW-authored `skos:definition` (gmeow:Agreement) must survive the
    // lift → projection round-trip into the rendered surface.
    let ttl = authenticated_surface();
    assert!(
        ttl.contains("mutual understanding between two or more agents"),
        "a known GMEOW term's skos:definition must appear in the rendered surface"
    );
}

/// AC1 anti-drift (the load-bearing test): the renderer is a PURE FUNCTION of the
/// lifted axiom set, NOT a raw source re-read. An injected sentinel definition — a
/// string that appears in NO source file — must reach the rendered output when it is
/// passed as a `NodeKind::Annotation` axiom, proving the surface is built from the
/// axioms it is handed.
#[test]
fn skos_surface_renders_from_lifted_axioms() {
    let sentinel = "ZZZ_INJECTED_SENTINEL_DEFINITION";
    let injected = vec![
        LogicAxiom::new(
            format!("{NS}InjectedSentinelTerm"),
            "http://www.w3.org/2004/02/skos/core#definition",
            gmeow_logic_compile::ir::AtomicTerm::Literal(purrdf::RdfLiteral::simple(sentinel)),
            // obj_is_literal
            false,
            // negated
            ContextualScope::default(),
        )
        .expect("valid axiom")
        .with_node_kind(NodeKind::Annotation),
    ];
    let ttl = render_skos_surface_from_axioms(&injected).expect("render from axioms");
    assert!(
        ttl.contains(sentinel),
        "the renderer must consume the passed axiom set (AC1 anti-drift): {ttl}"
    );
}
