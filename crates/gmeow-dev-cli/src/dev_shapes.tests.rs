// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use gmeow_logic_compile::ir::ConstraintProvenance;

use super::*;

#[test]
fn authored_shapes_with_the_same_target_are_all_retained() {
    let ds = parse_dataset(
        br#"
            @prefix ex: <https://example.test/> .
            @prefix sh: <http://www.w3.org/ns/shacl#> .

            ex:First a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:name ; sh:minCount 1 ] .
            ex:Second a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:code ; sh:minCount 1 ] .
            "#,
        "text/turtle",
        None,
    )
    .expect("fixture parses");
    let mut errors = Vec::new();
    let shapes = read_shapes(&ds, &mut errors);
    assert!(errors.is_empty());
    assert_eq!(
        shapes.len(),
        2,
        "no authored shape may disappear by target-key collision"
    );
    assert_eq!(shapes[0].0, "https://example.test/First");
    assert_eq!(shapes[1].0, "https://example.test/Second");
}

#[test]
fn projected_shapes_with_the_same_target_are_rejected() {
    let dataset = parse_dataset(
        br#"
            @prefix ex: <https://example.test/> .
            @prefix sh: <http://www.w3.org/ns/shacl#> .

            ex:First a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:name ; sh:minCount 1 ] .
            ex:Second a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:code ; sh:minCount 1 ] .
            "#,
        "text/turtle",
        None,
    )
    .expect("fixture parses");
    let mut errors = Vec::new();
    let projected = shapes_by_target(&dataset, &mut errors);
    assert_eq!(projected.len(), 1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("share target"));
}

/// The record-anchored clearance: a finer legacy shape whose typed failure class differs
/// from the (single) class-annotation failure of the aggregate projected peer is grounded
/// when an EXACT `logic:formalizes <legacy-shape-IRI>` record carries the legacy class and
/// the projected peer subsumes the covered fragment.
#[test]
fn record_anchored_failure_identity_grounds_a_finer_legacy_shape() {
    use gmeow_logic_compile::ir::ShaclNodeKind;
    let target = ShapeTarget::Class("https://example.test/Widget".to_owned());
    // Projected aggregate peer: nodeKind + minCount on the path, class-level failure class.
    let proj_ir = ValidationShapeIr::new(
        "https://example.test/Widget-shape",
        target.clone(),
        vec![
            PropertyConstraintIr::new(
                "https://example.test/law",
                Some(1),
                None,
                Some(ConstraintProvenance::OwlRestriction),
                vec![ConstraintComponent::NodeKindShacl(
                    ShaclNodeKind::BlankNodeOrIri,
                )],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
    .with_failure_class("https://example.test/CoarseFailure")
    .unwrap();
    // Legacy finer shape: only the nodeKind (no minCount), a FINER failure class.
    let legacy_ir = ValidationShapeIr::new(
        "https://example.test/LawShape",
        target.clone(),
        vec![
            PropertyConstraintIr::new(
                "https://example.test/law",
                None,
                None,
                None,
                vec![ConstraintComponent::NodeKindShacl(
                    ShaclNodeKind::BlankNodeOrIri,
                )],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
    .with_failure_class("https://example.test/FineFailure")
    .unwrap();
    let read = ShapeRead {
        ir: legacy_ir,
        unsupported: vec![],
        extra_targets: vec![],
    };
    let mut projected = BTreeMap::new();
    projected.insert(
        target.clone(),
        (
            "https://example.test/Widget-shape".to_owned(),
            ShapeRead {
                ir: proj_ir,
                unsupported: vec![],
                extra_targets: vec![],
            },
        ),
    );
    let mut ctx = OracleCtx {
        projected,
        formalized_shapes: std::collections::BTreeSet::new(),
        formalized_failure_classes: BTreeMap::new(),
        object_properties: std::collections::BTreeSet::new(),
        object_ranges: BTreeMap::new(),
        constraint_surfaces: Vec::new(),
    };
    let ds = parse_dataset(b"", "text/turtle", None).expect("empty dataset parses");
    // WITHOUT the record: the differing failure class blocks deletion.
    let v = ctx.verdict("https://example.test/LawShape", &target, &read, &ds);
    assert!(
        matches!(v, Verdict::NotEquiv(ref r) if r.contains("failure class")),
        "{}",
        v.label()
    );
    // WITH the exact formalizes record carrying the legacy class: grounded.
    ctx.formalized_shapes
        .insert("https://example.test/LawShape".to_owned());
    ctx.formalized_failure_classes.insert(
        "https://example.test/LawShape".to_owned(),
        std::iter::once("https://example.test/FineFailure".to_owned()).collect(),
    );
    let v = ctx.verdict("https://example.test/LawShape", &target, &read, &ds);
    assert!(matches!(v, Verdict::EquivGroundedResidue), "{}", v.label());
    // A record carrying the WRONG failure class never clears the block.
    ctx.formalized_failure_classes.insert(
        "https://example.test/LawShape".to_owned(),
        std::iter::once("https://example.test/OtherFailure".to_owned()).collect(),
    );
    let v = ctx.verdict("https://example.test/LawShape", &target, &read, &ds);
    assert!(
        matches!(v, Verdict::NotEquiv(_)),
        "a wrong-class record must not clear deletion: {}",
        v.label()
    );
}

// ── Functional-max credit no longer rescues a dropped class-surface cap ──────
//
// A projected CLASS shape that has DROPPED a `sh:maxCount` facet must not clear as equivalent
// just because a SEPARATE property-scoped `sh:targetSubjectsOf` functional shape still carries
// the cap: the projected class surface itself must carry every cap it is credited with. These
// drive the tightened `verdict` directly. `WIDGET`/`WP` are this block's own class/path.

const WIDGET: &str = "https://example.test/Widget";
const WP: &str = "https://example.test/wp";
const WQ: &str = "https://example.test/wq";

/// A legacy class-target read carrying the given per-path `(min, max)` bounds on `WIDGET`.
fn widget_read(bounds: &[(&str, Option<u32>, Option<u32>)]) -> ShapeRead {
    let props = bounds
        .iter()
        .map(|(path, min, max)| {
            PropertyConstraintIr::new(
                (*path).to_owned(),
                *min,
                *max,
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )
            .expect("legacy property builds")
        })
        .collect();
    let ir = ValidationShapeIr::new(
        "https://example.test/WidgetLegacy".to_owned(),
        ShapeTarget::Class(WIDGET.to_owned()),
        props,
        None,
    )
    .expect("legacy shape builds");
    ShapeRead {
        ir,
        unsupported: vec![],
        extra_targets: vec![],
    }
}

/// A property-scoped functional shape (`sh:targetSubjectsOf WP` with `sh:maxCount 1`), the
/// SEPARATE surface whose bound must NOT rescue a dropped class-shape cap.
const WP_FUNCTIONAL: &str = concat!(
    "<https://example.test/wp-functional> a sh:NodeShape ;\n",
    "    sh:targetSubjectsOf <https://example.test/wp> ;\n",
    "    sh:property [ sh:path <https://example.test/wp> ; sh:maxCount 1 ] .\n"
);

#[test]
fn dropped_class_cap_is_not_rescued_by_functional_subjectsof_shape() {
    // Projected class shape keeps min-1 on WP but DROPPED the max; the functional shape
    // still carries max-1. The legacy class shape caps WP at 1. This is the R3 regression.
    let projected = format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://example.test/Widget-shape> a sh:NodeShape ;\n\
                 sh:targetClass <https://example.test/Widget> ;\n\
                 sh:property [ sh:path <https://example.test/wp> ; sh:minCount 1 ] .\n\
             {WP_FUNCTIONAL}"
    );
    let ctx = ctx_from(&projected, &[]);
    let target = ShapeTarget::Class(WIDGET.to_owned());
    let read = widget_read(&[(WP, Some(1), Some(1))]);
    let ds = parse_ttl("@prefix sh: <http://www.w3.org/ns/shacl#> .\n");
    let v = ctx.verdict("https://example.test/WidgetLegacy", &target, &read, &ds);
    assert!(
        matches!(v, Verdict::NotEquiv(_)),
        "a class shape that dropped the cap must be NOT-EQUIV: {}",
        v.label()
    );
}

#[test]
fn class_shape_that_carries_the_cap_still_clears_equiv() {
    // The faithful projection: the class shape itself carries min-1 AND max-1 on WP. The
    // functional shape is present too, but the class cap stands on its own — still EQUIV.
    let projected = format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://example.test/Widget-shape> a sh:NodeShape ;\n\
                 sh:targetClass <https://example.test/Widget> ;\n\
                 sh:property [ sh:path <https://example.test/wp> ; sh:minCount 1 ; sh:maxCount 1 ] .\n\
             {WP_FUNCTIONAL}"
    );
    let ctx = ctx_from(&projected, &[]);
    let target = ShapeTarget::Class(WIDGET.to_owned());
    let read = widget_read(&[(WP, Some(1), Some(1))]);
    let ds = parse_ttl("@prefix sh: <http://www.w3.org/ns/shacl#> .\n");
    let v = ctx.verdict("https://example.test/WidgetLegacy", &target, &read, &ds);
    assert!(
        matches!(v, Verdict::Equiv),
        "a class shape that carries the cap must still clear EQUIV: {}",
        v.label()
    );
}

#[test]
fn functional_capped_path_omitted_by_class_shape_is_not_equiv() {
    // The projected class shape carries WQ but OMITS WP entirely; only the functional shape
    // caps WP. The legacy shape requires WQ and caps WP at 1. The omitted cap is a loss on the
    // class surface — the functional bound is no longer injected to paper over it.
    let projected = format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://example.test/Widget-shape> a sh:NodeShape ;\n\
                 sh:targetClass <https://example.test/Widget> ;\n\
                 sh:property [ sh:path <https://example.test/wq> ; sh:minCount 1 ] .\n\
             {WP_FUNCTIONAL}"
    );
    let ctx = ctx_from(&projected, &[]);
    let target = ShapeTarget::Class(WIDGET.to_owned());
    let read = widget_read(&[(WQ, Some(1), None), (WP, None, Some(1))]);
    let ds = parse_ttl("@prefix sh: <http://www.w3.org/ns/shacl#> .\n");
    let v = ctx.verdict("https://example.test/WidgetLegacy", &target, &read, &ds);
    assert!(
        matches!(v, Verdict::NotEquiv(_)),
        "a functional-capped path the class shape omits must be NOT-EQUIV: {}",
        v.label()
    );
}

#[test]
fn functional_only_class_shape_with_no_peer_is_not_grounded() {
    // No projected class shape for WIDGET at all — only the functional `sh:targetSubjectsOf`
    // shape. A pure max-only legacy class shape is NO LONGER synthesized into an equivalent
    // from that functional coverage; with no declarative peer it is `NO-PROJECTED-PEER` and
    // does not clear the gate.
    let projected = format!("@prefix sh: <http://www.w3.org/ns/shacl#> .\n{WP_FUNCTIONAL}");
    let ctx = ctx_from(&projected, &[]);
    let target = ShapeTarget::Class(WIDGET.to_owned());
    let read = widget_read(&[(WP, None, Some(1))]);
    let ds = parse_ttl("@prefix sh: <http://www.w3.org/ns/shacl#> .\n");
    let v = ctx.verdict("https://example.test/WidgetLegacy", &target, &read, &ds);
    assert!(
        !v.is_grounded(),
        "a functional-only class shape with no declarative peer must not clear: {}",
        v.label()
    );
    assert!(
        matches!(v, Verdict::NoProjectedPeer),
        "the honest verdict is NO-PROJECTED-PEER: {}",
        v.label()
    );
}

const K: &str = "https://example.test/Constant";
const P: &str = "https://example.test/isExact";

fn empty_data() -> InstanceData {
    InstanceData {
        types: BTreeMap::new(),
        edges: BTreeMap::new(),
        literal_edges: std::collections::BTreeSet::new(),
    }
}

fn min_plus_hasvalue_shape() -> ValidationShapeIr {
    let pc = PropertyConstraintIr::new(
        P,
        Some(1),
        None,
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::HasValue(ShapeValue::Literal(
            purrdf::RdfLiteral::typed("true", "http://www.w3.org/2001/XMLSchema#boolean"),
        ))],
    )
    .expect("property constraint builds");
    ValidationShapeIr::new(
        "https://example.test/ConstantShape".to_owned(),
        ShapeTarget::Class(K.to_owned()),
        vec![pc],
        None,
    )
    .expect("shape IR builds")
}

#[test]
fn datatype_valued_existence_never_fabricates_owl_thing() {
    // A shape on a declared owl:DatatypeProperty (i.e. NOT in the object-property set) with
    // only minCount + a literal sh:hasValue: `owl:allValuesFrom owl:Thing` derives
    // `sh:nodeKind sh:BlankNodeOrIRI`, which every literal value violates — the existence
    // must be residue, with no fabricated axiom and no inert closure entry.
    let empty = std::collections::BTreeSet::new();
    let emit = reasoner_safe_emit(K, &min_plus_hasvalue_shape(), &empty, &empty, &empty_data());
    assert!(
        emit.class_stmts.iter().all(|s| !s.contains(OWL_THING)),
        "fabricated owl:Thing axiom on a datatype-valued path: {:?}",
        emit.class_stmts
    );
    assert!(
        emit.class_stmts.iter().all(|s| !s.contains("ClosureEntry")),
        "closure entry without an allValuesFrom carrier is inert: {:?}",
        emit.class_stmts
    );
    assert!(
        emit.residue.iter().any(|r| r.contains("sh:minCount")),
        "the un-projectable existence must be named as residue: {:?}",
        emit.residue
    );
}

/// The workspace root (this crate's manifest sits two levels below it).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn parse_ttl(ttl: &str) -> std::sync::Arc<RdfDataset> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("test turtle must parse")
}

/// Build the oracle context through the SAME construction path the CLI uses
/// ([`OracleCtx::from_surfaces`]): a projected validation surface plus the
/// constraint/procedural record surfaces, all as parsed Turtle.
fn ctx_from(projected_ttl: &str, surfaces: &[&str]) -> OracleCtx {
    let projected = parse_ttl(projected_ttl);
    let surfaces = surfaces.iter().map(|s| parse_ttl(s)).collect();
    OracleCtx::from_surfaces(
        &projected,
        surfaces,
        std::collections::BTreeSet::new(),
        BTreeMap::new(),
    )
    .expect("test surfaces must index cleanly")
}

// ── Scanner ────────────────────────────────────────────────────────────────

#[test]
fn scanner_discovers_root_shape_files_and_skips_generated() {
    // The real repo: the root shapes/gmeow-shapes.ttl is now fully drained (every check is a
    // canonical logic: constraint or a derived OWL/RDFS axiom), so it declares NO sh:NodeShape
    // and the scanner correctly SKIPS it — a shape-free file is not a legacy-shape obligation.
    let mut files = Vec::new();
    collect_legacy_shape_files(&repo_root().join("shapes"), &mut files);
    assert!(
        !files.iter().any(|p| p.ends_with("gmeow-shapes.ttl")),
        "the drained root shapes file carries no sh:NodeShape and must not be scanned: {files:?}"
    );

    // A synthetic tree: an authored declarer is found; a generated/ declarer and a
    // non-declaring .ttl are not.
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let tmp = tmp_dir.path();
    let declarer = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/S> a sh:NodeShape ; sh:targetClass <https://ex/C> .\n";
    std::fs::create_dir_all(tmp.join("generated")).expect("mkdir");
    std::fs::write(tmp.join("a.ttl"), declarer).expect("write");
    std::fs::write(tmp.join("generated/b.ttl"), declarer).expect("write");
    std::fs::write(
        tmp.join("c.ttl"),
        "# NodeShape mentioned in a comment only\n<https://ex/x> <https://ex/p> <https://ex/y> .\n",
    )
    .expect("write");
    let mut found = Vec::new();
    collect_legacy_shape_files(tmp, &mut found);
    assert_eq!(found, vec![tmp.join("a.ttl")], "{found:?}");
}

#[test]
fn scanner_universe_over_slices_is_unchanged() {
    // The generalized rule must reproduce the previous name-based universe over slices/ up
    // to the files that CONTRIBUTE shapes: every per-slice shapes.ttl that declares at least
    // one sh:NodeShape, and NOTHING else (module.ttl files mention NodeShape only in prose,
    // never as a declaration; a fully-migrated tombstone shapes.ttl declares none and
    // contributed ZERO shapes to the old scan's output too).
    let slices = repo_root().join("slices");
    let mut new_scan = Vec::new();
    collect_legacy_shape_files(&slices, &mut new_scan);
    fn old_rule(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                old_rule(&path, out);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("shapes.ttl") {
                out.push(path);
            }
        }
    }
    let mut named = Vec::new();
    old_rule(&slices, &mut named);
    assert!(!new_scan.is_empty());
    // Every scanned file is a per-slice shapes.ttl (nothing NEW joined the universe) …
    for f in &new_scan {
        assert!(
            named.contains(f),
            "a non-shapes.ttl slice file joined the universe: {}",
            f.display()
        );
    }
    // … and the only named files missing from the scan are declaration-free tombstones.
    for f in &named {
        if !new_scan.contains(f) {
            assert!(
                !declares_node_shape(f),
                "{} declares sh:NodeShape but was not scanned",
                f.display()
            );
        }
    }
}

#[test]
fn only_a_registered_fail_witness_is_exempt_from_the_counter_example_scan() {
    // Two hand-authored node shapes in the SAME `counter-examples/` directory. One is
    // registered as a `gmeow:saFailWitness`, one is not. The registration is the whole
    // difference: a witness that proves the ban has teeth must not be punished by the
    // scan, and a shape nobody registered must not hide behind that fact.
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let tmp = tmp_dir.path();
    let ce = tmp.join("tests/counter-examples");
    std::fs::create_dir_all(&ce).expect("mkdir");
    std::fs::write(
        tmp.join("manifest.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <https://ex/slice> a gmeow:Slice .\n",
    )
    .expect("write manifest");
    std::fs::write(
        tmp.join("tests/structural.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <https://ex/saNoShapes> a gmeow:StructuralAssertion ;\n\
             \x20\x20gmeow:saFailWitness \"tests/counter-examples/registered.ttl\" .\n",
    )
    .expect("write structural");
    let declarer = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/S> a sh:NodeShape ; sh:targetClass <https://ex/C> .\n";
    std::fs::write(ce.join("registered.ttl"), declarer).expect("write");
    std::fs::write(ce.join("smuggled.ttl"), declarer).expect("write");

    let mut found = Vec::new();
    collect_legacy_shape_files(tmp, &mut found);

    assert!(
        found.contains(&ce.join("smuggled.ttl")),
        "an UNREGISTERED hand-authored shape in counter-examples/ must be scanned — the \
             blanket directory exclusion is exactly the hole this closes: {found:?}"
    );
    assert!(
        !found.contains(&ce.join("registered.ttl")),
        "a REGISTERED gmeow:saFailWitness must stay exempt, or keeping the scan clean \
             would require leaving the ban unwitnessed: {found:?}"
    );
}

#[test]
fn a_conforming_example_conformance_fixture_confers_no_exemption() {
    // `gmeow:expectedOutcome gmeow:conforms` is not a fail-witness: the fixture is
    // supposed to VALIDATE, so it has no reason to hand-author a shape and registering
    // it must not buy one an exemption.
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let tmp = tmp_dir.path();
    let ce = tmp.join("tests/counter-examples");
    std::fs::create_dir_all(&ce).expect("mkdir");
    std::fs::write(
        tmp.join("manifest.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <https://ex/slice> a gmeow:Slice .\n",
    )
    .expect("write manifest");
    std::fs::write(
        tmp.join("tests/example-conformance.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <https://ex/ecOk> a gmeow:ExampleConformance ;\n\
             \x20\x20gmeow:exampleFile \"tests/counter-examples/conforming.ttl\" ;\n\
             \x20\x20gmeow:expectedOutcome gmeow:conforms .\n\
             <https://ex/ecBad> a gmeow:ExampleConformance ;\n\
             \x20\x20gmeow:exampleFile \"tests/counter-examples/violating.ttl\" ;\n\
             \x20\x20gmeow:expectedOutcome gmeow:violates .\n",
    )
    .expect("write cells");
    let declarer = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/S> a sh:NodeShape ; sh:targetClass <https://ex/C> .\n";
    std::fs::write(ce.join("conforming.ttl"), declarer).expect("write");
    std::fs::write(ce.join("violating.ttl"), declarer).expect("write");

    let mut found = Vec::new();
    collect_legacy_shape_files(tmp, &mut found);

    assert!(
        found.contains(&ce.join("conforming.ttl")),
        "a cell expecting gmeow:conforms is not a fail-witness and confers no \
             exemption: {found:?}"
    );
    assert!(
        !found.contains(&ce.join("violating.ttl")),
        "a cell expecting gmeow:violates IS a fail-witness: {found:?}"
    );
}

#[test]
fn the_shipped_hand_authored_shape_witness_is_registered_and_exempt() {
    // The one real fixture the exemption exists for, proved against the shipped tree
    // rather than a synthetic one: a rename or a dropped `gmeow:saFailWitness` line
    // must make this fail rather than quietly re-including the witness.
    let slice = repo_root().join("slices/core/work-orchestration");
    let witness = slice.join("tests/counter-examples/hand-authored-shape.ttl");
    assert!(
        declares_node_shape(&witness),
        "the witness must actually hand-author a shape, or it proves nothing"
    );
    assert!(
        registered_fail_witnesses(&slice).contains(&witness),
        "the shipped hand-authored-shape witness must be REGISTERED, not exempted by \
             the directory it sits in"
    );
    let mut found = Vec::new();
    collect_legacy_shape_files(&slice, &mut found);
    assert!(!found.contains(&witness), "{found:?}");
}

// ── Semantic clearance matrix: sh:xone beside a projected declarative peer ──

const XONE_LEGACY_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        <https://ex/ParamShape> a sh:NodeShape ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:property [ sh:path <https://ex/name> ; sh:minCount 1 ; sh:maxCount 1 ] ;\n\
        \x20\x20sh:xone (\n\
        \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/value> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:Literal ] ]\n\
        \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/entity> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:IRI ] ]\n\
        \x20\x20) .\n";

/// The projected declarative peer reproducing the covered fragment.
const XONE_PEER_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        <https://ex/Param-shape> a sh:NodeShape ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:property [ sh:path <https://ex/name> ; sh:minCount 1 ; sh:maxCount 1 ] .\n";

/// The faithful record: flags a focus with NEITHER alternative and a focus with BOTH.
const XONE_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/ParamXoneConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/ParamShape> ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:sparql [\n\
        \x20\x20\x20\x20a sh:SPARQLConstraint ;\n\
        \x20\x20\x20\x20sh:message \"exactly one of value/entity\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE {\n\
            { FILTER NOT EXISTS { $this <https://ex/value> ?v } FILTER NOT EXISTS { $this <https://ex/entity> ?e } }\n\
            UNION\n\
            { $this <https://ex/value> ?v2 . $this <https://ex/entity> ?e2 . }\n\
        }\"\"\" ;\n\
        \x20\x20] .\n";

/// The WRONG-SEMANTICS record: an at-least-one lowering of the exactly-one obligation.
const XONE_OR_LOWERED_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/ParamXoneConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/ParamShape> ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:sparql [\n\
        \x20\x20\x20\x20a sh:SPARQLConstraint ;\n\
        \x20\x20\x20\x20sh:message \"at least one of value/entity\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE {\n\
            FILTER NOT EXISTS { $this <https://ex/value> ?v }\n\
            FILTER NOT EXISTS { $this <https://ex/entity> ?e }\n\
        }\"\"\" ;\n\
        \x20\x20] .\n";

#[test]
fn xone_clearance_matrix() {
    let ds = parse_ttl(XONE_LEGACY_TTL);
    let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("legacy reads");
    assert!(
        read.unsupported.iter().any(|u| u == SH_XONE),
        "{:?}",
        read.unsupported
    );

    // Correct grounding: record + failure identity + witness agreement → cleared.
    let ctx = ctx_from(XONE_PEER_TTL, &[XONE_RECORD_TTL]);
    let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivGroundedResidueSemantic),
        "the faithful record must clear: {}",
        v.label()
    );

    // Missing record → the residue stays ungrounded.
    let ctx = ctx_from(XONE_PEER_TTL, &[]);
    let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivResidue(_)),
        "no record must not clear: {}",
        v.label()
    );

    // Wrong-semantics record (an or-lowering): the witness cross-check MUST deny clearance.
    let ctx = ctx_from(XONE_PEER_TTL, &[XONE_OR_LOWERED_RECORD_TTL]);
    let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivResidue(_)),
        "an or-lowered record must NOT clear: {}",
        v.label()
    );
}

#[test]
fn xone_clearance_rejects_a_mismatched_failure_class() {
    // The same fixture pair with a typed failure class on the legacy shape and its peer;
    // the record carries a DIFFERENT class, so the failure identity blocks clearance.
    let failure = "gmeow:enforcesFailureClass";
    let legacy = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
        XONE_LEGACY_TTL.replace(
            "sh:targetClass <https://ex/Param> ;",
            &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/F> ;")
        )
    );
    let peer = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
        XONE_PEER_TTL.replace(
            "sh:targetClass <https://ex/Param> ;",
            &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/F> ;")
        )
    );
    let wrong_class_record = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
        XONE_RECORD_TTL.replace(
            "sh:targetClass <https://ex/Param> ;",
            &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/G> ;")
        )
    );
    let right_class_record = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
        XONE_RECORD_TTL.replace(
            "sh:targetClass <https://ex/Param> ;",
            &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/F> ;")
        )
    );
    let ds = parse_ttl(&legacy);
    let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("legacy reads");

    let ctx = ctx_from(&peer, &[&right_class_record]);
    let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivGroundedResidueSemantic),
        "the matching failure class clears: {}",
        v.label()
    );

    let ctx = ctx_from(&peer, &[&wrong_class_record]);
    let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
    assert!(
        !v.is_grounded(),
        "a mismatched failure class must NOT clear: {}",
        v.label()
    );
}

// ── Semantic clearance matrix: a raw-SPARQL-target block (meta-shape style) ──

const META_LEGACY_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
        <https://ex/MetaShape> a sh:NodeShape ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] ;\n\
        \x20\x20sh:property [ sh:path <https://ex/role> ; sh:minCount 1 ; sh:nodeKind sh:IRI ] .\n";

/// The faithful record: the SAME focus selection plus the SAME structural constraints,
/// carried on the projected constraint surface with the exact formalizes back-reference.
const META_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/MetaConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/MetaShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] ;\n\
        \x20\x20sh:property [ sh:path <https://ex/role> ; sh:minCount 1 ; sh:nodeKind sh:IRI ] .\n";

/// The WRONG-SEMANTICS record: it silently drops the role obligation.
const META_WEAK_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/MetaConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/MetaShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] .\n";

#[test]
fn sparql_target_clearance_matrix() {
    let ds = parse_ttl(META_LEGACY_TTL);
    let read = read_shacl_shape(&ds, "https://ex/MetaShape").expect("legacy reads");
    assert!(
        matches!(read.ir.target, ShapeTarget::Sparql(_)),
        "{:?}",
        read.ir.target
    );
    assert!(
        read.unsupported
            .iter()
            .any(|u| u == RAW_SPARQL_TARGET_RESIDUE),
        "{:?}",
        read.unsupported
    );

    // Correct grounding: the record reproduces the structural constraints on the same focus
    // selection → cleared (the covered witnesses are part of the plan: no declarative peer
    // exists for a raw SPARQL target).
    let ctx = ctx_from("", &[META_RECORD_TTL]);
    let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivGroundedResidueSemantic),
        "the faithful record must clear: {}",
        v.label()
    );

    // Missing record → whole-shape residue, not cleared.
    let ctx = ctx_from("", &[]);
    let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivResidue(_)),
        "no record must not clear: {}",
        v.label()
    );

    // Wrong-semantics record (drops an obligation) → the structural witness cross-check
    // MUST deny clearance.
    let ctx = ctx_from("", &[META_WEAK_RECORD_TTL]);
    let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivResidue(_)),
        "a record that drops an obligation must NOT clear: {}",
        v.label()
    );
}

// ── Raw-SPARQL-target identity clearance (join-shaped target skeleton) ─────

/// A legacy raw-target block whose focus selection is a variable-class JOIN (outside the
/// witness synthesizer's skeleton) and whose ONLY enforcement is one sh:sparql constraint
/// carrying an anonymous `[]` node.
const JOIN_LEGACY_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        <https://ex/OpenValueShape> a sh:NodeShape ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?profile <https://ex/openValue> ?openClass .\n\
                ?this a ?openClass .\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:severity sh:Warning ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:message \"m\" ; sh:select \"\"\"\n\
            SELECT $this WHERE {\n\
                ?profile <https://ex/openValue> ?openClass .\n\
                $this a ?openClass .\n\
                FILTER NOT EXISTS {\n\
                    ?profile <https://ex/descriptor> ?descriptor .\n\
                    [] ?descriptor $this .\n\
                }\n\
            }\n\
        \"\"\" ] .\n";

/// The faithful projected record: the SAME target select and constraint body up to
/// whitespace, variable names (`?p2`), and the `[]` node written as an explicit variable.
const JOIN_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/OpenValueConstraintShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/OpenValueShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE { ?p2 <https://ex/openValue> ?oc . ?this a ?oc . }\"\"\" ] ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ; sh:message \"m\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { ?p2 <https://ex/openValue> ?oc . $this a ?oc . FILTER NOT EXISTS { ?p2 <https://ex/descriptor> ?d . ?subj ?d $this . } }\"\"\" ] .\n";

/// A DIVERGENT record: same target, but the constraint body checks a different predicate.
const JOIN_WRONG_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/OpenValueConstraintShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/OpenValueShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE { ?p2 <https://ex/openValue> ?oc . ?this a ?oc . }\"\"\" ] ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ; sh:message \"m\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { ?p2 <https://ex/OTHER> ?oc . $this a ?oc . }\"\"\" ] .\n";

#[test]
fn raw_sparql_target_identity_clearance_matrix() {
    let ds = parse_ttl(JOIN_LEGACY_TTL);
    let read = read_shacl_shape(&ds, "https://ex/OpenValueShape").expect("legacy reads");
    assert!(
        matches!(read.ir.target, ShapeTarget::Sparql(_)),
        "{:?}",
        read.ir.target
    );
    assert!(read.ir.properties.is_empty(), "{:?}", read.ir.properties);

    // The α-equivalent record clears the block: identical selects (modulo variable
    // renaming, `[]`, whitespace, keyword case) are a verified reproduction.
    let ctx = ctx_from("", &[JOIN_RECORD_TTL]);
    let v = ctx.verdict_all("https://ex/OpenValueShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivGroundedResidue),
        "the α-equivalent record must clear: {}",
        v.label()
    );

    // No record → whole-shape residue.
    let ctx = ctx_from("", &[]);
    let v = ctx.verdict_all("https://ex/OpenValueShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivResidue(_)),
        "no record must not clear: {}",
        v.label()
    );

    // A record whose constraint body diverges must NOT clear (and the witness path cannot
    // synthesize a plan for the join-shaped target, so the block stays residue).
    let ctx = ctx_from("", &[JOIN_WRONG_RECORD_TTL]);
    let v = ctx.verdict_all("https://ex/OpenValueShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivResidue(_)),
        "a divergent record must NOT clear: {}",
        v.label()
    );
}

#[test]
fn sparql_alpha_canonical_equates_renamings_and_distinguishes_predicates() {
    let a = "SELECT $this WHERE { ?x <https://ex/p> ?y . [] ?y $this . }";
    let b = "SELECT ?this WHERE {\n  ?profile <https://ex/p> ?d .\n  ?subj ?d ?this . }";
    assert_eq!(sparql_alpha_canonical(a), sparql_alpha_canonical(b));
    let c = "SELECT $this WHERE { ?x <https://ex/OTHER> ?y . [] ?y $this . }";
    assert_ne!(sparql_alpha_canonical(a), sparql_alpha_canonical(c));
    // Two `[]` occurrences are DISTINCT fresh variables, never unified.
    let d = "SELECT ?this WHERE { [] <https://ex/p> ?this . [] <https://ex/q> ?this . }";
    let e = "SELECT ?this WHERE { ?s <https://ex/p> ?this . ?s <https://ex/q> ?this . }";
    assert_ne!(sparql_alpha_canonical(d), sparql_alpha_canonical(e));
}

#[test]
fn sparql_target_clearance_rejects_a_mismatched_failure_class() {
    let legacy = META_LEGACY_TTL.replace(
        "<https://ex/MetaShape> a sh:NodeShape ;",
        "<https://ex/MetaShape> a sh:NodeShape ;\n  \
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> <https://ex/F> ;",
    );
    let wrong_record = META_RECORD_TTL.replace(
        "<https://ex/MetaConstraint> a sh:NodeShape ;",
        "<https://ex/MetaConstraint> a sh:NodeShape ;\n  \
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> <https://ex/G> ;",
    );
    let right_record = META_RECORD_TTL.replace(
        "<https://ex/MetaConstraint> a sh:NodeShape ;",
        "<https://ex/MetaConstraint> a sh:NodeShape ;\n  \
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> <https://ex/F> ;",
    );
    let ds = parse_ttl(&legacy);
    let read = read_shacl_shape(&ds, "https://ex/MetaShape").expect("legacy reads");

    let ctx = ctx_from("", &[&right_record]);
    let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivGroundedResidueSemantic),
        "the matching failure class clears: {}",
        v.label()
    );

    let ctx = ctx_from("", &[&wrong_record]);
    let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
    assert!(
        !v.is_grounded(),
        "a mismatched failure class must NOT clear: {}",
        v.label()
    );
}

// ── Targetless documentation marker ────────────────────────────────────────

#[test]
fn targetless_doc_marker_reads_and_verdicts_equiv() {
    let ttl = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/DocMarker> a sh:NodeShape ;\n\
             \x20\x20rdfs:label \"doc-only marker\" ;\n\
             \x20\x20rdfs:comment \"asserts and enforces nothing\" .\n";
    let ds = parse_ttl(ttl);
    let read = read_shacl_shape(&ds, "https://ex/DocMarker").expect("targetless doc reads");
    let ctx = ctx_from("", &[]);
    let v = ctx.verdict_all("https://ex/DocMarker", &read, &ds);
    assert!(
        matches!(v, Verdict::Equiv),
        "a no-op doc marker enforces nothing and is trivially grounded: {}",
        v.label()
    );
}

#[test]
fn enforcement_free_class_block_clears_only_via_its_exact_record() {
    // A class-target block with ZERO enforcement components: deleting it loses nothing,
    // but its identity clears only once its intended obligation rides an exact
    // `logic:formalizes` record on the projected constraint surface.
    let ttl = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/NoOpShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20rdfs:comment \"documentation only\" .\n";
    let record = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/CConstraint> a sh:NodeShape ;\n\
             \x20\x20<https://blackcatinformatics.ca/logic/formalizes> <https://ex/NoOpShape> .\n";
    let ds = parse_ttl(ttl);
    let read = read_shacl_shape(&ds, "https://ex/NoOpShape").expect("no-op class block reads");

    // Without a record the honest verdict stays NO-PROJECTED-PEER.
    let ctx = ctx_from("", &[]);
    let v = ctx.verdict_all("https://ex/NoOpShape", &read, &ds);
    assert!(
        matches!(v, Verdict::NoProjectedPeer),
        "no record must not clear an enforcement-free class block: {}",
        v.label()
    );

    // The exact record grounds the block's identity.
    let ctx = ctx_from("", &[record]);
    let v = ctx.verdict_all("https://ex/NoOpShape", &read, &ds);
    assert!(
        matches!(v, Verdict::EquivGroundedResidue),
        "the exact formalizes record must clear: {}",
        v.label()
    );
}

// ── Prune-splicer proof against the REAL shapes/gmeow-shapes.ttl ──────────

/// The real repo-wide shapes file plus its parsed `sh:NodeShape` IRIs.
fn real_gmeow_shapes() -> (String, Vec<String>) {
    let path = repo_root().join("shapes/gmeow-shapes.ttl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let ds = parse_dataset(text.as_bytes(), "text/turtle", None)
        .expect("the committed shapes file must parse");
    let iris = node_shape_iris(&ds);
    (text, iris)
}

#[test]
fn splicer_resolves_every_real_block_subject_count_exact() {
    // Count-agnostic: the wave-C drain reduced the committed file's census, and a future wave
    // will empty it entirely, so the proof pins the splicer MECHANISM (each block resolves to
    // its OWN non-overlapping span) against whatever blocks the file currently carries.
    let (text, iris) = real_gmeow_shapes();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for iri in &iris {
        let span = subject_span(&text, local_name(iri))
            .unwrap_or_else(|| panic!("subject_span failed for {iri}"));
        spans.push(span);
    }
    spans.sort_unstable();
    spans.dedup();
    assert_eq!(
        spans.len(),
        iris.len(),
        "every block resolves to its OWN span"
    );
    for w in spans.windows(2) {
        assert!(
            w[0].1 <= w[1].0,
            "block spans must never overlap: {:?} vs {:?}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn splicing_every_real_block_leaves_valid_turtle_with_zero_shapes() {
    let (text, iris) = real_gmeow_shapes();
    let mut pruned = text.clone();
    let spans: Vec<(usize, usize)> = iris
        .iter()
        .map(|iri| subject_span(&pruned, local_name(iri)).expect("span resolves"))
        .collect();
    splice_out_spans(&mut pruned, spans);
    let ds = parse_dataset(pruned.as_bytes(), "text/turtle", None)
        .expect("the fully-pruned file must stay valid Turtle");
    assert!(
        node_shape_iris(&ds).is_empty(),
        "every block must be gone after the full prune"
    );
}

#[test]
fn splicing_each_real_block_individually_round_trips() {
    let (text, iris) = real_gmeow_shapes();
    for iri in &iris {
        let mut copy = text.clone();
        let span = subject_span(&copy, local_name(iri)).expect("span resolves");
        splice_out_spans(&mut copy, vec![span]);
        let ds = parse_dataset(copy.as_bytes(), "text/turtle", None)
            .unwrap_or_else(|e| panic!("pruning {iri} broke the Turtle: {e}"));
        let remaining = node_shape_iris(&ds);
        assert_eq!(
            remaining.len(),
            iris.len() - 1,
            "pruning {iri} must remove exactly one block"
        );
        assert!(!remaining.contains(iri), "{iri} must be the removed block");
    }
}

// ── Block classification + subsumption-lattice pre-analysis ────────────────

/// Fixture blocks read through the real comparison-only reader.
fn lattice_blocks(ttl: &str) -> Vec<(String, ShapeRead)> {
    let ds = parse_ttl(ttl);
    let mut errors = Vec::new();
    let blocks = read_shapes(&ds, &mut errors);
    assert!(errors.is_empty(), "fixture blocks must read: {errors:?}");
    blocks
}

/// A hierarchy from literal direct edges.
fn lattice_hier(edges: &[(&str, &str)]) -> ClassHierarchy {
    let mut direct: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for (sub, sup) in edges {
        direct
            .entry((*sub).to_owned())
            .or_default()
            .insert((*sup).to_owned());
    }
    ClassHierarchy::from_direct(direct)
}

/// `ClassHierarchy::load` reads BOTH spellings of the subsumption predicate,
/// so a slice re-authored onto the canonical `logic:subClassOf` is not seen as
/// a FLAT hierarchy.
///
/// The blinding regression this pins: an `rdfs:`-only read returned no edges
/// at all for a converted slice, so `focus_overlaps` / `target_covers` found
/// no class-target overlap and the lattice pre-analysis reported a clean
/// lattice over a hierarchy it simply could not see.
#[test]
fn class_hierarchy_load_reads_both_subsumption_spellings() {
    let tmp_dir = tempfile::Builder::new()
        .prefix("gmeow-class-hierarchy-")
        .tempdir()
        .expect("create temp dir");
    let tmp = tmp_dir.path();
    let canonical = tmp.join("slices").join("canonical");
    let projected = tmp.join("slices").join("projected");
    std::fs::create_dir_all(&canonical).expect("mkdir");
    std::fs::create_dir_all(&projected).expect("mkdir");

    // A slice authored entirely in the CANONICAL spelling — no `rdfs:` edge.
    std::fs::write(
            canonical.join("module.ttl"),
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             <https://ex/Cat> <https://blackcatinformatics.ca/logic/subClassOf> <https://ex/Animal> .\n\
             <https://ex/Animal> logic:subClassOf <https://ex/Organism> .\n\
             <https://ex/Blank> logic:subClassOf [ <https://ex/p> <https://ex/v> ] .\n",
        )
        .expect("write");
    // A slice still authored in the projected spelling — must keep working.
    std::fs::write(
            projected.join("module.ttl"),
            "<https://ex/Rock> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://ex/Mineral> .\n",
        )
        .expect("write");

    let hier = ClassHierarchy::load(tmp);

    assert!(
        hier.class_covers("https://ex/Animal", "https://ex/Cat"),
        "a direct `logic:subClassOf` edge is a hierarchy edge"
    );
    assert!(
        hier.class_covers("https://ex/Organism", "https://ex/Cat"),
        "the canonical edges saturate transitively"
    );
    assert!(
        hier.class_covers("https://ex/Mineral", "https://ex/Rock"),
        "the projected spelling still loads"
    );
    assert!(
        !hier.direct.contains_key("https://ex/Blank"),
        "a blank-node class expression is never a hierarchy edge"
    );
}

#[test]
fn classification_partitions_by_block_with_all_tags() {
    let blocks = lattice_blocks(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/ClassBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/SubjectsBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetSubjectsOf <https://ex/p> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:maxCount 1 ] .\n\
             <https://ex/SparqlBlock> a sh:NodeShape ;\n\
             \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE { ?this <https://ex/q> ?v . }\"\"\" ] ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/MultiBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:targetSubjectsOf <https://ex/q> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/NoOpBlock> a sh:NodeShape ;\n\
             \x20\x20rdfs:label \"documentation-only marker\" .\n",
    );
    assert_eq!(blocks.len(), 5, "the census is by-block");
    let report = lattice_report(&blocks, &lattice_hier(&[]));
    assert!(report.contains("  block <https://ex/ClassBlock> tags=targetClass\n"));
    assert!(report.contains("  block <https://ex/SubjectsBlock> tags=targetSubjectsOf\n"));
    assert!(
        report.contains("  block <https://ex/SparqlBlock> tags=sparql-target\n"),
        "{report}"
    );
    assert!(
        report.contains("  block <https://ex/MultiBlock> tags=targetClass,targetSubjectsOf\n"),
        "a multi-target block lists ALL its tags: {report}"
    );
    assert!(
        report.contains("  block <https://ex/NoOpBlock> tags=no-target no-op\n"),
        "a zero-enforcement block carries the no-op flag: {report}"
    );
    assert!(report.contains("  tally targetClass=2\n"), "{report}");
    assert!(report.contains("  tally targetSubjectsOf=2\n"), "{report}");
    assert!(report.contains("  tally sparql-target=1\n"), "{report}");
    assert!(report.contains("  tally no-target=1\n"), "{report}");
    assert!(report.contains("  tally no-op=1\n"), "{report}");
    assert!(report.contains("  TOTAL blocks=5\n"), "{report}");
}

#[test]
fn contradiction_hasvalue_excluded_by_in_is_reported() {
    let blocks = lattice_blocks(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/HasBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:hasValue <https://ex/X> ] .\n\
             <https://ex/InBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:in ( <https://ex/Y> <https://ex/Z> ) ] .\n",
    );
    let report = lattice_report(&blocks, &lattice_hier(&[]));
    assert!(
        report.contains(
            "  <https://ex/HasBlock> ⊗ <https://ex/InBlock> — path <https://ex/p>: \
                 sh:hasValue <https://ex/X> excluded by sh:in (<https://ex/Y> <https://ex/Z>)"
        ),
        "{report}"
    );
}

#[test]
fn contradiction_min_over_max_across_subclass_related_targets() {
    // The overlap rides the hierarchy: Sub ⊑ Super, so a Sub instance is validated by both.
    let blocks = lattice_blocks(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/MinBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Sub> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/q> ; sh:minCount 2 ] .\n\
             <https://ex/MaxBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Super> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/q> ; sh:maxCount 1 ] .\n",
    );
    let hier = lattice_hier(&[("https://ex/Sub", "https://ex/Super")]);
    let report = lattice_report(&blocks, &hier);
    assert!(
        report.contains(
            "  <https://ex/MaxBlock> ⊗ <https://ex/MinBlock> — path <https://ex/q>: \
                 sh:minCount 2 vs sh:maxCount 1 (min > max across the pair)"
        ),
        "{report}"
    );
    // Without the hierarchy the focus sets never meet — no contradiction.
    let report = lattice_report(&blocks, &lattice_hier(&[]));
    assert!(report.contains("CONTRADICTIONS\n  none\n"), "{report}");
}

#[test]
fn contradiction_incompatible_datatypes_on_a_required_path() {
    let blocks = lattice_blocks(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             <https://ex/DecBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:datatype xsd:decimal ] .\n\
             <https://ex/StrBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:datatype xsd:string ] .\n",
    );
    let report = lattice_report(&blocks, &lattice_hier(&[]));
    assert!(
            report.contains("sh:datatype <http://www.w3.org/2001/XMLSchema#decimal> vs sh:datatype <http://www.w3.org/2001/XMLSchema#string> (no value can carry both)"),
            "{report}"
        );
}

#[test]
fn redundancy_superclass_block_subsumes_subclass_block() {
    let blocks = lattice_blocks(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/SubBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Sub> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/SuperBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Super> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:maxCount 1 ] .\n",
    );
    let hier = lattice_hier(&[("https://ex/Sub", "https://ex/Super")]);
    let report = lattice_report(&blocks, &hier);
    assert!(
        report.contains("  <https://ex/SubBlock> ⊑ <https://ex/SuperBlock>\n"),
        "the superclass block's stricter payload entails the subclass block: {report}"
    );
    assert!(
        !report.contains("  <https://ex/SuperBlock> ⊑ <https://ex/SubBlock>"),
        "the reverse never holds (the subclass block covers fewer focus nodes): {report}"
    );
    // Without the hierarchy the coverage leg fails — no redundancy.
    let report = lattice_report(&blocks, &lattice_hier(&[]));
    assert!(report.contains("REDUNDANCIES\n  none\n"), "{report}");
}

#[test]
fn hoist_identical_constraint_on_two_siblings_reports_the_lub() {
    let blocks = lattice_blocks(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/C1Block> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C1> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:maxCount 1 ] .\n\
             <https://ex/C2Block> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C2> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:maxCount 1 ] .\n",
    );
    let hier = lattice_hier(&[
        ("https://ex/C1", "https://ex/P0"),
        ("https://ex/C2", "https://ex/P0"),
    ]);
    let report = lattice_report(&blocks, &hier);
    assert!(
        report.contains(
            "  lub=<https://ex/P0> constraint=[sh:path <https://ex/p> ; sh:minCount 1 ; \
                 sh:maxCount 1] siblings=<https://ex/C1>,<https://ex/C2>\n"
        ),
        "{report}"
    );
    // Siblings under DIFFERENT parents never hoist.
    let hier = lattice_hier(&[
        ("https://ex/C1", "https://ex/P0"),
        ("https://ex/C2", "https://ex/P1"),
    ]);
    let report = lattice_report(&blocks, &hier);
    assert!(report.contains("HOISTS\n  none\n"), "{report}");
}

#[test]
fn classification_over_the_real_gmeow_shapes_matches_the_block_census() {
    // Count-agnostic: the wave-C drain reduced the committed census (and a future wave empties
    // it), so the classifier's reported TOTAL must simply agree with the blocks actually read.
    let (text, _) = real_gmeow_shapes();
    let ds = parse_dataset(text.as_bytes(), "text/turtle", None)
        .expect("the committed shapes file must parse");
    let mut errors = Vec::new();
    let blocks = read_shapes(&ds, &mut errors);
    assert!(errors.is_empty(), "every committed block reads: {errors:?}");
    let report = lattice_report(&blocks, &lattice_hier(&[]));
    assert!(
        report.contains(&format!("  TOTAL blocks={}\n", blocks.len())),
        "the by-block census must match the blocks read ({}): {}",
        blocks.len(),
        report.lines().rev().take(12).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn object_property_existence_keeps_the_owl_thing_carrier() {
    // The same bare existence on a declared owl:ObjectProperty stays sound: values are
    // IRI-named individuals, so the owl:Thing carrier (+ its closure entry) is emitted.
    let pc = PropertyConstraintIr::new(
        P,
        Some(1),
        None,
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .expect("property constraint builds");
    let ir = ValidationShapeIr::new(
        "https://example.test/ConstantShape".to_owned(),
        ShapeTarget::Class(K.to_owned()),
        vec![pc],
        None,
    )
    .expect("shape IR builds");
    let functional = std::collections::BTreeSet::new();
    let objects: std::collections::BTreeSet<String> = std::iter::once(P.to_owned()).collect();
    let emit = reasoner_safe_emit(K, &ir, &functional, &objects, &empty_data());
    assert!(
        emit.class_stmts
            .iter()
            .any(|s| s.contains(&format!("allValuesFrom <{OWL_THING}>"))),
        "object-property existence keeps its universal carrier: {:?}",
        emit.class_stmts
    );
    assert!(
        emit.class_stmts.iter().any(|s| s.contains("ClosureEntry")),
        "the carrier's closure entry projects the sh:minCount: {:?}",
        emit.class_stmts
    );
    assert!(emit.residue.is_empty(), "residue: {:?}", emit.residue);
}
