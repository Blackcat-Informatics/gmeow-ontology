// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::projections::get_leg::projections;

#[test]
fn stable_bnode_encodes_injectively() {
    // Alphanumerics pass through; `-` (0x2d) is hex-escaped.
    assert_eq!(Nt::stable_bnode("cell-foaf-x-0"), "_:ncell_2dfoaf_2dx_2d0");
    // The old `non-alnum → '_'` collapse mapped these two distinct labels to the
    // same id; the injective encoding keeps them distinct.
    assert_ne!(
        Nt::stable_bnode("cell-a_b-0"),
        Nt::stable_bnode("cell-a-b-0")
    );
    // A literal underscore is itself escaped, so the encoding is unambiguous.
    assert_eq!(Nt::stable_bnode("a_b"), "_:na_5fb");
}

#[test]
fn format_double_matches_corpus() {
    assert_eq!(format_double(0.8), "0.8");
    assert_eq!(format_double(0.95), "0.95");
    assert_eq!(format_double(0.6), "0.6");
    assert_eq!(format_double(1.0), "1");
}

/// RED witness driving the overclaim gate THROUGH the real EDOAL lowering: a cell
/// authored as a `BridgeView` whose EDOAL relation symbol is the equivalence token
/// `=` must make the lowering return `Err` (Constitution Principle 5 — a bridge view
/// may never assert equivalence). This exercises the gate at the production call
/// site, not only the bare gate function.
#[test]
fn bridge_cell_emitting_equivalence_fails_the_lowering() {
    use crate::ir::MorphismClass;
    use crate::projections::correspondence_frontend::{CorrespondenceAnalysis, TypedRelation};

    let gm = "https://blackcatinformatics.ca/gmeow/";
    let bridge_cell = ProjectionCell {
        iri: format!("{gm}cellBridge"),
        label: "bridge".to_owned(),
        pattern: MappingPattern {
            anchor: "x".to_owned(),
            value: None,
            atoms: Vec::new(),
            suppress_when: Vec::new(),
            project_when: Vec::new(),
            exclude_when: Vec::new(),
            filters: Vec::new(),
            binds: Vec::new(),
            mints: Vec::new(),
            edoal_source: Some(format!("{gm}Foo")),
            edoal_source_kind: Some("class".to_owned()),
            edoal_path: false,
        },
        bindings: vec![ProfileBinding {
            profile: "schema-org".to_owned(),
            to_predicate: None,
            to_class: Some(format!("{gm}Bar")),
            template_atoms: Vec::new(),
            value_class_map: Vec::new(),
            // The EDOAL relation symbol is the equivalence token `=` …
            relation: "=".to_owned(),
            transform: None,
            confidence: None,
            lossy_drops: Vec::new(),
            edoal_target: None,
            edoal_target_kind: Some("class".to_owned()),
            // … but the correspondence is authored as a by-reference BridgeView.
            morphism_class: Some(MorphismClass::BridgeView),
            ingest_claim: None,
            ingest_residue: Vec::new(),
            mnemomorphic: false,
            emit_sssom: false,
            sssom_predicate: None,
            sssom_file: None,
        }],
        grounding: None,
    };
    let tag_map = BTreeMap::new();
    // The materialized correspondence for this binding: the relation `=` lattices to
    // Equiv, the authored class is BridgeView, the kind is InstitutionMorphism — the
    // exact triple `b.lattice()` (and so the transpiler) would mint. The gate consumes
    // this typed envelope, which forbids a BridgeView surfacing equivalence.
    let lookup = CorrespondenceAnalysis::for_binding_test(
        &bridge_cell,
        &bridge_cell.bindings[0],
        TypedRelation {
            relation: crate::ir::CorrespondenceRelation::Equiv,
            morphism_class: MorphismClass::BridgeView,
            morphism_kind: crate::ir::MorphismKind::InstitutionMorphism,
        },
    );
    let mut loss = LossLedger::new();
    // The overclaim gate fires before any entity kind is resolved, so the ontology
    // view is unused here — an empty view suffices.
    let onto_ds = ds("");
    let onto = DslView::new(&onto_ds);
    let err = emit_edoal_nt(
        &[bridge_cell],
        "schema-org",
        &onto,
        &tag_map,
        &lookup,
        &mut loss,
    )
    .expect_err("a bridge view emitting `=` must be rejected by the lowering");
    assert!(err.message().contains("bridge"), "{err}");
    assert!(err.message().contains("Principle 5"), "{err}");
}

// ── Entity-kind derivation rejects EDOAL-mistyped predicates ───────────────────

/// Parse Turtle into a frozen dataset for an ontology view (native lenient codec so
/// `@x-gmeow-*` tags parse — mirrors the pipeline file edge, which reads file bytes).
fn ds(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    parse_dataset(ttl.as_bytes(), NativeRdfFormat::Turtle.media_type(), None)
        .expect("parse fixture turtle")
}

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_PREFIX: &str = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                              @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n";

/// A minimal `toPredicate` projection cell whose single GMEOW `edoalSource` feeds the
/// derivation, with optionally-authored source/target kind overrides.
fn predicate_cell(
    source: &str,
    source_kind: Option<&str>,
    target: &str,
    target_kind: Option<&str>,
) -> ProjectionCell {
    ProjectionCell {
        iri: format!("{GM}cellDerive"),
        label: String::new(),
        pattern: MappingPattern {
            anchor: "x".to_owned(),
            value: None,
            atoms: Vec::new(),
            suppress_when: Vec::new(),
            project_when: Vec::new(),
            exclude_when: Vec::new(),
            filters: Vec::new(),
            binds: Vec::new(),
            mints: Vec::new(),
            edoal_source: Some(source.to_owned()),
            edoal_source_kind: source_kind.map(str::to_owned),
            edoal_path: false,
        },
        bindings: vec![ProfileBinding {
            profile: "sioc".to_owned(),
            to_predicate: Some(target.to_owned()),
            to_class: None,
            template_atoms: Vec::new(),
            value_class_map: Vec::new(),
            relation: "<=".to_owned(),
            transform: None,
            confidence: None,
            lossy_drops: Vec::new(),
            edoal_target: None,
            edoal_target_kind: target_kind.map(str::to_owned),
            morphism_class: None,
            ingest_claim: None,
            ingest_residue: Vec::new(),
            mnemomorphic: false,
            emit_sssom: false,
            sssom_predicate: None,
            sssom_file: None,
        }],
        grounding: None,
    }
}

/// Run `edoal_cells` (bypassing the overclaim gate) and return the emitted N-Triples.
fn emit_kind_nt(onto: &DslView, cell: &ProjectionCell) -> gmeow_errors::Result<String> {
    let mut nt = Nt::new();
    let b = &cell.bindings[0];
    let cells = edoal_cells(&mut nt, onto, cell, b, "x-gmeow-english")?;
    assert!(!cells.is_empty(), "expected a cell to be emitted");
    Ok(nt.lines)
}

#[test]
fn object_property_source_derives_relation() {
    let onto_ds = ds(&format!(
        "{OWL_PREFIX} gm:hasCreator a owl:ObjectProperty ."
    ));
    let onto = DslView::new(&onto_ds);
    // No authored kind on either side: the target kind is DERIVED from the object
    // property source, so entity2 is edoal:Relation (not the old silent Property).
    let cell = predicate_cell(
        &format!("{GM}hasCreator"),
        None,
        "http://rdfs.org/sioc/ns#has_creator",
        None,
    );
    let nt = emit_kind_nt(&onto, &cell).expect("derivation succeeds");
    assert!(nt.contains(&format!("{EDOAL}Relation")), "{nt}");
    assert!(!nt.contains(&format!("{EDOAL}Property")), "{nt}");
}

#[test]
fn datatype_property_source_derives_property() {
    let onto_ds = ds(&format!(
        "{OWL_PREFIX} gm:fullName a owl:DatatypeProperty ."
    ));
    let onto = DslView::new(&onto_ds);
    let cell = predicate_cell(
        &format!("{GM}fullName"),
        None,
        "http://rdfs.org/sioc/ns#name",
        None,
    );
    let nt = emit_kind_nt(&onto, &cell).expect("derivation succeeds");
    assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
    assert!(!nt.contains(&format!("{EDOAL}Relation")), "{nt}");
}

#[test]
fn authored_target_kind_overrides_derivation() {
    // Source is an object property (would derive Relation) but the binding authors an
    // explicit override — the override wins.
    let onto_ds = ds(&format!(
        "{OWL_PREFIX} gm:hasCreator a owl:ObjectProperty ."
    ));
    let onto = DslView::new(&onto_ds);
    let cell = predicate_cell(
        &format!("{GM}hasCreator"),
        Some("relation"),
        "http://rdfs.org/sioc/ns#name",
        Some("property"),
    );
    let nt = emit_kind_nt(&onto, &cell).expect("override succeeds");
    // entity2 (target) honors the "property" override.
    assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
}

#[test]
fn indeterminate_source_kind_is_a_hard_fail() {
    // The GMEOW source carries no owl:*Property/Class type and no override is authored.
    let onto_ds = ds(OWL_PREFIX);
    let onto = DslView::new(&onto_ds);
    let cell = predicate_cell(
        &format!("{GM}untyped"),
        None,
        "http://rdfs.org/sioc/ns#name",
        None,
    );
    let err = emit_kind_nt(&onto, &cell).expect_err("indeterminate kind must hard-fail");
    assert!(err.message().contains("indeterminate"), "{err}");
}

#[test]
fn annotation_property_derives_kind_from_range() {
    // An annotation property carries no object/datatype OWL character; a datatype
    // (xsd) range makes it a `property`, a class/IRI range a `relation`.
    let onto_ds = ds(&format!(
        "{OWL_PREFIX}@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gm:validFrom a owl:AnnotationProperty ; rdfs:range xsd:dateTime .\n\
             gm:seeThing a owl:AnnotationProperty ; rdfs:range gm:Thing .",
    ));
    let onto = DslView::new(&onto_ds);
    assert_eq!(
        gmeow_entity_kind(&onto, &format!("{GM}validFrom")),
        Some("property")
    );
    assert_eq!(
        gmeow_entity_kind(&onto, &format!("{GM}seeThing")),
        Some("relation")
    );
    // No range → indeterminate (the caller then requires an override or hard-fails).
    let bare_ds = ds(&format!("{OWL_PREFIX} gm:bare a owl:AnnotationProperty ."));
    let bare = DslView::new(&bare_ds);
    assert_eq!(gmeow_entity_kind(&bare, &format!("{GM}bare")), None);
}

// ── G3: OWL 2 object-property subtypes carry object character even without an
// explicit `owl:ObjectProperty` co-assertion ────────────────────────────────────

#[test]
fn object_property_subtype_alone_derives_relation() {
    // A term typed ONLY `owl:SymmetricProperty` (no co-asserted `owl:ObjectProperty`)
    // is still, by OWL 2 semantics, an object property — `gmeow_entity_kind` must not
    // derive `None` (which would HARD-FAIL the build) for it.
    let sym_ds = ds(&format!(
        "{OWL_PREFIX} gm:sibling a owl:SymmetricProperty ."
    ));
    let sym = DslView::new(&sym_ds);
    assert_eq!(
        gmeow_entity_kind(&sym, &format!("{GM}sibling")),
        Some("relation")
    );

    let trans_ds = ds(&format!(
        "{OWL_PREFIX} gm:ancestor a owl:TransitiveProperty ."
    ));
    let trans = DslView::new(&trans_ds);
    assert_eq!(
        gmeow_entity_kind(&trans, &format!("{GM}ancestor")),
        Some("relation")
    );
}

// ── G4: an annotation property ranged on an RDF-namespace datatype (rdf:langString,
// rdf:HTML, rdf:PlainLiteral) is a literal-valued `property`, not a `relation` ────

#[test]
fn annotation_property_range_rdf_lang_string_derives_property() {
    let onto_ds = ds(&format!(
        "{OWL_PREFIX}@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gm:label2 a owl:AnnotationProperty ; rdfs:range rdf:langString .\n",
    ));
    let onto = DslView::new(&onto_ds);
    assert_eq!(
        range_entity_kind(&onto, &format!("{GM}label2")),
        Some("property")
    );
}

#[test]
fn edoal_kind_rejects_unknown_token() {
    assert_eq!(edoal_kind("relation").unwrap(), "Relation");
    assert_eq!(edoal_kind("property").unwrap(), "Property");
    assert_eq!(edoal_kind("class").unwrap(), "Class");
    assert!(edoal_kind("relaton").is_err());
    assert!(valid_kind("relaton").is_err());
}

// ── Template-derived target kind (G1: EDOAL target kind must come from the
// correspondence TEMPLATE, never the GMEOW source predicate's OWL character) ──────

use crate::projections::get_leg::{Bind, Expr, Item};

fn plain_atom(subject_var: &str, predicate: &str, object_var: &str) -> Atom {
    Atom {
        subject_var: subject_var.to_owned(),
        predicate: Some(predicate.to_owned()),
        predicate_var: None,
        path: None,
        path_alts: Vec::new(),
        object_var: Some(object_var.to_owned()),
        object_value: None,
        object_literal: None,
        optional: false,
    }
}

fn typed_atom(subject_var: &str, class_iri: &str) -> Atom {
    Atom {
        subject_var: subject_var.to_owned(),
        predicate: Some(RDF_TYPE.to_owned()),
        predicate_var: None,
        path: None,
        path_alts: Vec::new(),
        object_var: None,
        object_value: Some(class_iri.to_owned()),
        object_literal: None,
        optional: false,
    }
}

/// A `toPredicate` binding whose target is built from a TEMPLATE (`templateAtoms`),
/// not a direct 1:1 predicate — the shape `owl-time`'s `mapTimeHasBeginning` and
/// friends use.
fn templated_cell(
    source_pred: &str,
    source_kind_decl: &str,
    mints: Vec<Bind>,
    template_atoms: Vec<Atom>,
    to_predicate: &str,
) -> (ProjectionCell, std::sync::Arc<purrdf::RdfDataset>) {
    let onto_ds = ds(&format!(
        "{OWL_PREFIX} gm:{source_pred} a owl:{source_kind_decl} ."
    ));
    let cell = ProjectionCell {
        iri: format!("{GM}cellTemplated"),
        label: String::new(),
        pattern: MappingPattern {
            anchor: "s".to_owned(),
            value: None,
            atoms: vec![Item::Atom(plain_atom(
                "s",
                &format!("{GM}{source_pred}"),
                "v",
            ))],
            suppress_when: Vec::new(),
            project_when: Vec::new(),
            exclude_when: Vec::new(),
            filters: Vec::new(),
            binds: Vec::new(),
            mints,
            edoal_source: Some(format!("{GM}{source_pred}")),
            edoal_source_kind: None,
            edoal_path: false,
        },
        bindings: vec![ProfileBinding {
            profile: "owl-time".to_owned(),
            to_predicate: Some(to_predicate.to_owned()),
            to_class: None,
            template_atoms,
            value_class_map: Vec::new(),
            relation: "<=".to_owned(),
            transform: None,
            confidence: None,
            lossy_drops: Vec::new(),
            edoal_target: None,
            edoal_target_kind: None,
            morphism_class: None,
            ingest_claim: None,
            ingest_residue: Vec::new(),
            mnemomorphic: false,
            emit_sssom: false,
            sssom_predicate: None,
            sssom_file: None,
        }],
        grounding: None,
    };
    (cell, onto_ds)
}

#[test]
fn template_minted_iri_object_derives_relation_even_though_source_is_a_datatype_property() {
    // The `owl-time` shape: `gm:startedAtTime` (source, DatatypeProperty) feeds the
    // pattern's plain value var "v", but the TEMPLATE's `time:hasBeginning` atom
    // points at a MINTED "inst" var (a fresh IRI, via `opIri`), then types it
    // `time:Instant`. The target is manifestly an individual — `relation` — even
    // though the source predicate carries a literal (DatatypeProperty) character.
    // This is the exact G1 regression: the old code derived entity2 from the
    // source's OWL character and got `Property`, not `Relation`.
    let ex_beginning = "http://example.org/hasBeginning";
    let ex_instant = "http://example.org/Instant";
    let (cell, onto_ds) = templated_cell(
        "startedAtTime",
        "DatatypeProperty",
        vec![Bind {
            var: "inst".to_owned(),
            expr: Expr::Op {
                op: GM_OP_IRI.to_owned(),
                args: Vec::new(),
            },
        }],
        vec![
            plain_atom("s", ex_beginning, "inst"),
            typed_atom("inst", ex_instant),
        ],
        ex_beginning,
    );
    let onto = DslView::new(&onto_ds);
    let b = &cell.bindings[0];
    assert_eq!(
        template_target_kind(&onto, b, &cell.pattern),
        Some("relation"),
        "a minted-IRI template object is an individual, not a literal"
    );
    let nt = emit_kind_nt(&onto, &cell).expect("template derivation succeeds");
    // entity1 (source `gm:startedAtTime`) still derives its OWN kind from ITS OWN
    // OWL character (`Property`, DatatypeProperty) — entity1 resolution is
    // untouched by this fix. entity2 (target) derives `Relation` from the
    // template. The cross-kind pairing is legal under `<=` (subsumption).
    assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
    assert!(nt.contains(&format!("{EDOAL}Relation")), "{nt}");
}

#[test]
fn template_literal_var_derives_property_even_though_no_mint_or_subject_use() {
    // The `spdx:checksumValue` shape (dcat.ttl's `mapDcatChecksum`): the template's
    // `spdx:checksumValue` atom points at "digest", a var that is never minted and
    // never a template/source SUBJECT — a pure leaf. It traces back to the GMEOW
    // source atom `gm:contentDigest` (a DatatypeProperty), so it is a literal.
    let ex_checksum_value = "http://example.org/checksumValue";
    let (cell, onto_ds) = templated_cell(
        "contentDigest",
        "DatatypeProperty",
        Vec::new(),
        vec![plain_atom("chk", ex_checksum_value, "v")],
        ex_checksum_value,
    );
    let onto = DslView::new(&onto_ds);
    let b = &cell.bindings[0];
    assert_eq!(
        template_target_kind(&onto, b, &cell.pattern),
        Some("property")
    );
    let nt = emit_kind_nt(&onto, &cell).expect("template derivation succeeds");
    assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
    assert!(!nt.contains(&format!("{EDOAL}Relation")), "{nt}");
}

#[test]
fn template_atoms_present_but_no_atom_names_to_predicate_is_a_hard_fail_without_override() {
    // `template_atoms` is non-empty (so the direct source-derived fallback must NOT
    // silently kick in — Constitution no-optionality) but NO template atom names
    // `to_predicate`: `template_target_kind` returns `None`, and with no authored
    // `gmeow:edoalTargetKind` override, `edoal_cells` must hard-fail rather than
    // guess from the source (the historical bug).
    let ex_target = "http://example.org/unrelatedTarget";
    let (cell, onto_ds) = templated_cell(
        "startedAtTime",
        "DatatypeProperty",
        Vec::new(),
        vec![plain_atom("s", "http://example.org/somethingElse", "v")],
        ex_target,
    );
    let onto = DslView::new(&onto_ds);
    let b = &cell.bindings[0];
    assert_eq!(template_target_kind(&onto, b, &cell.pattern), None);
    let err = emit_kind_nt(&onto, &cell)
        .expect_err("an indeterminate template target with no override must hard-fail");
    assert!(err.message().contains("indeterminate"), "{err}");
    assert!(err.message().contains("template"), "{err}");
}
#[test]
fn mapping_cell_identity_retains_full_owner_and_each_same_profile_binding() {
    let dataset = ds(r#"
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
<https://a.example/name> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s"];
 gm:hasBinding [gm:profile "test"; gm:toClass <urn:A>; gm:relation "="],
               [gm:profile "test"; gm:toClass <urn:B>; gm:relation "<="].
<https://b.example/name> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s"];
 gm:hasBinding [gm:profile "test"; gm:toClass <urn:A>; gm:relation "="].
"#);
    let cells = projections(&DslView::new(&dataset)).unwrap();
    let mut output = Nt::new();
    let mut ids = std::collections::BTreeSet::new();
    for cell in &cells {
        for binding in &cell.bindings {
            let id = make_cell(
                &mut output,
                cell,
                binding,
                "_:left".into(),
                "_:right".into(),
                "synthetic cell",
                "0",
                "en",
            );
            assert!(
                ids.insert(id),
                "different full owners or binding semantics must not merge into one EDOAL Cell"
            );
        }
    }
    assert_eq!(ids.len(), 3);
}
