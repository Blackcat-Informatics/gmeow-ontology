// SPDX-License-Identifier: AGPL-3.0-only
//! Equivalence-before-deletion certificates for the five retired hand-authored SHACL shapes.
//!
//! Four came from `slices/core/gts/shapes.ttl` (`gmeow:OpaqueFrameShape`,
//! `gmeow:SealedFrameRecipientShape`, `gmeow:GTSSegmentShape`, `gmeow:EvidenceCompactionShape`)
//! and the file itself is gone; the fifth is `lang:GmnEnvelopeContractShape`, deleted from
//! `slices/grounding/lang/shapes.ttl`. Doctrine (Principle 17,
//! `docs/MIGRATING-SHAPES-TO-LOGIC.md`): a shape is retired only once its check is *provably*
//! reproduced by the projection of an authored `logic:`/OWL node. These tests are that proof,
//! run against the REAL derivation (`derive_validation_shapes` /
//! `project_procedural_constraints`) over the live slice modules — never a re-implementation.
//!
//! Each retired shape's obligations are transcribed here component-by-component from the deleted
//! Turtle, so dropping any single one of them from the ontology (a restriction, a closure
//! opt-in, a facet, a `logic:Constraint`) reds exactly the assertion that named it.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use gmeow_logic_compile::frontend::{derive_validation_shapes, parse_logic_str};
use gmeow_logic_compile::ir::{
    ConstraintComponent, PropertyConstraintIr, ShaclNodeKind, ShapeTarget, ValidationShapeIr,
};
use gmeow_logic_compile::projections::shapes::{
    project_procedural_constraints, project_validation_shape_shacl,
};
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LANG: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const GTS_MODULE: &str = "slices/core/gts/module.ttl";
const LANG_MODULE: &str = "slices/grounding/lang/module.ttl";

/// The `sh:` prefix header a projected node-shape block needs to parse standalone (the block
/// itself writes every other term as a full IRI).
const SH_HEADER: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn g(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn read_module(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The REAL declarative derivation over one authored slice module.
fn derived(rel: &str) -> Vec<ValidationShapeIr> {
    let src = read_module(rel);
    let ds = purrdf::parse_dataset(src.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("{rel} parses as RDF: {e}"));
    derive_validation_shapes(ds.as_ref()).unwrap_or_else(|e| panic!("derive over {rel}: {e}"))
}

/// The derived `Class(C)` node shape, or a panic naming every target that WAS derived (so a
/// dropped class antecedent fails loudly rather than silently skipping the obligations).
fn class_shape<'a>(shapes: &'a [ValidationShapeIr], class: &str) -> &'a ValidationShapeIr {
    shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c == class))
        .unwrap_or_else(|| panic!("no derived class shape for {class}"))
}

/// Every derived property shape on `path` of `shape` (the derivation may keep more than one
/// after the same-path merge; an obligation is honoured when ANY of them carries it).
fn on_path<'a>(shape: &'a ValidationShapeIr, path: &str) -> Vec<&'a PropertyConstraintIr> {
    let hits: Vec<&PropertyConstraintIr> =
        shape.properties.iter().filter(|p| p.path == path).collect();
    assert!(
        !hits.is_empty(),
        "{} derives no property shape on {path}",
        shape.iri
    );
    hits
}

/// A component satisfying `pred` is present on `path` (searching the property shape's own
/// components; qualified inner shapes are matched by the qualified helpers below).
fn assert_component(
    shape: &ValidationShapeIr,
    path: &str,
    what: &str,
    pred: impl Fn(&ConstraintComponent) -> bool,
) {
    let found = on_path(shape, path)
        .iter()
        .any(|p| p.components.iter().any(&pred));
    assert!(
        found,
        "{} on {path} must carry {what}; derived: {:?}",
        shape.iri,
        on_path(shape, path)
    );
}

fn assert_class(shape: &ValidationShapeIr, path: &str, class: &str) {
    assert_component(
        shape,
        path,
        &format!("sh:class {class}"),
        |c| matches!(c, ConstraintComponent::Class(x) if x == class),
    );
}

fn assert_datatype(shape: &ValidationShapeIr, path: &str, dt: &str) {
    assert_component(
        shape,
        path,
        &format!("sh:datatype {dt}"),
        |c| matches!(c, ConstraintComponent::Datatype(x) if x == dt),
    );
}

fn assert_pattern(shape: &ValidationShapeIr, path: &str, regex: &str) {
    assert_component(
        shape,
        path,
        &format!("sh:pattern {regex}"),
        |c| matches!(c, ConstraintComponent::Pattern { regex: r, .. } if r == regex),
    );
}

fn assert_node_kind(shape: &ValidationShapeIr, path: &str, kind: ShaclNodeKind) {
    assert_component(
        shape,
        path,
        &format!("sh:nodeKind {}", kind.as_str()),
        |c| matches!(c, ConstraintComponent::NodeKindShacl(k) if *k == kind),
    );
}

/// A PLAIN `sh:minCount n` on `path`.
fn assert_min_count(shape: &ValidationShapeIr, path: &str, n: u32) {
    let found = on_path(shape, path)
        .iter()
        .any(|p| p.min_count.is_some_and(|m| m >= n));
    assert!(
        found,
        "{} on {path} must carry sh:minCount >= {n}; derived: {:?}",
        shape.iri,
        on_path(shape, path)
    );
}

/// A PLAIN `sh:maxCount n` on `path`.
fn assert_max_count(shape: &ValidationShapeIr, path: &str, n: u32) {
    let found = on_path(shape, path)
        .iter()
        .any(|p| p.max_count.is_some_and(|m| m <= n));
    assert!(
        found,
        "{} on {path} must carry sh:maxCount <= {n}; derived: {:?}",
        shape.iri,
        on_path(shape, path)
    );
}

/// The class-scoped count obligation the retired hand-authored shape stated, in EITHER
/// derived form the projector can legitimately produce for it.
///
/// A `sh:qualifiedValueShape [ sh:class class ]` with qualified counts is one. The other —
/// what an `allValuesFrom class` axiom plus a `logic:ClosureEntry` derives — is a UNIVERSAL
/// `sh:class class` alongside a BARE `sh:minCount`/`sh:maxCount` on the same path. The two
/// are equivalent for this obligation, and the universal form is in fact the stronger of the
/// pair: a qualified count says "at least/at most n values that are a `class`", while
/// `sh:class` + a bare count says that AND that no value is anything else. What must not
/// pass is a path carrying the counts with no class obligation at all, or the class with no
/// count — either would let the retired shape's obligation through unenforced, which is the
/// only thing this equivalence check exists to catch.
fn assert_qualified(
    shape: &ValidationShapeIr,
    path: &str,
    class: &str,
    min: Option<u32>,
    max: Option<u32>,
) {
    let found = on_path(shape, path).iter().any(|p| {
        let qualified = p.components.iter().any(|c| match c {
            ConstraintComponent::QualifiedValueShape {
                shape: inner,
                min: lo,
                max: hi,
            } => {
                inner
                    .iter()
                    .any(|i| matches!(i, ConstraintComponent::Class(x) if x == class))
                    && min.is_none_or(|want| lo.is_some_and(|got| got >= want))
                    && max.is_none_or(|want| hi.is_some_and(|got| got <= want))
            }
            _ => false,
        });
        let universal = p
            .components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(x) if x == class))
            && min.is_none_or(|want| p.min_count.is_some_and(|got| got >= want))
            && max.is_none_or(|want| p.max_count.is_some_and(|got| got <= want));
        qualified || universal
    });
    assert!(
        found,
        "{} on {path} must carry the {class} count obligation — either \
         sh:qualifiedValueShape [ sh:class {class} ] with qualifiedMin={min:?} \
         qualifiedMax={max:?}, or a universal sh:class {class} with bare \
         minCount={min:?} maxCount={max:?}; derived: {:?}",
        shape.iri,
        on_path(shape, path)
    );
}

/// Validate `data_ttl` against the SHACL projection of `shape` and return the flagged focus nodes.
fn flag_with_shape(shape: &ValidationShapeIr, data_ttl: &str) -> Vec<String> {
    let doc = format!("{SH_HEADER}{}", project_validation_shape_shacl(shape));
    let shapes =
        parse_shapes(&doc).unwrap_or_else(|e| panic!("projected shape parses: {e}\n{doc}"));
    let data = purrdf::parse_dataset(data_ttl.as_bytes(), "text/turtle", None)
        .expect("fixture parses as Turtle");
    validate_dataset(&data, &shapes)
        .expect("validate")
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect()
}

/// Validate `data_ttl` against EVERY projected procedural constraint of `module_rel` and return
/// the flagged focus nodes. Asserts the module carries no `MALFORMED_CONSTRAINT`.
fn flag_with_procedural(module_rel: &str, data_ttl: &str) -> Vec<String> {
    let src = read_module(module_rel);
    let (program, diags) = parse_logic_str(&src, None).expect("module parses");
    let malformed: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_CONSTRAINT")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        malformed.is_empty(),
        "{module_rel} has MALFORMED_CONSTRAINT: {malformed:?}"
    );
    let shapes = parse_shapes(&project_procedural_constraints(&program))
        .expect("projected procedural constraints parse");
    let data = purrdf::parse_dataset(data_ttl.as_bytes(), "text/turtle", None)
        .expect("fixture parses as Turtle");
    validate_dataset(&data, &shapes)
        .expect("validate")
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Part A — the four shapes retired from slices/core/gts/shapes.ttl
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// `gmeow:OpaqueFrameShape` (declarative): `gmeow:opacityReason` exactly one
/// `gmeow:OpacityReason`, `gmeow:opaqueFrameIn` exactly one `gmeow:GTSSegment`.
#[test]
fn opaque_frame_shape_obligations_survive_as_derived_components() {
    let shapes = derived(GTS_MODULE);
    let s = class_shape(&shapes, &g("OpaqueFrame"));
    for (path, class) in [
        ("opacityReason", "OpacityReason"),
        ("opaqueFrameIn", "GTSSegment"),
    ] {
        let (p, c) = (g(path), g(class));
        // sh:class — the universal value typing (from owl:someValuesFrom).
        assert_class(s, &p, &c);
        // sh:minCount 1 / sh:maxCount 1 — carried as the qualified counts over exactly that
        // class, which the co-present universal sh:class makes the plain counts.
        assert_qualified(s, &p, &c, Some(1), None);
        assert_qualified(s, &p, &c, None, Some(1));
    }
}

/// `gmeow:GTSSegmentShape` (declarative): `gmeow:gtsHeadId` exactly one `xsd:string` matching
/// the blake3 pattern, `gmeow:gtsSegmentIndex` exactly one `xsd:nonNegativeInteger`,
/// `gmeow:gtsProfile` exactly one `gmeow:GTSProfile`.
#[test]
fn gts_segment_shape_obligations_survive_as_derived_components() {
    let shapes = derived(GTS_MODULE);
    let s = class_shape(&shapes, &g("GTSSegment"));

    let head = g("gtsHeadId");
    assert_datatype(s, &head, &format!("{XSD}string"));
    assert_min_count(s, &head, 1);
    assert_max_count(s, &head, 1);
    assert_pattern(s, &head, "^blake3:[0-9a-f]{64}$");

    let index = g("gtsSegmentIndex");
    assert_datatype(s, &index, &format!("{XSD}nonNegativeInteger"));
    assert_min_count(s, &index, 1);
    assert_max_count(s, &index, 1);

    let profile = g("gtsProfile");
    let gts_profile = g("GTSProfile");
    assert_class(s, &profile, &gts_profile);
    assert_qualified(s, &profile, &gts_profile, Some(1), None);
    assert_qualified(s, &profile, &gts_profile, None, Some(1));
}

/// The `sh:pattern` obligation is not merely present in the IR — it REJECTS a malformed head.
#[test]
fn gts_head_id_pattern_rejects_a_malformed_head_and_accepts_a_wellformed_one() {
    let shapes = derived(GTS_MODULE);
    let s = class_shape(&shapes, &g("GTSSegment"));
    let hex = "9f2c4e1a".repeat(8);
    let seg = |head: &str| {
        format!(
            "@prefix gmeow: <{GMEOW}> .\n\
             @prefix xsd: <{XSD}> .\n\
             @prefix ex: <http://example.org/gts/> .\n\
             ex:profileX a gmeow:GTSProfile .\n\
             ex:seg a gmeow:GTSSegment ;\n\
                 gmeow:gtsHeadId \"{head}\" ;\n\
                 gmeow:gtsSegmentIndex \"0\"^^xsd:nonNegativeInteger ;\n\
                 gmeow:gtsProfile ex:profileX .\n"
        )
    };
    let bad = flag_with_shape(s, &seg("not-a-blake3-head"));
    assert!(
        bad.iter().any(|f| f.contains("seg")),
        "a malformed gtsHeadId must be flagged; flagged: {bad:?}"
    );
    let good = flag_with_shape(s, &seg(&format!("blake3:{hex}")));
    assert!(
        good.is_empty(),
        "a well-formed segment must validate clean; flagged: {good:?}"
    );
}

/// `gmeow:SealedFrameRecipientShape` (procedural `sh:sparql`): a missing-key opaque frame
/// without a `gmeow:sealedRecipient` is flagged; one WITH a recipient is not.
#[test]
fn sealed_frame_recipient_constraint_flags_the_unaddressed_seal() {
    let frame = |recipient: &str| {
        format!(
            "@prefix gmeow: <{GMEOW}> .\n\
             @prefix ex: <http://example.org/gts/> .\n\
             ex:lillith a gmeow:Agent .\n\
             ex:seg a gmeow:GTSSegment .\n\
             ex:frameSealed a gmeow:OpaqueFrame ;\n\
                 gmeow:opaqueFrameIn ex:seg ;\n\
                 gmeow:opacityReason gmeow:opacityMissingKey{recipient} .\n"
        )
    };
    let unaddressed = flag_with_procedural(GTS_MODULE, &frame(""));
    assert!(
        unaddressed.iter().any(|f| f.contains("frameSealed")),
        "a missing-key frame with no gmeow:sealedRecipient must be flagged; flagged: \
         {unaddressed:?}"
    );
    let addressed = flag_with_procedural(
        GTS_MODULE,
        &frame(" ;\n    gmeow:sealedRecipient ex:lillith"),
    );
    assert!(
        !addressed.iter().any(|f| f.contains("frameSealed")),
        "a sealed frame naming its recipient must NOT be flagged; flagged: {addressed:?}"
    );
}

/// `gmeow:EvidenceCompactionShape` (procedural `sh:sparql`, `sh:Warning`): an evidence-profile
/// document generated by a `gmeow:GTSCompaction` is flagged; a dist-profile one is not.
#[test]
fn evidence_compaction_constraint_flags_the_recompacted_exhibit() {
    let doc = |profile: &str| {
        format!(
            "@prefix gmeow: <{GMEOW}> .\n\
             @prefix ex: <http://example.org/gts/> .\n\
             ex:compact1 a gmeow:GTSCompaction .\n\
             ex:exhibit a gmeow:GTSDocument ;\n\
                 gmeow:wasGeneratedBy ex:compact1 ;\n\
                 gmeow:gtsSegment ex:seg .\n\
             ex:seg a gmeow:GTSSegment ;\n\
                 gmeow:gtsSegmentOf ex:exhibit ;\n\
                 gmeow:gtsSegmentIndex 0 ;\n\
                 gmeow:gtsHeadId \"blake3:0000000000000000000000000000000000000000000000000000000000000000\" ;\n\
                 gmeow:gtsProfile gmeow:{profile} .\n"
        )
    };
    let evidence = flag_with_procedural(GTS_MODULE, &doc("gtsProfileEvidence"));
    assert!(
        evidence.iter().any(|f| f.contains("exhibit")),
        "a compacted evidence-profile document must be flagged; flagged: {evidence:?}"
    );
    let dist = flag_with_procedural(GTS_MODULE, &doc("gtsProfileDist"));
    assert!(
        !dist.iter().any(|f| f.contains("exhibit")),
        "a compacted dist-profile document must NOT be flagged; flagged: {dist:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Part B — lang:GmnEnvelopeContractShape
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// The NINE envelope contract fields, with the shape components each must derive.
/// Seven ride a vacuous `owl:onClass owl:Thing` qualifier → a plain `sh:minCount 1`; the two
/// named-class-qualified ones ride `sh:qualifiedMinCount 1` + `sh:class` + `sh:nodeKind sh:IRI`.
const ENVELOPE_PLAIN_FIELDS: [&str; 7] = [
    "gmnSchemaVersion",
    "gmnDictionaryVersion",
    "gmnGlyphTableVersion",
    "accordingTo",
    "wasGeneratedBy",
    "contentDigest",
    // The field the retired eight-block shape never covered — an envelope missing only this one
    // passed the hand-authored contract while the class prose called it a nine-field contract.
    "gmnCodebookDigest",
];

/// `lang:GmnEnvelopeContractShape`: all NINE fields, the failure class, and the two
/// named-class obligations (`sh:class` + `sh:nodeKind sh:IRI`) the shape carried by hand.
#[test]
fn gmn_envelope_derived_shape_covers_all_nine_contract_fields() {
    let shapes = derived(LANG_MODULE);
    let s = class_shape(&shapes, &g("GmnEnvelope"));

    for field in ENVELOPE_PLAIN_FIELDS {
        assert_min_count(s, &g(field), 1);
    }
    // The plural-pinned coordinate keeps its cap.
    assert_max_count(s, &g("gmnDictionaryVersion"), 1);

    // gmeow:gmnSecurityRing → exactly one gmeow:GmnSecurityRing, an IRI.
    let ring = g("gmnSecurityRing");
    let ring_class = g("GmnSecurityRing");
    assert_class(s, &ring, &ring_class);
    assert_qualified(s, &ring, &ring_class, Some(1), None);
    assert_qualified(s, &ring, &ring_class, None, Some(1));
    assert_node_kind(s, &ring, ShaclNodeKind::Iri);

    // gmeow:gmnEnvelopeCorrespondence → at least one logic:Correspondence, an IRI.
    let corr = g("gmnEnvelopeCorrespondence");
    let corr_class = format!("{LOGIC}Correspondence");
    assert_class(s, &corr, &corr_class);
    assert_qualified(s, &corr, &corr_class, Some(1), None);
    assert_node_kind(s, &corr, ShaclNodeKind::Iri);

    assert_eq!(
        s.failure_class.as_deref(),
        Some(format!("{LANG}GmnMissingEnvelopeField").as_str()),
        "the derived envelope shape must carry gmeow:enforcesFailureClass \
         lang:GmnMissingEnvelopeField"
    );
}

/// A complete nine-field envelope, or one with `omit` left out.
fn envelope_fixture(omit: &str) -> String {
    let fields: [(&str, &str); 9] = [
        ("gmnSchemaVersion", "\"1\""),
        ("gmnDictionaryVersion", "\"3\""),
        ("gmnGlyphTableVersion", "\"2\""),
        ("gmnSecurityRing", "ex:ringCore"),
        ("accordingTo", "ex:archiveTeam"),
        ("wasGeneratedBy", "ex:gmnWriterRun"),
        ("contentDigest", "\"blake3:0d1e2f3a\""),
        ("gmnCodebookDigest", "\"blake3:f93d8b83\""),
        ("gmnEnvelopeCorrespondence", "ex:corrEnvelope"),
    ];
    let body = fields
        .iter()
        .filter(|(name, _)| *name != omit)
        .map(|(name, value)| format!("    gmeow:{name} {value} ;\n"))
        .collect::<String>();
    format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix logic: <{LOGIC}> .\n\
         @prefix ex: <http://example.org/lang/> .\n\
         ex:archiveTeam a gmeow:Agent .\n\
         ex:gmnWriterRun a gmeow:Activity .\n\
         ex:ringCore a gmeow:GmnSecurityRing .\n\
         ex:corrEnvelope a logic:Correspondence .\n\
         ex:envelope a gmeow:GmnEnvelope ;\n{body}    a gmeow:GmnEnvelope .\n"
    )
}

/// THE regression this migration exists to close: an envelope missing ONLY
/// `gmeow:gmnCodebookDigest` — the ninth field the retired eight-block hand-authored shape had
/// no counterpart for — is REJECTED by the derived shape.
#[test]
fn envelope_missing_only_the_codebook_digest_is_rejected() {
    let shapes = derived(LANG_MODULE);
    let s = class_shape(&shapes, &g("GmnEnvelope"));
    let flagged = flag_with_shape(s, &envelope_fixture("gmnCodebookDigest"));
    assert!(
        flagged.iter().any(|f| f.contains("envelope")),
        "an envelope missing only gmeow:gmnCodebookDigest must be REJECTED; flagged: {flagged:?}"
    );
}

/// The negative control for the test above: the complete nine-field envelope validates clean, so
/// the rejection is the missing field and not a broken fixture.
#[test]
fn complete_nine_field_envelope_validates_clean() {
    let shapes = derived(LANG_MODULE);
    let s = class_shape(&shapes, &g("GmnEnvelope"));
    let flagged = flag_with_shape(s, &envelope_fixture(""));
    assert!(
        flagged.is_empty(),
        "a complete nine-field envelope must validate clean; flagged: {flagged:?}"
    );
}

/// Every one of the nine fields is individually load-bearing: omitting ANY single one is
/// rejected. This is what makes the coverage claim non-vacuous — a dropped restriction reds here.
#[test]
fn omitting_any_single_envelope_field_is_rejected() {
    let shapes = derived(LANG_MODULE);
    let s = class_shape(&shapes, &g("GmnEnvelope"));
    for field in ENVELOPE_PLAIN_FIELDS
        .iter()
        .chain(["gmnSecurityRing", "gmnEnvelopeCorrespondence"].iter())
    {
        let flagged = flag_with_shape(s, &envelope_fixture(field));
        assert!(
            flagged.iter().any(|f| f.contains("envelope")),
            "an envelope missing gmeow:{field} must be rejected; flagged: {flagged:?}"
        );
    }
}

/// The node-kind obligation the hand-authored shape carried on both named-class fields is
/// enforced, not merely declared: a blank-node ring / correspondence is flagged.
#[test]
fn envelope_named_class_fields_reject_a_blank_node_value() {
    let shapes = derived(LANG_MODULE);
    let s = class_shape(&shapes, &g("GmnEnvelope"));
    let blank_ring = envelope_fixture("gmnSecurityRing").replace(
        "ex:envelope a gmeow:GmnEnvelope ;\n",
        "ex:envelope a gmeow:GmnEnvelope ;\n    \
         gmeow:gmnSecurityRing [ a gmeow:GmnSecurityRing ] ;\n",
    );
    let flagged = flag_with_shape(s, &blank_ring);
    assert!(
        flagged.iter().any(|f| f.contains("envelope")),
        "a blank-node gmeow:gmnSecurityRing must be rejected (sh:nodeKind sh:IRI); flagged: \
         {flagged:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Part C — gmeow:RightsStatement statementAbout Entity range (Principle 17)
//
// gmeow:statementAbout carries an Entity value range: every asset a RightsStatement governs is a
// gmeow:Entity, exactly one, an IRI/blank node. It is authored as logic:someValuesFrom
// gmeow:Entity which, under the exact-one (min1/max1, onClass logic:Thing) cardinality declared
// on the class, derives exactly the universal sh:class gmeow:Entity plus bare exact-one counts a
// closed-world allValuesFrom would — so the existential authoring already closes the range and no
// separate logic:ClosureEntry is needed. These tests pin the derived obligation and prove it is
// ENFORCED, so a future edit that drops the restriction reds here.
// ─────────────────────────────────────────────────────────────────────────────────────────────

const RIGHTS_MODULE: &str = "slices/core/rights/module.ttl";

/// The pipeline single-outcome axes are FUNCTIONAL: a gmeow:StageExecution ends in at most
/// one gmeow:StageDisposition, and a gmeow:PipelineStage carries at most one
/// gmeow:StageObligation and one gmeow:StageStability. The max-qualified restrictions derive
/// the sh:qualifiedMaxCount 1 bound over the value class, alongside the sh:in value closure.
#[test]
fn pipeline_stage_outcome_axes_are_functional() {
    let shapes = derived("slices/core/pipeline/module.ttl");
    let exec = class_shape(&shapes, &g("StageExecution"));
    // stageDisposition is exactly-one (a run ends in one disposition; none is incomplete) —
    // the min and max ride separate qualified components, so assert each.
    assert_qualified(
        exec,
        &g("stageDisposition"),
        &g("StageDisposition"),
        Some(1),
        None,
    );
    assert_qualified(
        exec,
        &g("stageDisposition"),
        &g("StageDisposition"),
        None,
        Some(1),
    );
    let stage = class_shape(&shapes, &g("PipelineStage"));
    assert_qualified(
        stage,
        &g("stageObligation"),
        &g("StageObligation"),
        None,
        Some(1),
    );
    assert_qualified(
        stage,
        &g("stageStability"),
        &g("StageStability"),
        None,
        Some(1),
    );
}

/// A minimal RightsStatement governing `ex:asset`. Only gmeow:statementAbout is set; every
/// other RightsStatement property is optional, so a well-typed target validates clean and the
/// only obligation under test is the closed-world gmeow:Entity range. `target_type` is the
/// asset's `rdf:type` local, or `None` to leave it untyped (a non-Entity value).
fn rights_statement_fixture(target_type: Option<&str>) -> String {
    let decl = target_type
        .map(|t| format!("ex:asset a gmeow:{t} .\n"))
        .unwrap_or_default();
    format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix ex: <http://example.org/rights/> .\n\
         {decl}\
         ex:governingStatement a gmeow:RightsStatement ;\n\
             gmeow:statementAbout ex:asset .\n"
    )
}

/// The derived RightsStatement shape carries the closed-world range obligation on
/// gmeow:statementAbout: universal sh:class gmeow:Entity plus bare exact-one counts.
#[test]
fn rights_statement_about_derives_closed_world_entity_range() {
    let shapes = derived(RIGHTS_MODULE);
    let s = class_shape(&shapes, &g("RightsStatement"));
    let path = g("statementAbout");
    // The universal value typing — the Entity range (logic:someValuesFrom under exact-one).
    assert_class(s, &path, &g("Entity"));
    // Presence + functionality: exactly one governed asset (the onClass logic:Thing exact-one).
    assert_min_count(s, &path, 1);
    assert_max_count(s, &path, 1);
}

/// The range is ENFORCED, not merely declared: a gmeow:Entity target validates clean while a
/// non-Entity (untyped) target is rejected by the closed-world sh:class gmeow:Entity.
#[test]
fn rights_statement_about_rejects_a_non_entity_target() {
    let shapes = derived(RIGHTS_MODULE);
    let s = class_shape(&shapes, &g("RightsStatement"));

    let good = flag_with_shape(s, &rights_statement_fixture(Some("Entity")));
    assert!(
        good.is_empty(),
        "a RightsStatement governing a gmeow:Entity must validate clean; flagged: {good:?}"
    );

    let bad = flag_with_shape(s, &rights_statement_fixture(None));
    assert!(
        bad.iter().any(|f| f.contains("governingStatement")),
        "a RightsStatement whose gmeow:statementAbout value is not a gmeow:Entity must be \
         REJECTED by the closed-world range; flagged: {bad:?}"
    );
}
