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

/// Build a representative `dsl/mappings/` fixture: two native alignment cells (an
/// `exactMatch` and a grounding `closeMatch`, so two distinct relation bands) plus one
/// `gmeow:ProjectionMapping` carrying a single per-profile binding.
fn fixture_dsl() -> std::sync::Arc<purrdf::RdfDataset> {
    let ttl = br#"
@prefix gmeow:  <https://blackcatinformatics.ca/gmeow/> .
@prefix logic:  <https://blackcatinformatics.ca/logic/> .
@prefix skos:   <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .

gmeow:Foo skos:exactMatch gmeow:Bar {|
    gmeow:sssomFile     "demo.sssom.tsv" ;
    gmeow:confidence    1.0 ;
    gmeow:justification semapv:ManualMappingCuration
|} .

gmeow:Baz skos:closeMatch gmeow:Qux {|
    a                      logic:GroundingCorrespondence ;
    gmeow:sssomFile        "demo.sssom.tsv" ;
    gmeow:confidence       0.8 ;
    gmeow:justification    semapv:ManualMappingCuration ;
    logic:sourceEndpoint   gmeow:Baz ;
    logic:targetEndpoint   gmeow:Qux ;
    logic:morphismClass    logic:AffineCorrespondence ;
    logic:morphismKind     logic:InstitutionMorphism ;
    logic:preservationKind logic:ValidationOnly
|} .

gmeow:pm1 a gmeow:ProjectionMapping ;
    gmeow:hasMappingPattern [ gmeow:anchor "?s" ] ;
    gmeow:hasBinding [
        gmeow:profile "schema-org" ;
        gmeow:relation "=" ;
        gmeow:toPredicate schema:name ;
        gmeow:confidence 0.9
    ] .
"#;
    purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse fixture dsl")
}

fn parse_nt(nt: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection N-Triples")
}

/// A single term-level grounding bridge fixture. `metadata=false` deliberately omits the
/// required authored judgments so the fail-closed frontend can be tested directly.
fn term_grounding_dsl(predicate: &str, metadata: bool) -> std::sync::Arc<purrdf::RdfDataset> {
    // The judgment block is authored only when `metadata` is set; otherwise the cell is a
    // grounding alignment cell missing its required judgments (the fail-closed case).
    let judgments = if metadata {
        "    gmeow:justification    semapv:ManualMappingCuration ;\n\
         \x20   logic:sourceEndpoint   logic:Object ;\n\
         \x20   logic:targetEndpoint   obo:BFO_0000040 ;\n\
         \x20   logic:morphismClass    logic:BridgeView ;\n\
         \x20   logic:morphismKind     logic:CommitmentShiftingBridge ;\n\
         \x20   logic:preservationKind logic:ValidationOnly ;\n"
    } else {
        ""
    };
    let ttl = format!(
        "@prefix gmeow:  <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic:  <https://blackcatinformatics.ca/logic/> .\n\
         @prefix obo:    <http://purl.obolibrary.org/obo/> .\n\
         @prefix semapv: <https://w3id.org/semapv/vocab/> .\n\
         \n\
         logic:Object <{predicate}> obo:BFO_0000040 {{|\n\
         \x20   a               logic:GroundingCorrespondence ;\n\
         {judgments}\
         \x20   gmeow:sssomFile \"grounding.sssom.tsv\"\n\
         |}} .\n"
    );
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("parse term grounding fixture")
}

/// An executable grounding correspondence. `binding_count=1` is the accepted frontend;
/// any larger value is ambiguous and must fail before materializing an IR node.
fn projection_grounding_dsl(
    binding_count: usize,
    metadata: bool,
    target_matches: bool,
    bridge_pair: bool,
) -> std::sync::Arc<purrdf::RdfDataset> {
    projection_grounding_dsl_with_binding(
        binding_count,
        metadata,
        target_matches,
        bridge_pair,
        None,
        1,
    )
}

/// The configurable grounding-projection fixture used by the fail-closed binding-envelope
/// tests. `relation` overrides the honest default for a bridge/non-bridge pair, while
/// `target_count` controls how many of the three mutually-exclusive target properties are
/// authored on each binding.
fn projection_grounding_dsl_with_binding(
    binding_count: usize,
    metadata: bool,
    target_matches: bool,
    bridge_pair: bool,
    relation: Option<&str>,
    target_count: usize,
) -> std::sync::Arc<purrdf::RdfDataset> {
    use purrdf::{RdfDatasetBuilder, RdfLiteral};

    assert!(target_count <= 3, "the fixture has three target properties");

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

    let cell = format!("{GMEOW}projectionGrounding");
    let pattern = format!("{cell}/pattern");
    for ty in [
        format!("{GMEOW}ProjectionMapping"),
        format!("{LOGIC}GroundingCorrespondence"),
    ] {
        triple(&mut b, &cell, RDF_TYPE, Some(&ty), None);
    }
    triple(
        &mut b,
        &cell,
        &format!("{GMEOW}hasMappingPattern"),
        Some(&pattern),
        None,
    );
    triple(&mut b, &pattern, &format!("{GMEOW}anchor"), None, Some("s"));
    if metadata {
        triple(
            &mut b,
            &cell,
            &format!("{GMEOW}justification"),
            Some("https://w3id.org/semapv/vocab/ManualMappingCuration"),
            None,
        );
        for (property, value) in [
            ("sourceEndpoint", format!("{GMEOW}name")),
            (
                "targetEndpoint",
                if target_matches {
                    "https://schema.org/name".to_owned()
                } else {
                    "https://schema.org/alternateName".to_owned()
                },
            ),
            (
                "morphismClass",
                if bridge_pair {
                    format!("{LOGIC}BridgeView")
                } else {
                    format!("{LOGIC}WellBehavedLens")
                },
            ),
            (
                "morphismKind",
                if bridge_pair {
                    format!("{LOGIC}CommitmentShiftingBridge")
                } else {
                    format!("{LOGIC}InstitutionMorphism")
                },
            ),
            (
                "preservationKind",
                format!("{LOGIC}SoundUnderApproximation"),
            ),
        ] {
            triple(
                &mut b,
                &cell,
                &format!("{LOGIC}{property}"),
                Some(&value),
                None,
            );
        }
    }
    for index in 0..binding_count {
        let binding = format!("{cell}/binding/{index}");
        triple(
            &mut b,
            &cell,
            &format!("{GMEOW}hasBinding"),
            Some(&binding),
            None,
        );
        triple(
            &mut b,
            &binding,
            &format!("{GMEOW}profile"),
            None,
            Some(if index == 0 { "schema-org" } else { "foaf" }),
        );
        triple(
            &mut b,
            &binding,
            &format!("{GMEOW}relation"),
            None,
            Some(relation.unwrap_or(if bridge_pair { "~" } else { "=" })),
        );
        if target_count >= 1 {
            triple(
                &mut b,
                &binding,
                &format!("{GMEOW}toPredicate"),
                Some(if index == 0 {
                    "https://schema.org/name"
                } else {
                    "http://xmlns.com/foaf/0.1/name"
                }),
                None,
            );
        }
        if target_count >= 2 {
            triple(
                &mut b,
                &binding,
                &format!("{GMEOW}toClass"),
                Some("https://schema.org/Thing"),
                None,
            );
        }
        if target_count >= 3 {
            triple(
                &mut b,
                &binding,
                &format!("{GMEOW}edoalTarget"),
                Some("https://schema.org/PropertyValue"),
                None,
            );
        }
    }
    b.freeze().expect("freeze projection grounding fixture")
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

    // Two native alignment cells + one ProjectionMapping binding = three typed nodes.
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

#[test]
fn grounding_projection_lowers_to_the_same_typed_ir() {
    let dsl = projection_grounding_dsl(1, true, true, false);
    let empty = parse_nt("");
    let program = transpile_correspondences(&DslView::new(&dsl), &DslView::new(&empty))
        .expect("transpile executable grounding correspondence");
    let [grounding] = program.correspondences.as_slice() else {
        panic!("one grounding ProjectionMapping binding must produce one correspondence")
    };
    assert!(grounding.grounding);
    assert_eq!(grounding.morphism_class, MorphismClass::WellBehavedLens);
    assert_eq!(grounding.morphism_kind, MorphismKind::InstitutionMorphism);
    assert_eq!(grounding.preservation, Some(PreservationKind::SoundUnder));
    assert_eq!(
        grounding.source_endpoint.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/name")
    );
    assert_eq!(
        grounding.target_endpoint.as_deref(),
        Some("https://schema.org/name")
    );
    assert_eq!(grounding.evidence_strength, Some(0.5));
}

#[test]
fn grounding_projection_rejects_multiple_bindings() {
    let dsl = projection_grounding_dsl(2, true, true, false);
    let empty = parse_nt("");
    let err = transpile_correspondences(&DslView::new(&dsl), &DslView::new(&empty))
        .expect_err("a grounding ProjectionMapping cannot have an ambiguous target");
    assert!(err.message().contains("exactly one"), "{err}");
}

#[test]
fn grounding_projection_rejects_missing_metadata_and_target_drift() {
    let empty = parse_nt("");
    let missing = projection_grounding_dsl(1, false, true, false);
    let err = transpile_correspondences(&DslView::new(&missing), &DslView::new(&empty))
        .expect_err("an implicit executable grounding mapping must fail closed");
    assert!(err.message().contains("must explicitly author"), "{err}");

    let drifted = projection_grounding_dsl(1, true, false, false);
    let err = transpile_correspondences(&DslView::new(&drifted), &DslView::new(&empty))
        .expect_err("the explicit target endpoint must match the binding target");
    assert!(err.message().contains("targetEndpoint must equal"), "{err}");
}

#[test]
fn grounding_projection_accepts_an_honest_commitment_shift() {
    let dsl = projection_grounding_dsl(1, true, true, true);
    let empty = parse_nt("");
    let program = transpile_correspondences(&DslView::new(&dsl), &DslView::new(&empty))
        .expect("a non-equivalence bridge pair is an honest grounding correspondence");
    let [bridge] = program.correspondences.as_slice() else {
        panic!("one binding must produce one correspondence")
    };
    assert_eq!(bridge.morphism_class, MorphismClass::BridgeView);
    assert_eq!(bridge.morphism_kind, MorphismKind::CommitmentShiftingBridge);
    assert_ne!(bridge.relation, CorrespondenceRelation::Equiv);
}

#[test]
fn grounding_projection_rejects_a_commitment_shift_with_equivalence_relation() {
    let dsl = projection_grounding_dsl_with_binding(1, true, true, true, Some("="), 1);
    let empty = parse_nt("");
    let err = transpile_correspondences(&DslView::new(&dsl), &DslView::new(&empty))
        .expect_err("a commitment-shifting bridge may not materialize as equivalence");
    assert!(
        err.message().contains("must not declare an equivalence"),
        "{err}"
    );
}

#[test]
fn grounding_projection_requires_exactly_one_binding_target_form() {
    let empty = parse_nt("");
    for target_count in [0, 2] {
        let dsl = projection_grounding_dsl_with_binding(1, true, true, false, None, target_count);
        let err = transpile_correspondences(&DslView::new(&dsl), &DslView::new(&empty))
            .expect_err("a grounding binding target must be unambiguous and present");
        assert!(err.message().contains("exactly one of"), "{err}");
        assert!(
            err.message().contains(&format!("found {target_count}")),
            "{err}"
        );
    }
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
