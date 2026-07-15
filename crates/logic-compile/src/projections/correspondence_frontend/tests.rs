// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::CorrespondenceRelation;
use crate::projections::correspondence::{extract_correspondences, project_correspondence};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";

/// Build a representative `dsl/mappings/` fixture: two `gmeow:TermEquivalence` cells (an
/// `exactMatch` and a `closeMatch`, so two distinct relation bands) plus one
/// `gmeow:ProjectionMapping` carrying a single per-profile binding.
fn fixture_dsl() -> std::sync::Arc<purrdf::RdfDataset> {
    use purrdf::{RdfDatasetBuilder, RdfLiteral};

    let mut b = RdfDatasetBuilder::new();
    let triple =
        |b: &mut RdfDatasetBuilder, s: &str, p: &str, o_iri: Option<&str>, o_lit: Option<&str>| {
            let s = b.intern_iri(s);
            let p = b.intern_iri(p);
            let o = match (o_iri, o_lit) {
                (Some(o), _) => b.intern_iri(o),
                (_, Some(l)) => b.intern_literal(RdfLiteral::simple(l.to_owned())),
                _ => unreachable!(),
            };
            b.push_quad(s, p, o, None);
        };

    // ── Two TermEquivalence cells ──────────────────────────────────────────────────
    let eq1 = format!("{GMEOW}eq1");
    triple(
        &mut b,
        &eq1,
        RDF_TYPE,
        Some(&format!("{GMEOW}TermEquivalence")),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}alignSubject"),
        Some(&format!("{GMEOW}Foo")),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}alignPredicate"),
        Some(SKOS_EXACT_MATCH),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}alignObject"),
        Some(&format!("{GMEOW}Bar")),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}sssomFile"),
        None,
        Some("demo.sssom.tsv"),
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}confidence"),
        None,
        Some("1.0"),
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}justification"),
        Some("https://w3id.org/semapv/vocab/ManualMappingCuration"),
        None,
    );

    let eq2 = format!("{GMEOW}eq2");
    triple(
        &mut b,
        &eq2,
        RDF_TYPE,
        Some(&format!("{GMEOW}TermEquivalence")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        RDF_TYPE,
        Some(&format!("{LOGIC}GroundingCorrespondence")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}alignSubject"),
        Some(&format!("{GMEOW}Baz")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}alignPredicate"),
        Some(SKOS_CLOSE_MATCH),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}alignObject"),
        Some(&format!("{GMEOW}Qux")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}sssomFile"),
        None,
        Some("demo.sssom.tsv"),
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}confidence"),
        None,
        Some("0.8"),
    );
    triple(
        &mut b,
        &eq2,
        &format!("{LOGIC}morphismClass"),
        Some(&format!("{LOGIC}AffineCorrespondence")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{LOGIC}morphismKind"),
        Some(&format!("{LOGIC}InstitutionMorphism")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{LOGIC}preservationKind"),
        Some(&format!("{LOGIC}ValidationOnly")),
        None,
    );

    // ── One ProjectionMapping with one per-profile binding ─────────────────────────
    let pm = format!("{GMEOW}pm1");
    let pat = format!("{GMEOW}pm1/pattern");
    let bind = format!("{GMEOW}pm1/binding/schema-org");
    triple(
        &mut b,
        &pm,
        RDF_TYPE,
        Some(&format!("{GMEOW}ProjectionMapping")),
        None,
    );
    triple(
        &mut b,
        &pm,
        &format!("{GMEOW}hasMappingPattern"),
        Some(&pat),
        None,
    );
    triple(&mut b, &pat, &format!("{GMEOW}anchor"), None, Some("?s"));
    triple(
        &mut b,
        &pm,
        &format!("{GMEOW}hasBinding"),
        Some(&bind),
        None,
    );
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}profile"),
        None,
        Some("schema-org"),
    );
    triple(&mut b, &bind, &format!("{GMEOW}relation"), None, Some("="));
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}toPredicate"),
        Some("https://schema.org/name"),
        None,
    );
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}confidence"),
        None,
        Some("0.9"),
    );

    b.freeze().expect("freeze fixture dsl")
}

fn parse_nt(nt: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection N-Triples")
}

/// A single term-level grounding bridge fixture. `metadata=false` deliberately omits the
/// three required authored judgments so the fail-closed frontend can be tested directly.
fn term_grounding_dsl(predicate: &str, metadata: bool) -> std::sync::Arc<purrdf::RdfDataset> {
    use purrdf::{RdfDatasetBuilder, RdfLiteral};

    let mut b = RdfDatasetBuilder::new();
    let triple =
        |b: &mut RdfDatasetBuilder, s: &str, p: &str, o_iri: Option<&str>, o_lit: Option<&str>| {
            let s = b.intern_iri(s);
            let p = b.intern_iri(p);
            let o = match (o_iri, o_lit) {
                (Some(o), _) => b.intern_iri(o),
                (_, Some(l)) => b.intern_literal(RdfLiteral::simple(l.to_owned())),
                _ => unreachable!(),
            };
            b.push_quad(s, p, o, None);
        };
    let cell = format!("{GMEOW}groundingBridge");
    for ty in [
        format!("{GMEOW}TermEquivalence"),
        format!("{LOGIC}GroundingCorrespondence"),
    ] {
        triple(&mut b, &cell, RDF_TYPE, Some(&ty), None);
    }
    triple(
        &mut b,
        &cell,
        &format!("{GMEOW}alignSubject"),
        Some(&format!("{LOGIC}Object")),
        None,
    );
    triple(
        &mut b,
        &cell,
        &format!("{GMEOW}alignPredicate"),
        Some(predicate),
        None,
    );
    triple(
        &mut b,
        &cell,
        &format!("{GMEOW}alignObject"),
        Some("http://purl.obolibrary.org/obo/BFO_0000040"),
        None,
    );
    triple(
        &mut b,
        &cell,
        &format!("{GMEOW}sssomFile"),
        None,
        Some("grounding.sssom.tsv"),
    );
    if metadata {
        for (property, value) in [
            ("morphismClass", "BridgeView"),
            ("morphismKind", "CommitmentShiftingBridge"),
            ("preservationKind", "ValidationOnly"),
        ] {
            triple(
                &mut b,
                &cell,
                &format!("{LOGIC}{property}"),
                Some(&format!("{LOGIC}{value}")),
                None,
            );
        }
    }
    b.freeze().expect("freeze term grounding fixture")
}

/// The transpiler materializes one typed node per cell, and the projected program
/// ROUND-TRIPS through its backing graph: `extract_correspondences` re-derives the exact
/// typed nodes (content identity) AND returns NO errors (a malformed transpile must not
/// pass silently — extract is fail-soft, so an empty error vec is the real assertion).
#[test]
fn transpile_round_trips_with_no_extract_errors() {
    let dsl = fixture_dsl();
    // The frontend accepts an ontology view for symmetry; an empty one suffices here.
    let empty = parse_nt("");
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&empty);

    let program =
        transpile_correspondences(&dsl_view, &onto_view).expect("transpile the fixture cells");

    // Two TermEquivalence cells + one ProjectionMapping binding = three typed nodes.
    assert_eq!(
        program.correspondences.len(),
        3,
        "two term-equivalences and one projection binding materialize three nodes"
    );

    // The relation bands come from the SHARED derivations: exactMatch → Equiv,
    // closeMatch → Overlaps, the `=` binding → Equiv.
    let relations: Vec<CorrespondenceRelation> =
        program.correspondences.iter().map(|c| c.relation).collect();
    assert_eq!(
        relations
            .iter()
            .filter(|r| **r == CorrespondenceRelation::Equiv)
            .count(),
        2,
        "the exactMatch cell and the `=` binding are both Equiv"
    );
    assert!(
        relations.contains(&CorrespondenceRelation::Overlaps),
        "the closeMatch cell is an Overlaps"
    );

    let grounding = program
        .correspondences
        .iter()
        .find(|c| c.grounding)
        .expect("the explicitly-authored grounding cell is retained as such");
    assert_eq!(
        grounding.source_endpoint.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/Baz")
    );
    assert_eq!(
        grounding.target_endpoint.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/Qux")
    );
    assert_eq!(
        grounding.morphism_class,
        MorphismClass::AffineCorrespondence
    );
    assert_eq!(grounding.morphism_kind, MorphismKind::InstitutionMorphism);
    assert_eq!(
        grounding.preservation,
        Some(PreservationKind::ValidationOnly)
    );

    // Project the typed program to its backing graph, then re-extract the bare nodes.
    let nt = project_correspondence(&program);
    let backing = parse_nt(&nt);
    let (extracted, errors) = extract_correspondences(&backing);

    // (b) Extract is fail-soft per-node; a malformed transpile would surface here — the
    // empty error vec is the load-bearing assertion that the transpile is well-formed.
    assert!(
        errors.is_empty(),
        "extract_correspondences must report no malformed nodes: {errors:?}"
    );

    // (a) The typed nodes round-trip by content identity: same count and field-for-field
    // equal (both sides are sorted by IRI — the program ctor and `extract` both order
    // ascending — so a positional compare is a content compare).
    assert_eq!(
        extracted.len(),
        program.correspondences.len(),
        "every materialized node round-trips through the backing graph"
    );
    assert_eq!(
        extracted, program.correspondences,
        "the re-extracted nodes are content-identical to the transpiled set"
    );
}

/// The correspondence IRIs are content-addressed and STABLE across re-runs: transpiling
/// the same corpus twice yields byte-identical node identities (no run-to-run drift).
#[test]
fn correspondence_iris_are_content_stable() {
    let dsl = fixture_dsl();
    let empty = parse_nt("");
    let onto_view = DslView::new(&empty);

    let a = transpile_correspondences(&DslView::new(&dsl), &onto_view).expect("transpile a");
    let b = transpile_correspondences(&DslView::new(&dsl), &onto_view).expect("transpile b");

    let iris_a: Vec<&str> = a.correspondences.iter().map(|c| c.iri.as_str()).collect();
    let iris_b: Vec<&str> = b.correspondences.iter().map(|c| c.iri.as_str()).collect();
    assert_eq!(iris_a, iris_b, "content-addressed IRIs are run-stable");
    // And the whole program keys identically.
    assert_eq!(a.content_key(), b.content_key());
}

#[test]
fn grounding_term_bridge_keeps_typed_commitment_and_endpoints() {
    let dsl = term_grounding_dsl(SKOS_CLOSE_MATCH, true);
    let empty = parse_nt("");
    let program = transpile_correspondences(&DslView::new(&dsl), &DslView::new(&empty))
        .expect("transpile the grounding bridge");
    let [bridge] = program.correspondences.as_slice() else {
        panic!("one authored grounding cell must produce one correspondence")
    };
    assert!(bridge.grounding);
    assert_eq!(bridge.morphism_class, MorphismClass::BridgeView);
    assert_eq!(bridge.morphism_kind, MorphismKind::CommitmentShiftingBridge);
    assert_eq!(bridge.preservation, Some(PreservationKind::ValidationOnly));
    assert_eq!(
        bridge.source_endpoint.as_deref(),
        Some("https://blackcatinformatics.ca/logic/Object")
    );
    assert_eq!(
        bridge.target_endpoint.as_deref(),
        Some("http://purl.obolibrary.org/obo/BFO_0000040")
    );
}

#[test]
fn grounding_term_bridge_requires_explicit_judgments() {
    let dsl = term_grounding_dsl(SKOS_CLOSE_MATCH, false);
    let empty = parse_nt("");
    let err = transpile_correspondences(&DslView::new(&dsl), &DslView::new(&empty))
        .expect_err("a grounding bridge with implicit defaults must fail closed");
    assert!(err.message().contains("must explicitly author"), "{err}");
}

#[test]
fn grounding_term_bridge_cannot_surface_exact_match() {
    use crate::projections::sssom::lower_sssom;

    let dsl = term_grounding_dsl(SKOS_EXACT_MATCH, true);
    let empty = parse_nt("");
    let view = DslView::new(&dsl);
    let (_program, lookup) = transpile_correspondences_indexed(&view, &DslView::new(&empty))
        .expect("the typed bridge materializes before the dialect gate");
    let err = match lower_sssom(&view, "test", "2026-01-01", &lookup) {
        Err(err) => err,
        Ok(_) => panic!("a commitment-shifting bridge must not emit exactMatch"),
    };
    assert!(err.message().contains("bridge"), "{err}");
    assert!(err.message().contains("Principle 5"), "{err}");
}

/// A `dsl/mappings/` fixture with ONE `gmeow:ProjectionMapping` whose single per-profile
/// binding declares itself a commitment-shifting `logic:BridgeView` (`gmeow:morphismClass`)
/// while its EDOAL relation token is the equivalence symbol `=`. This is the authored
/// shape of a bridge view trying to surface an equivalence predicate — the negative case
/// the overclaim gate must refuse (Constitution Principle 5). When `bridge` is false the
/// SAME cell is a plain (non-bridge) equivalence binding — the control that must pass.
fn projection_bridge_dsl(bridge: bool) -> std::sync::Arc<purrdf::RdfDataset> {
    use purrdf::{RdfDatasetBuilder, RdfLiteral};

    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
    let mut b = RdfDatasetBuilder::new();
    let triple =
        |b: &mut RdfDatasetBuilder, s: &str, p: &str, o_iri: Option<&str>, o_lit: Option<&str>| {
            let s = b.intern_iri(s);
            let p = b.intern_iri(p);
            let o = match (o_iri, o_lit) {
                (Some(o), _) => b.intern_iri(o),
                (_, Some(l)) => b.intern_literal(RdfLiteral::simple(l.to_owned())),
                _ => unreachable!(),
            };
            b.push_quad(s, p, o, None);
        };

    let pm = format!("{GMEOW}pmBridge");
    let pat = format!("{GMEOW}pmBridge/pattern");
    let bind = format!("{GMEOW}pmBridge/binding/schema-org");
    triple(
        &mut b,
        &pm,
        RDF_TYPE,
        Some(&format!("{GMEOW}ProjectionMapping")),
        None,
    );
    triple(
        &mut b,
        &pm,
        &format!("{GMEOW}hasMappingPattern"),
        Some(&pat),
        None,
    );
    triple(&mut b, &pat, &format!("{GMEOW}anchor"), None, Some("s"));
    // An edoalSource so the EDOAL lowering emits a real cell carrying the `=` relation.
    triple(
        &mut b,
        &pat,
        &format!("{GMEOW}edoalSource"),
        Some(&format!("{GMEOW}Foo")),
        None,
    );
    triple(
        &mut b,
        &pat,
        &format!("{GMEOW}edoalSourceKind"),
        None,
        Some("class"),
    );
    triple(
        &mut b,
        &pm,
        &format!("{GMEOW}hasBinding"),
        Some(&bind),
        None,
    );
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}profile"),
        None,
        Some("schema-org"),
    );
    // The EDOAL relation token is the equivalence symbol `=` …
    triple(&mut b, &bind, &format!("{GMEOW}relation"), None, Some("="));
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}toClass"),
        Some("https://schema.org/Thing"),
        None,
    );
    if bridge {
        // … but the correspondence is authored as a by-reference BridgeView, which the
        // overclaim gate then refuses (it may never assert equivalence).
        triple(
            &mut b,
            &bind,
            &format!("{GMEOW}morphismClass"),
            Some(&format!("{LOGIC}BridgeView")),
            None,
        );
    }
    b.freeze().expect("freeze bridge dsl")
}

/// PART B — the negative conformance case proving the re-seated gate is strictly stronger
/// than the old predicate-only lint: an authored `gmeow:ProjectionMapping` cell whose
/// binding is a commitment-shifting `logic:BridgeView` surfacing the equivalence token `=`
/// flows CELL → transpile (materialized typed correspondence) → lowering (which CONSUMES
/// the materialized typed relation for its gate) and HARD-FAILS the whole build — through
/// BOTH the EDOAL and the SPARQL lowerings, end-to-end (not just the bare gate helper).
///
/// The control (`bridge=false`) is the SAME cell minus the BridgeView class: it passes.
/// So the failure is attributable SOLELY to the gate consuming the materialized
/// `morphismClass=BridgeView` — remove the `assert_relation_no_overclaim` call from the
/// lowering and this case goes green, which is exactly what makes it a RED witness.
#[test]
fn bridge_cell_surfacing_equivalence_fails_end_to_end() {
    use crate::projections::{edoal::lower_edoal, sparql::lower_sparql};

    let empty = parse_nt("");
    let onto_view = DslView::new(&empty);

    // ── The RED case: the authored bridge cell flows through transpile + both lowerings. ─
    let dsl = projection_bridge_dsl(true);
    let dsl_view = DslView::new(&dsl);
    let (_program, lookup) = transpile_correspondences_indexed(&dsl_view, &onto_view)
        .expect("the bridge cell transpiles to a typed correspondence (BridgeView)");

    // `*Lowering` Ok types are not `Debug`, so match the `Result` rather than `expect_err`.
    let edoal_err = match lower_edoal(&dsl_view, &onto_view, &lookup) {
        Err(e) => e,
        Ok(_) => panic!("a BridgeView surfacing `=` must hard-fail the EDOAL lowering"),
    };
    assert!(edoal_err.message().contains("bridge"), "{edoal_err}");
    assert!(edoal_err.message().contains("Principle 5"), "{edoal_err}");

    let sparql_err = match lower_sparql(&dsl_view, &onto_view, &lookup) {
        Err(e) => e,
        Ok(_) => panic!("a BridgeView surfacing `=` must hard-fail the SPARQL lowering"),
    };
    assert!(sparql_err.message().contains("bridge"), "{sparql_err}");
    assert!(sparql_err.message().contains("Principle 5"), "{sparql_err}");

    // ── The control: the SAME cell as a plain (non-bridge) equivalence binding passes the
    // gate. We assert through the EDOAL lowering — it emits an alignment for every profile
    // regardless of which carry a binding, so the gate is the only thing that could reject
    // this cell. (The SPARQL lowering additionally requires EVERY profile to carry a
    // binding, an orthogonal whole-corpus constraint a one-binding fixture cannot satisfy;
    // the RED SPARQL assertion above still bites because `schema-org` — bound, and first in
    // PROFILES — hits the gate before any unbound profile.) The control passing shows the
    // RED case fails SOLELY because the gate consumes the authored BridgeView class.
    let ctrl_dsl = projection_bridge_dsl(false);
    let ctrl_view = DslView::new(&ctrl_dsl);
    let (_ctrl_program, ctrl_lookup) = transpile_correspondences_indexed(&ctrl_view, &onto_view)
        .expect("the control equivalence cell transpiles");
    lower_edoal(&ctrl_view, &onto_view, &ctrl_lookup)
        .expect("a genuine equivalence binding passes the EDOAL gate");
}

/// The justification → evidence-strength band is honest: a manually-curated cell carries
/// a modest non-zero warrant, an un-justified cell leaves the axis unset (never a
/// fabricated number).
#[test]
fn evidence_strength_tracks_the_justification_band() {
    assert_eq!(
        evidence_strength_of_justification(Some(
            "https://w3id.org/semapv/vocab/ManualMappingCuration"
        )),
        Some(0.5)
    );
    assert_eq!(evidence_strength_of_justification(None), None);
    assert_eq!(
        evidence_strength_of_justification(Some("https://example.org/UnknownJustification")),
        None
    );
}
