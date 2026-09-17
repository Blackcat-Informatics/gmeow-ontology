// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF-isomorphic projection back-ends: OWL-DL, OWL-EL, gUFO, canonical-RDF12.
//!
//! These build a wasm-clean triple set ([`RdfDatasetBuilder`]) and serialize to
//! Turtle through the native codec.  The conformance goldens compare these targets
//! by **graph isomorphism** (not bytes), so the serialization need only reproduce
//! the same triples.  This is the single source of truth for these projections.

use crate::ir::AtomicTerm;

use std::collections::HashSet;

use purrdf::{RdfDatasetBuilder, RdfLiteral, SerializeGraph, serialize_dataset};

use std::collections::BTreeMap;

use super::super::graphutil::sha256_12;
use super::super::ir::{Formula, LogicAxiom, LogicModality, LogicProgram, NodeKind, Term};
use super::super::restriction;
use super::{
    GMEOW_NS, LOGIC_NS, OWL_NS, OverclaimError, ProjectionResult, RDF_NS, RDF_TYPE, RDFS_NS,
    XSD_NS, assert_no_overclaim, contract_drop_notes, generated_banner, is_modal_or_scoped,
    target_meta,
};

const GUFO_NS: &str = "http://purl.org/nemo/gufo#";

fn rdfs(local: &str) -> String {
    format!("{RDFS_NS}{local}")
}
fn owl(local: &str) -> String {
    format!("{OWL_NS}{local}")
}
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

/// Resolve a facet *value* to its emitted IRI. The open facet-value vocabulary
/// admits values that are already full custom IRIs (not under the
/// `logic:` namespace); these must be emitted verbatim, not re-prefixed (which
/// would yield a corrupt `…/logic/https://…`). A bare local name is prefixed under
/// the `logic:` namespace. This is symmetric with the front-end storage convention
/// (`strip_prefix(LOGIC_NAMESPACE).unwrap_or(full_iri)`), so a custom IRI
/// round-trips identically.
fn facet_value_iri(value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_owned()
    } else {
        logic(value)
    }
}

// --------------------------------------------------------------------------- //
// Projection-side mapping tables (the authoritative logic: → OWL/gUFO maps)
// --------------------------------------------------------------------------- //

/// logic: sort IRI → gUFO class IRI (the 37 faithful down-projection targets).
fn gufo_for_sort(obj: &str) -> Option<String> {
    let local = obj.strip_prefix(LOGIC_NS)?;
    let g = match local {
        "Kind" => "Kind",
        "SubKind" => "SubKind",
        "Phase" => "Phase",
        "Role" => "Role",
        "Category" => "Category",
        "Mixin" => "Mixin",
        "RoleMixin" => "RoleMixin",
        "PhaseMixin" => "PhaseMixin",
        "Relator" => "Relator",
        "Event" => "EventType",
        "Situation" => "SituationType",
        "Individual" => "Individual",
        "ConcreteIndividual" => "ConcreteIndividual",
        "AbstractIndividual" => "AbstractIndividual",
        "Endurant" => "Endurant",
        "Participation" => "Participation",
        "Object" => "Object",
        "Aspect" => "Aspect",
        "Quality" => "Quality",
        "QualityValue" => "QualityValue",
        "Collection" => "Collection",
        "FixedCollection" => "FixedCollection",
        "VariableCollection" => "VariableCollection",
        "Quantity" => "Quantity",
        "FunctionalComplex" => "FunctionalComplex",
        "Type" => "Type",
        "EndurantType" => "EndurantType",
        "RelationshipType" => "RelationshipType",
        "MaterialRelationshipType" => "MaterialRelationshipType",
        "ComparativeRelationshipType" => "ComparativeRelationshipType",
        "AbstractIndividualType" => "AbstractIndividualType",
        "ConcreteIndividualType" => "ConcreteIndividualType",
        "Sortal" => "Sortal",
        "NonSortal" => "NonSortal",
        "RigidType" => "RigidType",
        "AntiRigidType" => "AntiRigidType",
        "SemiRigidType" => "SemiRigidType",
        "NonRigidType" => "NonRigidType",
        _ => return None,
    };
    Some(format!("{GUFO_NS}{g}"))
}

/// logic: structural predicate IRI → OWL/RDFS predicate IRI.
fn owl_for_pred(pred: &str) -> Option<String> {
    let local = pred.strip_prefix(LOGIC_NS)?;
    Some(match local {
        "subClassOf" => rdfs("subClassOf"),
        "equivalentClass" => owl("equivalentClass"),
        "disjointWith" => owl("disjointWith"),
        "subPropertyOf" => rdfs("subPropertyOf"),
        "equivalentProperty" => owl("equivalentProperty"),
        "inverseOf" => owl("inverseOf"),
        "domain" => rdfs("domain"),
        "range" => rdfs("range"),
        _ => return None,
    })
}

/// logic: characteristic sort IRI → OWL characteristic-type IRI.
fn owl_for_char(obj: &str) -> Option<String> {
    let local = obj.strip_prefix(LOGIC_NS)?;
    Some(match local {
        "transitiveProperty" => owl("TransitiveProperty"),
        "symmetricProperty" => owl("SymmetricProperty"),
        "functionalProperty" => owl("FunctionalProperty"),
        "inverseFunctionalProperty" => owl("InverseFunctionalProperty"),
        "reflexiveProperty" => owl("ReflexiveProperty"),
        "asymmetricProperty" => owl("AsymmetricProperty"),
        "irreflexiveProperty" => owl("IrreflexiveProperty"),
        _ => return None,
    })
}

/// The characteristic sort local names an OWL grounding view re-emits from a canonical
/// `logic:PropertyCharacteristicAssertion` carrier. These are exactly the sorts that OWL 2 DL
/// admits on a GENERAL object property: `functionalProperty` and `inverseFunctionalProperty`
/// (single-carrier since the direct marker was retired at source), and `transitiveProperty` /
/// `symmetricProperty` (the marker was flipped to its `logic:` spelling but the canonical
/// carrier is the record). `asymmetricProperty` and `irreflexiveProperty` are DELIBERATELY
/// absent: OWL 2 DL forbids them on a non-simple property, so — exactly as `logic:properPartOf`
/// keeps only `owl:TransitiveProperty` (see the holon-loss note) — they stay `logic:`-only and
/// are never projected. `owl_for_char` maps each of these to its OWL characteristic class.
const DL_PROJECTABLE_CHARACTERISTIC_SORTS: [&str; 4] = [
    "functionalProperty",
    "inverseFunctionalProperty",
    "transitiveProperty",
    "symmetricProperty",
];

/// The object properties a `logic:PropertyCharacteristicAssertion` characterises with a
/// DL-PROJECTABLE sort, each paired with that sort's local name, joining `logic:characterizes ?P`
/// with `logic:characteristicSort logic:<sort>` on the record IRI. This central record is the
/// CANONICAL carrier of the characteristic; the `owl:{…}Property` rdf:type marker is its lossy
/// projection and is no longer an authored slice source. The OWL grounding view re-emits the
/// matching `owl:{…}Property` from this record (a valid lossy down-projection of the canonical
/// characteristic), so the characteristic survives the removal of the direct
/// `?P rdf:type owl:{…}Property` marker — exactly as each carrier record's prose promises, and
/// mirroring the direct-marker `owl_for_char` emission (the OWL type + `owl:ObjectProperty`).
/// Returned as a sorted (property, sort-local) BTreeSet for a deterministic, idempotent view.
fn dl_projectable_carrier_characteristics(
    program: &LogicProgram,
) -> std::collections::BTreeSet<(String, String)> {
    let characterizes = logic("characterizes");
    let characteristic_sort = logic("characteristicSort");
    let mut rec_prop: BTreeMap<String, String> = BTreeMap::new();
    let mut rec_sort: BTreeMap<String, String> = BTreeMap::new();
    for ax in &program.axioms {
        let Some(object) = ax.obj.as_iri() else {
            continue;
        };
        if ax.predicate == characterizes {
            rec_prop.insert(ax.subject.clone(), object.to_owned());
        } else if ax.predicate == characteristic_sort
            && let Some(local) = object.strip_prefix(LOGIC_NS)
            && DL_PROJECTABLE_CHARACTERISTIC_SORTS.contains(&local)
        {
            rec_sort.insert(ax.subject.clone(), local.to_owned());
        }
    }
    rec_prop
        .iter()
        .filter_map(|(rec, prop)| rec_sort.get(rec).map(|sort| (prop.clone(), sort.clone())))
        .collect()
}

fn is_el_safe_pred(pred: &str) -> bool {
    matches!(
        pred.strip_prefix(LOGIC_NS),
        Some("subClassOf" | "equivalentClass" | "subPropertyOf" | "domain" | "range")
    )
}

fn is_el_safe_char(obj: &str) -> bool {
    obj.strip_prefix(LOGIC_NS) == Some("transitiveProperty")
}

/// Whether a lifted restriction constraint local name is expressible in OWL 2 EL.
/// `someValuesFrom` and `hasValue` are EL-safe; the remaining families
/// (`allValuesFrom`, cardinality, `oneOf`, …) are not and force the whole restriction
/// to drop from the EL projection.
fn is_el_safe_restriction_constraint(local: &str) -> bool {
    matches!(local, "someValuesFrom" | "hasValue")
}

// --------------------------------------------------------------------------- //
// Triple sink + deterministic Turtle serialization
// --------------------------------------------------------------------------- //

/// Accumulates triples (default graph) and serializes them to deterministic
/// Turtle. Predicates are IRIs; subjects preserve the compact IR's resource kind
/// (absolute IRI or canonical blank label); objects retain their native kind.
/// Variables travel only through the explicit reified formula/rule paths.
#[derive(Default)]
pub(crate) struct TripleSink {
    pub(super) builder: RdfDatasetBuilder,
}

impl TripleSink {
    fn intern_subject(&mut self, subject: &str) -> purrdf::TermId {
        assert!(
            !subject.starts_with('?'),
            "logical variables require a reified projection path"
        );
        if crate::ir::atomic::is_absolute_iri(subject) {
            self.builder.intern_iri(subject)
        } else {
            self.builder.intern_blank(
                subject.trim_start_matches("_:"),
                purrdf::BlankScope::DEFAULT,
            )
        }
    }

    pub(crate) fn add_iri(&mut self, s: &str, p: &str, o: &str) {
        let s = self.intern_subject(s);
        let p = self.builder.intern_iri(p);
        let o = self.builder.intern_iri(o);
        self.builder.push_quad(s, p, o, None);
    }

    pub(crate) fn add_lit(&mut self, s: &str, p: &str, lit: RdfLiteral) {
        let s = self.intern_subject(s);
        let p = self.builder.intern_iri(p);
        let o = self.builder.intern_literal(lit);
        self.builder.push_quad(s, p, o, None);
    }

    /// Emit the exact native object. Variables use the explicit reified rule path.
    fn add_atomic(&mut self, s: &str, p: &str, object: &AtomicTerm) {
        let subject = self.intern_subject(s);
        let predicate = self.builder.intern_iri(p);
        let object = match object {
            AtomicTerm::Var(value) => self.builder.intern_literal(RdfLiteral::simple(value)),
            AtomicTerm::Iri(value) => self.builder.intern_iri(value),
            AtomicTerm::Blank(value) => self
                .builder
                .intern_blank(value, purrdf::BlankScope::DEFAULT),
            AtomicTerm::Literal(value) => self.builder.intern_literal(value.clone()),
        };
        self.builder.push_quad(subject, predicate, object, None);
    }

    /// Carry the native annotation literal, including its authored carrier tag.
    pub(crate) fn add_annotation(&mut self, axiom: &LogicAxiom) {
        debug_assert!(axiom.obj.is_literal(), "annotation must be literal-valued");
        self.add_atomic(&axiom.subject, &axiom.predicate, &axiom.obj);
    }

    /// Serialize to Turtle with a GENERATED banner.  The triple set is frozen into
    /// the `RdfDataset` IR and serialized through the native codec, which
    /// emits canonical, deterministic Turtle — so no manual pre-sort is needed (the
    /// goldens compare by isomorphism either way). All projected quads live in the
    /// default graph, so `SerializeGraph::DefaultGraph` is the faithful selector.
    /// # Panics
    ///
    /// If the accumulated triples cannot be frozen or serialized — most often because
    /// a term is not a legal IRI (the RDF IR refuses to intern one containing a
    /// control character or another disallowed codepoint). The underlying diagnostic
    /// is carried into the panic message: it names the offending term, the codepoint
    /// and its byte offset, which is the whole difference between a defect a reader
    /// can act on and one they cannot.
    pub(crate) fn serialize(self, banner: &str) -> String {
        let body = self
            .serialize_as("text/turtle")
            .unwrap_or_else(|e| panic!("constructed triple set must serialize as Turtle: {e}"));
        let body = format!("{}\n", body.trim_end_matches('\n'));
        format!("{banner}{body}")
    }

    /// Serialize the accumulated default graph in `media_type` without a generated banner.
    /// Kept separate from [`Self::serialize`] so correspondence-owned formula trees can be
    /// embedded in their deterministic N-Triples carrier without reimplementing the formula
    /// projection.
    ///
    /// # Errors
    ///
    /// The freeze or the codec refused. Returning the diagnostic rather than `None` is
    /// the point: `.ok()` here previously discarded exactly the message that says WHICH
    /// term was rejected and why, leaving callers to panic with a sentence that could
    /// not distinguish an illegal IRI from an encoding fault.
    fn serialize_as(self, media_type: &str) -> gmeow_errors::Result<String> {
        let projection_err =
            |detail: String| gmeow_errors::Diag::of_kind(crate::error::Projection { detail });
        let dataset = self
            .builder
            .freeze()
            .map_err(|e| projection_err(e.to_string()))?;
        let bytes = serialize_dataset(dataset.as_ref(), media_type, SerializeGraph::DefaultGraph)
            .map_err(|e| projection_err(e.to_string()))?;
        String::from_utf8(bytes)
            .map_err(|e| projection_err(format!("serialized {media_type} is not UTF-8: {e}")))
    }
}

/// A typed RDF projection with its target judgment. The target's structural and
/// actual drops are owned by the caller's single [`crate::loss_ledger::LossLedger`],
/// exactly as for [`ProjectionResult`]. Keeping this dataset native does not
/// strengthen the preservation claim or discharge correspondence laws.
#[derive(Debug, Clone)]
pub struct RdfProjectionResult {
    /// Selected projection target.
    pub target: String,
    /// The admitted RDF projection, ready for another native stage.
    pub dataset: std::sync::Arc<purrdf::RdfDataset>,
    /// Declared preservation judgment checked against actual drops.
    pub preservation: super::PreservationKind,
    /// The target's declared complexity class.
    pub complexity: String,
    banner: String,
}

impl RdfProjectionResult {
    /// Metadata-only row for the shared projection report. The native dataset
    /// remains the content owner; the report never reads serialized payloads.
    #[must_use]
    pub fn report_row(&self) -> ProjectionResult {
        ProjectionResult {
            target: self.target.clone(),
            content: String::new(),
            is_rdf: true,
            preservation: self.preservation,
            complexity: self.complexity.clone(),
        }
    }

    /// Render the terminal Turtle artifact. Native consumers use `dataset`
    /// directly and never pay for serialization or a subsequent parse.
    ///
    /// # Panics
    /// If the native codec cannot serialize the already-admitted projection.
    #[must_use]
    pub fn into_text(self) -> ProjectionResult {
        let bytes = serialize_dataset(&self.dataset, "text/turtle", SerializeGraph::DefaultGraph)
            .expect("admitted RDF projection must serialize as Turtle");
        let body = String::from_utf8(bytes).expect("Turtle serialization is UTF-8");
        let content = format!("{}{}\n", self.banner, body.trim_end_matches('\n'));
        ProjectionResult {
            target: self.target,
            content,
            is_rdf: true,
            preservation: self.preservation,
            complexity: self.complexity,
        }
    }
}

/// Admit a typed RDF target after checking its actual residue. The caller interns
/// the drops once; neither native execution nor terminal serialization duplicates
/// or weakens that ledger.
fn rdf_result(
    target: &str,
    sink: TripleSink,
    banner_label: &str,
    actual_drops: &[String],
) -> Result<RdfProjectionResult, OverclaimError> {
    let (kind, cx, _drops) = target_meta(target);
    let residue: Vec<&str> = actual_drops.iter().map(String::as_str).collect();
    assert_no_overclaim(target, kind, &residue)?;
    let dataset = sink.builder.freeze().map_err(|error| {
        OverclaimError(format!(
            "{target} RDF projection failed native admission: {error}"
        ))
    })?;
    Ok(RdfProjectionResult {
        target: target.to_owned(),
        dataset,
        preservation: kind,
        complexity: cx.to_owned(),
        banner: generated_banner(banner_label),
    })
}

/// Intern a lossy RDF target's structural (from [`target_meta`]) + per-run `actual_drops`
/// into the single loss store, keyed by the target focus. `attributed` maps a drop note (by
/// its EXACT string — never scraped) to the DOCUMENTED gmeow: source term it concerns (e.g. a
/// rule-head predicate with no OWL form), so that drop lands on the term's per-term
/// projection-loss table; a note absent from the map stays whole-program.
fn intern_rdf_drops(
    loss: &mut crate::loss_ledger::LossLedger,
    target: &str,
    actual_drops: &[String],
    attributed: &std::collections::BTreeMap<String, String>,
) {
    let (kind, _, structural) = target_meta(target);
    let structural: Vec<String> = structural.into_iter().map(str::to_owned).collect();
    let drops: Vec<(String, Option<String>)> = actual_drops
        .iter()
        .map(|note| (note.clone(), attributed.get(note).cloned()))
        .collect();
    loss.record_projection_drops_attributed(target, kind, &structural, &drops);
}

// --------------------------------------------------------------------------- //
// Holon surface helper
// --------------------------------------------------------------------------- //

/// Emit the OWL skeleton for the holon vocabulary: class and property
/// declarations that survive the lossy OWL projection.  Called only when the
/// program actually uses `logic:properPartOf` (i.e. it has holon axioms).
///
/// Loss ledger rationale:
/// * `asymmetric` + `irreflexive` characteristics on `logic:properPartOf` are
///   not expressible in OWL 2 DL; only `owl:TransitiveProperty` survives.
/// * The five-place `logic:HolonicPosition` relation is projected as the unary
///   `logic:Holon` class; positional arity is dropped.
/// * `logic:WeakSupplementation` stays in the logic: layer; no OWL lowering.
fn emit_holon_surface(g: &mut TripleSink) {
    // Classes
    for cls in &["Holon", "HolonicPosition", "Holarchy"] {
        g.add_iri(&logic(cls), RDF_TYPE, &owl("Class"));
    }
    // Object properties
    for prop in &[
        "properPartOf",
        "isHolon",
        "positionEntity",
        "positionHolarchy",
        "positionContext",
        "positionInterval",
        "positionPath",
    ] {
        g.add_iri(&logic(prop), RDF_TYPE, &owl("ObjectProperty"));
    }
    // properPartOf also gets TransitiveProperty (transitivity survives;
    // asymmetric + irreflexive are the documented loss).
    g.add_iri(&logic("properPartOf"), RDF_TYPE, &owl("TransitiveProperty"));
    // Datatype property
    g.add_iri(&logic("holonicLevel"), RDF_TYPE, &owl("DatatypeProperty"));
}

// --------------------------------------------------------------------------- //
// Class-expression restrictions (owl:Restriction re-emission)
// --------------------------------------------------------------------------- //

/// A lifted restriction reconstructed from the flat `logic:` axiom set for OWL
/// re-emission.  The `node` is the skolem IRI (`logic:restriction/<hash>`) that serves
/// as the (non-blank) `owl:Restriction` node.
#[derive(Default)]
struct LiftedRestriction {
    on_property: Option<String>,
    /// Constraint local name and complete native filler/value, in axiom order.
    constraints: Vec<(String, AtomicTerm)>,
}

/// Collect every lifted restriction from the program's flat axioms, keyed by skolem
/// node IRI.  A node qualifies once it is typed `logic:Restriction` or carries
/// `logic:onProperty`; its constraints are the [`restriction::CONSTRAINT_LOCALS`]
/// predicates on it.  The `BTreeMap` gives a deterministic emission order.
fn collect_lifted_restrictions(program: &LogicProgram) -> BTreeMap<String, LiftedRestriction> {
    let restriction_ty = logic(restriction::RESTRICTION_CLASS_LOCAL);
    let on_property = logic(restriction::ON_PROPERTY_LOCAL);
    let mut out: BTreeMap<String, LiftedRestriction> = BTreeMap::new();
    for axiom in &program.axioms {
        let pred = axiom.predicate.as_str();
        if pred == RDF_TYPE && axiom.obj.as_iri() == Some(restriction_ty.as_str()) {
            out.entry(axiom.subject.clone()).or_default();
        } else if pred == on_property {
            out.entry(axiom.subject.clone()).or_default().on_property =
                axiom.obj.as_iri().map(str::to_owned);
        } else if let Some(local) = pred.strip_prefix(LOGIC_NS)
            && restriction::CONSTRAINT_LOCALS.contains(&local)
        {
            out.entry(axiom.subject.clone())
                .or_default()
                .constraints
                .push((local.to_owned(), axiom.obj.clone()));
        }
    }
    out
}

/// Emit the `owl:Restriction` graph for one lifted restriction (used by OWL 2 DL,
/// which expresses every restriction family).  The skolem `node` IRI is the restriction
/// node; the `C rdfs:subClassOf node` anchor is emitted by the main axiom loop from the
/// `logic:subClassOf` axiom.  A restriction missing `onProperty` or constraints is
/// structurally incomplete and is skipped (the lift never produces one).
fn emit_restriction(
    g: &mut TripleSink,
    node: &str,
    r: &LiftedRestriction,
) -> Result<(), OverclaimError> {
    let Some(on_property) = &r.on_property else {
        return Ok(());
    };
    if r.constraints.is_empty() {
        return Ok(());
    }
    g.add_iri(node, RDF_TYPE, &owl("Restriction"));
    g.add_iri(node, &owl(restriction::ON_PROPERTY_LOCAL), on_property);
    for (local, object) in &r.constraints {
        if matches!(
            local.as_str(),
            "cardinality"
                | "minCardinality"
                | "maxCardinality"
                | "qualifiedCardinality"
                | "minQualifiedCardinality"
                | "maxQualifiedCardinality"
        ) {
            let literal = owl_cardinality_literal(object).ok_or_else(|| OverclaimError(
                format!("OWL restriction <{node}> logic:{local} requires a non-negative integer count, found {}", object.key()),
            ))?;
            g.add_atomic(node, &owl(local), &AtomicTerm::Literal(literal));
        } else {
            g.add_atomic(node, &owl(local), object);
        }
    }
    Ok(())
}

/// The OWL RDF mapping requires xsd:nonNegativeInteger at this target boundary.
/// Preserve the authored value in the IR; validate its integer value with PurRDF.
/// Plain strings are the explicit lexical-count convention of logic cardinalities.
fn owl_cardinality_literal(object: &AtomicTerm) -> Option<RdfLiteral> {
    let literal = object.as_literal()?;
    if literal.language.is_some() || literal.direction.is_some() {
        return None;
    }
    let datatype = if literal.datatype_iri() == "http://www.w3.org/2001/XMLSchema#string" {
        purrdf::xsd::XsdDatatype::NonNegativeInteger
    } else {
        purrdf::xsd::XsdDatatype::from_iri(literal.datatype_iri())?
    };
    let purrdf::xsd::XsdValue::Integer { value, .. } =
        purrdf::xsd::parse(&literal.lexical_form, datatype).ok()?
    else {
        return None;
    };
    (value >= 0).then(|| {
        RdfLiteral::typed(
            value.to_string(),
            "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
        )
    })
}

/// Collect every lifted anonymous enumeration (`logic:enumeration/<hash>` typed
/// `logic:Enumeration`, carrying `logic:oneOf` members), keyed by skolem node IRI.  The
/// typed members arrive object-sorted because `program.axioms` is globally
/// ordered by `LogicAxiom::sort_key` (see `LogicProgram::new` in `ir.rs`), but this
/// collector also sorts+dedups each member list locally so the deterministic `owl:oneOf`
/// list is guaranteed here rather than relying on that non-local ordering.
fn collect_lifted_enumerations(program: &LogicProgram) -> BTreeMap<String, Vec<AtomicTerm>> {
    let enumeration_ty = logic(restriction::ENUMERATION_CLASS_LOCAL);
    let one_of = logic(restriction::ONE_OF_LOCAL);
    let mut out: BTreeMap<String, Vec<AtomicTerm>> = BTreeMap::new();
    for axiom in &program.axioms {
        let pred = axiom.predicate.as_str();
        if pred == RDF_TYPE && axiom.obj.as_iri() == Some(enumeration_ty.as_str()) {
            out.entry(axiom.subject.clone()).or_default();
        } else if pred == one_of {
            out.entry(axiom.subject.clone())
                .or_default()
                .push(axiom.obj.clone());
        }
    }
    // Belt-and-braces determinism: program.axioms is already globally ordered by
    // LogicAxiom::sort_key (see LogicProgram::new in ir.rs), so members arrive
    // object-sorted; sort+dedup here makes the guarantee local rather than relying
    // on that non-local ordering.
    for members in out.values_mut() {
        members.sort();
        members.dedup();
    }
    out
}

/// Emit the `owl:oneOf` enumeration graph for one lifted enumeration (OWL 2 DL): the
/// skolem node typed `owl:Class` with an `owl:oneOf` `rdf:List` of the members (minted
/// as deterministic list-cell IRIs, never blank nodes).  Literal members are not valid
/// OWL individuals, so an enumeration is emitted only when all members are IRIs.
fn emit_enumeration(g: &mut TripleSink, node: &str, members: &[AtomicTerm]) {
    if members.is_empty() || members.iter().any(|term| term.as_iri().is_none()) {
        return;
    }
    let iris: Vec<String> = members
        .iter()
        .filter_map(|term| term.as_iri().map(str::to_owned))
        .collect();
    let list_head = emit_class_list(g, node, &iris);
    g.add_iri(node, RDF_TYPE, &owl("Class"));
    g.add_iri(node, &owl(restriction::ONE_OF_LOCAL), &list_head);
}

// --------------------------------------------------------------------------- //
// Datatype restrictions (owl:withRestrictions dataranges)
// --------------------------------------------------------------------------- //

/// A lifted datatype restriction (datarange) reconstructed from the flat `logic:` axiom
/// set for OWL re-emission.  The `node` is the skolem IRI (`logic:datarange/<hash>`) that
/// serves as the (non-blank) `rdfs:Datatype` node.
#[derive(Default)]
struct LiftedDatarange {
    on_datatype: Option<String>,
    /// `(full xsd: facet IRI, facet value)`, sorted + deduped for a deterministic list.
    facets: Vec<(String, AtomicTerm)>,
}

/// Collect every lifted datarange from the program's flat axioms, keyed by skolem node
/// IRI.  A node qualifies once it is typed `logic:Datarange` or carries `logic:onDatatype`;
/// its facets are the `xsd:`-namespaced [`restriction::FACET_LOCALS`] predicates on it.
/// Each facet list is sorted + deduped locally so the emitted `owl:withRestrictions` list
/// is deterministic (the enumeration collector's discipline), and the `BTreeMap` gives a
/// deterministic emission order.
fn collect_lifted_dataranges(program: &LogicProgram) -> BTreeMap<String, LiftedDatarange> {
    let datarange_ty = logic(restriction::DATARANGE_CLASS_LOCAL);
    let on_datatype = logic(restriction::ON_DATATYPE_LOCAL);
    let mut out: BTreeMap<String, LiftedDatarange> = BTreeMap::new();
    for axiom in &program.axioms {
        let pred = axiom.predicate.as_str();
        if pred == RDF_TYPE && axiom.obj.as_iri() == Some(datarange_ty.as_str()) {
            out.entry(axiom.subject.clone()).or_default();
        } else if pred == on_datatype {
            out.entry(axiom.subject.clone()).or_default().on_datatype =
                axiom.obj.as_iri().map(str::to_owned);
        } else if let Some(local) = pred.strip_prefix(XSD_NS)
            && restriction::FACET_LOCALS.contains(&local)
        {
            out.entry(axiom.subject.clone())
                .or_default()
                .facets
                .push((pred.to_owned(), axiom.obj.clone()));
        }
    }
    for dr in out.values_mut() {
        dr.facets.sort();
        dr.facets.dedup();
    }
    out
}

/// Emit the `owl:withRestrictions` `rdf:List` for one datarange: each list element is a
/// facet node carrying its single `<facetIRI> <value>` triple (never a bare member).  The
/// facet-cell and list-cell IRIs are minted deterministically off `base` (never blank
/// nodes), adapting [`emit_class_list`].  Returns the list head IRI.
fn emit_facet_list(g: &mut TripleSink, base: &str, facets: &[(String, AtomicTerm)]) -> String {
    let rdf_first = format!("{RDF_NS}first");
    let rdf_rest = format!("{RDF_NS}rest");
    let mut rest = format!("{RDF_NS}nil");
    for (i, (facet_iri, value)) in facets.iter().enumerate().rev() {
        // The facet node carries its one constraining-facet triple.
        let facet_node = format!("{base}/facet/{i:04}");
        g.add_atomic(&facet_node, facet_iri, value);
        // The list cell points at that facet node.
        let cell = format!("{base}/cell/{i:04}");
        g.add_iri(&cell, &rdf_first, &facet_node);
        g.add_iri(&cell, &rdf_rest, &rest);
        rest = cell;
    }
    rest
}

/// Emit the `rdfs:Datatype` graph for one lifted datarange (used by OWL 2 DL, which
/// expresses datatype facets): the skolem `node` typed `rdfs:Datatype` with its
/// `owl:onDatatype` base and an `owl:withRestrictions` list of facet cells.  A datarange
/// missing `onDatatype` or facets is structurally incomplete and is skipped (the lift
/// never produces one).
fn emit_datarange(g: &mut TripleSink, node: &str, dr: &LiftedDatarange) {
    let Some(on_datatype) = &dr.on_datatype else {
        return;
    };
    if dr.facets.is_empty() {
        return;
    }
    g.add_iri(node, RDF_TYPE, &rdfs("Datatype"));
    g.add_iri(node, &owl(restriction::ON_DATATYPE_LOCAL), on_datatype);
    let list_head = emit_facet_list(g, node, &dr.facets);
    g.add_iri(node, &owl(restriction::WITH_RESTRICTIONS_LOCAL), &list_head);
}

// --------------------------------------------------------------------------- //
// OWL 2 DL
// --------------------------------------------------------------------------- //

/// Project to OWL 2 DL Turtle (`generated/owl/gmeow-dl.ttl`).
pub fn project_owl_dl(
    program: &LogicProgram,
    loss: &mut crate::loss_ledger::LossLedger,
) -> Result<ProjectionResult, OverclaimError> {
    project_owl_dl_dataset(program, loss).map(RdfProjectionResult::into_text)
}

/// Produce the native OWL DL projection through the same lowering and loss ledger.
pub fn project_owl_dl_dataset(
    program: &LogicProgram,
    loss: &mut crate::loss_ledger::LossLedger,
) -> Result<RdfProjectionResult, OverclaimError> {
    let mut g = TripleSink::default();
    let mut actual_drops: Vec<String> = Vec::new();
    // Per-drop attribution to a DOCUMENTED gmeow: source term (by exact note string).
    let mut attributed: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    g.add_iri(
        &format!("{GMEOW_NS}owl/gmeow-dl"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    // Detect holon usage before the axiom loop so we can emit the surface once.
    let uses_holons = program
        .axioms
        .iter()
        .any(|a| a.predicate == logic("properPartOf"));
    if uses_holons {
        emit_holon_surface(&mut g);
    }

    // Re-emit class-expression restrictions as owl:Restriction graphs and anonymous
    // enumerations as owl:oneOf graphs (DL expresses both), and skip their flat
    // internals in the axiom loop.
    let restrictions = collect_lifted_restrictions(program);
    for (node, r) in &restrictions {
        emit_restriction(&mut g, node, r)?;
    }
    let enumerations = collect_lifted_enumerations(program);
    for (node, members) in &enumerations {
        emit_enumeration(&mut g, node, members);
    }
    let dataranges = collect_lifted_dataranges(program);
    for (node, dr) in &dataranges {
        emit_datarange(&mut g, node, dr);
    }

    for axiom in &program.axioms {
        let pred = &axiom.predicate;
        let obj = &axiom.obj;
        // Restriction / enumeration / datarange internals are emitted above.
        if restrictions.contains_key(&axiom.subject)
            || enumerations.contains_key(&axiom.subject)
            || dataranges.contains_key(&axiom.subject)
        {
            continue;
        }
        // Lifted RDFS/SKOS annotations are valid OWL annotation assertions — carry them
        // through the grounding view losslessly, with its native carrier tag.
        if axiom.node_kind == NodeKind::Annotation {
            g.add_annotation(axiom);
            continue;
        }
        if pred == RDF_TYPE {
            if let Some(gufo_type) = obj.as_iri().and_then(gufo_for_sort) {
                g.add_iri(&axiom.subject, RDF_TYPE, &gufo_type);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("Class"));
                continue;
            }
            if let Some(owl_char) = obj.as_iri().and_then(owl_for_char) {
                g.add_iri(&axiom.subject, RDF_TYPE, &owl_char);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("ObjectProperty"));
                continue;
            }
            // A canonical `logic:` bare typing / header marker (`logic:Class`,
            // `logic:ObjectProperty`, `logic:Ontology`, …) is projected exactly as its
            // `owl:` spelling was: OMITTED from the grounding view. A bare `owl:Class` /
            // `owl:Ontology` declaration never reached this projection (the frontend lifts
            // only the structural edges + the gUFO sort, and each class earns its
            // `owl:Class` from that sort), so the canonical marker is dropped in lockstep
            // rather than leaking through as a `logic:`-namespaced type.
            if obj
                .as_iri()
                .is_some_and(crate::typing_vocab::is_logic_typing_marker)
            {
                continue;
            }
            if !axiom.obj.is_literal() {
                g.add_atomic(&axiom.subject, RDF_TYPE, obj);
            }
            continue;
        }
        // properPartOf edges survive as object-property assertions.
        if pred == &logic("properPartOf") {
            if !axiom.obj.is_literal() {
                g.add_atomic(&axiom.subject, &logic("properPartOf"), obj);
            }
            continue;
        }
        if let Some(owl_pred) = owl_for_pred(pred) {
            g.add_atomic(&axiom.subject, &owl_pred, obj);
            continue;
        }
        if let Some(local) = pred.strip_prefix(LOGIC_NS) {
            let note = format!(
                "logic:{local} on <{}> has no OWL DL equivalent",
                axiom.subject
            );
            // The dropped assertion is ABOUT its subject; attribute to it when the subject is
            // a documented gmeow: term (a logic:-NS subject has no term page yet → whole-program).
            if let Some(src) = super::gmeow_term(&axiom.subject) {
                attributed.insert(note.clone(), src);
            }
            actual_drops.push(note);
        }
    }

    // Record holon-surface structural losses.
    if uses_holons {
        actual_drops.push(
            "logic:properPartOf strict-order characteristics (asymmetric + irreflexive) \
             cannot be declared in OWL 2 DL; only owl:TransitiveProperty is projected"
                .to_string(),
        );
        actual_drops.push(
            "the five-place logic:HolonicPosition relation is projected lossily as the \
             unary logic:Holon class; its positional arity (holarchy, context, interval, \
             path) is dropped"
                .to_string(),
        );
        actual_drops.push(
            "the logic:WeakSupplementation mereology axiom is not lowered to OWL and \
             stays in logic:"
                .to_string(),
        );
    }

    for rule in &program.rules {
        let head = &rule.head;
        if rule.body.len() == 1
            && owl_for_pred(&head.predicate).is_some()
            && !head.obj.is_literal()
            && owl_for_pred(&rule.body[0].predicate).is_some()
        {
            let owl_head_pred = owl_for_pred(&head.predicate).unwrap();
            g.add_atomic(&head.subject, &owl_head_pred, &head.obj);
            continue;
        }
        let note = format!(
            "rule head <{}> {} not expressible in OWL DL (body complexity)",
            rule.head.subject,
            super::python_repr(&rule.head.predicate)
        );
        // The dropped rule is ABOUT its head predicate; attribute to it when it is a
        // documented gmeow: property (so gmeow:knowsAbout / gmeow:findingCluster / … carry
        // this OWL-DL loss on their pages), else the drop stays whole-program.
        if let Some(src) = super::gmeow_term(&rule.head.predicate) {
            attributed.insert(note.clone(), src);
        }
        actual_drops.push(note);
    }

    // Re-emit each DL-projectable characteristic marker from its canonical carrier record. The
    // direct `?P rdf:type owl:{…}Property` marker is no longer an authored source (functionality /
    // inverse-functionality were retired to a carrier-only record; transitivity / symmetry were
    // flipped to their `logic:` spelling), so the ONLY thing carrying the characteristic into this
    // OWL grounding view is the `logic:PropertyCharacteristicAssertion` record. Projecting the
    // matching `owl:{…}Property` from it (a lossy down-projection of the canonical characteristic)
    // is exactly what each carrier record's prose promises, and mirrors the `owl_for_char`
    // emission for a direct marker (the OWL type + owl:ObjectProperty). The triple set is a set,
    // so this is idempotent with any surviving direct marker. Asymmetric / irreflexive sorts are
    // excluded by the projectable set — OWL 2 DL cannot carry them on a non-simple property.
    for (prop, sort_local) in dl_projectable_carrier_characteristics(program) {
        if let Some(owl_char) = owl_for_char(&logic(&sort_local)) {
            g.add_iri(&prop, RDF_TYPE, &owl_char);
            g.add_iri(&prop, RDF_TYPE, &owl("ObjectProperty"));
        }
    }

    project_formulas_owl_dl(&mut g, program);

    actual_drops.extend(contract_drop_notes(program, "OWL 2 DL", &|f| {
        recognize_covering(f).is_some()
    }));
    intern_rdf_drops(loss, "owl-dl", &actual_drops, &attributed);
    rdf_result("owl-dl", g, "OWL 2 DL", &actual_drops)
}

// --------------------------------------------------------------------------- //
// OWL 2 EL
// --------------------------------------------------------------------------- //

/// Project to OWL 2 EL Turtle (`generated/owl/gmeow-el.ttl`).
pub fn project_owl_el(
    program: &LogicProgram,
    loss: &mut crate::loss_ledger::LossLedger,
) -> Result<ProjectionResult, OverclaimError> {
    project_owl_el_dataset(program, loss).map(RdfProjectionResult::into_text)
}

/// Produce the native OWL EL projection through the same lowering and loss ledger.
pub fn project_owl_el_dataset(
    program: &LogicProgram,
    loss: &mut crate::loss_ledger::LossLedger,
) -> Result<RdfProjectionResult, OverclaimError> {
    let mut g = TripleSink::default();
    let mut actual_drops: Vec<String> = Vec::new();
    let mut attributed: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    g.add_iri(
        &format!("{GMEOW_NS}owl/gmeow-el"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    // Detect holon usage before the axiom loop so we can emit the surface once.
    let uses_holons = program
        .axioms
        .iter()
        .any(|a| a.predicate == logic("properPartOf"));
    if uses_holons {
        emit_holon_surface(&mut g);
    }

    // Re-emit only the EL-safe class-expression restrictions (someValuesFrom /
    // hasValue).  A restriction with any non-EL constraint (allValuesFrom, cardinality,
    // oneOf, …) drops WHOLE — its node and every `subClassOf` edge pointing at it — so
    // the EL surface never carries a dangling reference.  EL is SoundUnder, so dropping
    // needs no enum change, only an `actual_drops` note.
    let restrictions = collect_lifted_restrictions(program);
    let mut dropped_class_exprs: HashSet<String> = HashSet::new();
    for (node, r) in &restrictions {
        let well_formed = r.on_property.is_some() && !r.constraints.is_empty();
        let el_safe = well_formed
            && r.constraints
                .iter()
                .all(|(local, _)| is_el_safe_restriction_constraint(local));
        if el_safe {
            emit_restriction(&mut g, node, r)?;
        } else {
            dropped_class_exprs.insert(node.clone());
            if well_formed {
                let kinds = r
                    .constraints
                    .iter()
                    .map(|(local, _)| local.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                actual_drops.push(format!(
                    "owl:Restriction <{node}> ({kinds}) is not OWL 2 EL-safe; dropped"
                ));
            }
        }
    }
    // Enumerations are nominals (owl:oneOf) — not OWL 2 EL. Every anonymous enumeration
    // drops whole, along with the subClassOf/equivalentClass edges into it.
    let enumerations = collect_lifted_enumerations(program);
    for node in enumerations.keys() {
        dropped_class_exprs.insert(node.clone());
        actual_drops.push(format!(
            "owl:oneOf enumeration <{node}> is not OWL 2 EL-safe (nominals); dropped"
        ));
    }
    // Datatype facets are not OWL 2 EL either. Every datarange drops whole, along with the
    // subClassOf/equivalentClass edges into it.
    let dataranges = collect_lifted_dataranges(program);
    for node in dataranges.keys() {
        dropped_class_exprs.insert(node.clone());
        actual_drops.push(format!(
            "owl:withRestrictions datarange <{node}> is not OWL 2 EL-safe (datatype facets); \
             dropped"
        ));
    }

    for axiom in &program.axioms {
        let pred = &axiom.predicate;
        let obj = &axiom.obj;
        // Restriction / enumeration / datarange internals are emitted (or dropped) above.
        if restrictions.contains_key(&axiom.subject)
            || enumerations.contains_key(&axiom.subject)
            || dataranges.contains_key(&axiom.subject)
        {
            continue;
        }
        // Lifted RDFS/SKOS annotations are valid OWL annotation assertions — EL-safe as plain
        // annotation triples; carry them through losslessly with its native carrier tag.
        if axiom.node_kind == NodeKind::Annotation {
            g.add_annotation(axiom);
            continue;
        }
        // A subClassOf / equivalentClass edge into a dropped class expression must not
        // dangle in EL.
        if obj
            .as_iri()
            .is_some_and(|iri| dropped_class_exprs.contains(iri))
            && matches!(
                pred.strip_prefix(LOGIC_NS),
                Some("subClassOf" | "equivalentClass")
            )
        {
            continue;
        }
        if pred == RDF_TYPE {
            if let Some(gufo_type) = obj.as_iri().and_then(gufo_for_sort) {
                g.add_iri(&axiom.subject, RDF_TYPE, &gufo_type);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("Class"));
                continue;
            }
            if obj.as_iri().is_some_and(is_el_safe_char) {
                let owl_char = obj.as_iri().and_then(owl_for_char).unwrap();
                g.add_iri(&axiom.subject, RDF_TYPE, &owl_char);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("ObjectProperty"));
                continue;
            }
            if let Some(local) = obj.as_iri().and_then(|iri| iri.strip_prefix(LOGIC_NS))
                && obj.as_iri().and_then(owl_for_char).is_some()
            {
                actual_drops.push(format!(
                    "logic:{local} on <{}> is not EL-safe; dropped",
                    axiom.subject
                ));
                continue;
            }
            // A canonical `logic:` bare typing / header marker is OMITTED from the OWL 2 EL
            // grounding view exactly as its `owl:` spelling was (see the OWL-DL twin) — it is
            // dropped in lockstep rather than leaking through as a `logic:`-namespaced type.
            if obj
                .as_iri()
                .is_some_and(crate::typing_vocab::is_logic_typing_marker)
            {
                continue;
            }
            if !axiom.obj.is_literal() {
                g.add_atomic(&axiom.subject, RDF_TYPE, obj);
            }
            continue;
        }
        // properPartOf edges survive as object-property assertions (transitivity
        // is EL-safe; asymmetric/irreflexive are the documented loss).
        if pred == &logic("properPartOf") {
            if !axiom.obj.is_literal() {
                g.add_atomic(&axiom.subject, &logic("properPartOf"), obj);
            }
            continue;
        }
        if is_el_safe_pred(pred) {
            let owl_pred = owl_for_pred(pred).unwrap();
            g.add_atomic(&axiom.subject, &owl_pred, obj);
            continue;
        }
        if let Some(local) = pred.strip_prefix(LOGIC_NS) {
            if owl_for_pred(pred).is_some() {
                actual_drops.push(format!(
                    "logic:{local} on <{}> is not EL-safe; dropped",
                    axiom.subject
                ));
            } else {
                actual_drops.push(format!(
                    "logic:{local} on <{}> has no EL equivalent",
                    axiom.subject
                ));
            }
        }
    }

    // Re-emit the EL-safe characteristic markers from their canonical carrier records, mirroring
    // the OWL-DL twin but confined to the sorts OWL 2 EL admits — only `owl:TransitiveProperty`
    // (`is_el_safe_char`). Symmetry, functionality, and inverse-functionality are NOT EL-safe, so
    // their carriers are the documented EL loss and never projected here. Set-idempotent with any
    // surviving direct marker the axiom loop above already lowered.
    for (prop, sort_local) in dl_projectable_carrier_characteristics(program) {
        let sort_iri = logic(&sort_local);
        if is_el_safe_char(&sort_iri)
            && let Some(owl_char) = owl_for_char(&sort_iri)
        {
            g.add_iri(&prop, RDF_TYPE, &owl_char);
            g.add_iri(&prop, RDF_TYPE, &owl("ObjectProperty"));
        }
    }

    // Record holon-surface structural losses.
    if uses_holons {
        actual_drops.push(
            "logic:properPartOf strict-order characteristics (asymmetric + irreflexive) \
             cannot be declared in OWL 2 EL; only owl:TransitiveProperty is projected"
                .to_string(),
        );
        actual_drops.push(
            "the five-place logic:HolonicPosition relation is projected lossily as the \
             unary logic:Holon class; its positional arity (holarchy, context, interval, \
             path) is dropped"
                .to_string(),
        );
        actual_drops.push(
            "the logic:WeakSupplementation mereology axiom is not lowered to OWL 2 EL \
             and stays in logic:"
                .to_string(),
        );
    }

    for rule in &program.rules {
        let note = format!(
            "rule head <{}> dropped (EL has no rule surface)",
            rule.head.subject
        );
        // The dropped rule is ABOUT its head predicate; attribute to it when it is a
        // documented gmeow: property (structural, not scraped from the note).
        if let Some(src) = super::gmeow_term(&rule.head.predicate) {
            attributed.insert(note.clone(), src);
        }
        actual_drops.push(note);
    }

    actual_drops.extend(contract_drop_notes(program, "OWL 2 EL", &|_| false));
    intern_rdf_drops(loss, "owl-el", &actual_drops, &attributed);
    rdf_result("owl-el", g, "OWL 2 EL", &actual_drops)
}

// --------------------------------------------------------------------------- //
// gUFO bridge
// --------------------------------------------------------------------------- //

/// Project to gUFO bridge Turtle (`generated/foundation/gufo.ttl`).
pub fn project_gufo(
    program: &LogicProgram,
    loss: &mut crate::loss_ledger::LossLedger,
) -> Result<ProjectionResult, OverclaimError> {
    project_gufo_dataset(program, loss).map(RdfProjectionResult::into_text)
}

/// Produce the native GUFO projection through the same lowering and loss ledger.
pub fn project_gufo_dataset(
    program: &LogicProgram,
    loss: &mut crate::loss_ledger::LossLedger,
) -> Result<RdfProjectionResult, OverclaimError> {
    let mut g = TripleSink::default();
    let mut actual_drops: Vec<String> = Vec::new();
    let mut attributed: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    g.add_iri(
        &format!("{GMEOW_NS}foundation/gufo"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    for axiom in &program.axioms {
        let pred = &axiom.predicate;
        let obj = &axiom.obj;
        if pred == RDF_TYPE {
            if let Some(gufo_type) = obj.as_iri().and_then(gufo_for_sort) {
                g.add_iri(&axiom.subject, RDF_TYPE, &gufo_type);
                continue;
            }
            if let Some(local) = obj.as_iri().and_then(|iri| iri.strip_prefix(LOGIC_NS)) {
                let note = format!(
                    "rdf:type logic:{local} on <{}> has no gUFO equivalent",
                    axiom.subject
                );
                if let Some(src) = super::gmeow_term(&axiom.subject) {
                    attributed.insert(note.clone(), src);
                }
                actual_drops.push(note);
            }
            continue;
        }
        if pred == &logic("subClassOf") {
            if !axiom.obj.is_literal() {
                g.add_atomic(&axiom.subject, &rdfs("subClassOf"), obj);
            }
            continue;
        }
        if let Some(local) = pred.strip_prefix(LOGIC_NS) {
            let note = format!(
                "logic:{local} on <{}> has no gUFO bridge equivalent",
                axiom.subject
            );
            if let Some(src) = super::gmeow_term(&axiom.subject) {
                attributed.insert(note.clone(), src);
            }
            actual_drops.push(note);
        }
    }

    for rule in &program.rules {
        let note = format!(
            "rule head <{}> dropped (gUFO bridge has no rule surface)",
            rule.head.subject
        );
        if let Some(src) = super::gmeow_term(&rule.head.predicate) {
            attributed.insert(note.clone(), src);
        }
        actual_drops.push(note);
    }

    actual_drops.extend(contract_drop_notes(program, "the gUFO bridge", &|_| false));
    intern_rdf_drops(loss, "gufo", &actual_drops, &attributed);
    rdf_result("gufo", g, "gUFO bridge", &actual_drops)
}

// --------------------------------------------------------------------------- //
// Canonical RDF 1.2 (round-trippable)
// --------------------------------------------------------------------------- //

/// Project to canonical RDF 1.2 Turtle (`generated/logic/gmeow.logic.rdf12.ttl`).
pub fn project_canonical_rdf12(program: &LogicProgram) -> Result<ProjectionResult, OverclaimError> {
    project_canonical_rdf12_dataset(program).map(RdfProjectionResult::into_text)
}

/// Produce the native canonical RDF 1.2 projection without rendering Turtle.
pub fn project_canonical_rdf12_dataset(
    program: &LogicProgram,
) -> Result<RdfProjectionResult, OverclaimError> {
    if let crate::ir::PresentationProgramIr::Refused { detail, .. } = &program.presentations {
        return Err(OverclaimError(detail.clone()));
    }
    let mut g = TripleSink::default();

    g.add_iri(
        &format!("{GMEOW_NS}logic/gmeow.logic.rdf12"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    let rule_struct_preds = [
        logic("head"),
        logic("body"),
        logic("negatedBody"),
        logic("distinctBody"),
    ];

    // Axioms (skipping rule-structural predicates — re-emitted as Rule nodes).
    for axiom in &program.axioms {
        if rule_struct_preds.contains(&axiom.predicate) {
            continue;
        }
        // The complete annotation value, including its native carrier tag, is preserved.
        if axiom.node_kind == NodeKind::Annotation {
            g.add_annotation(axiom);
            continue;
        }
        // Canonical output keeps the authored value. Target-specific datatype conversions
        // belong at the corresponding projection boundary.
        let direct_predicate = axiom.predicate.starts_with(LOGIC_NS)
            || (axiom.predicate == RDF_TYPE
                && axiom
                    .obj
                    .as_iri()
                    .is_some_and(|iri| iri.starts_with(LOGIC_NS)));
        if !is_modal_or_scoped(axiom)
            && (!direct_predicate
                || axiom.subject.starts_with('?')
                || axiom.obj.as_variable().is_some())
        {
            emit_compact_formula(&mut g, axiom)?;
            continue;
        }
        if is_modal_or_scoped(axiom) {
            let key_hash = sha256_12(&axiom.content_key());
            let reifier = format!("{LOGIC_NS}reifier/{key_hash}");
            g.add_iri(&reifier, RDF_TYPE, &format!("{RDF_NS}Statement"));
            g.add_atomic(
                &reifier,
                &format!("{RDF_NS}subject"),
                &AtomicTerm::resource(&axiom.subject),
            );
            g.add_iri(&reifier, &format!("{RDF_NS}predicate"), &axiom.predicate);
            g.add_atomic(&reifier, &format!("{RDF_NS}object"), &axiom.obj);
            let scope = &axiom.scope;
            if let Some(sp) = &scope.standpoint {
                g.add_iri(&reifier, &logic("standpoint"), sp);
            }
            if let Some(t) = &scope.time {
                g.add_lit(&reifier, &logic("time"), RdfLiteral::simple(t));
            }
            if let Some(c) = scope.confidence {
                g.add_lit(&reifier, &logic("confidence"), decimal_literal(c));
            }
            if scope.modality != LogicModality::None {
                g.add_iri(
                    &reifier,
                    &logic("modality"),
                    &logic(scope.modality.as_str()),
                );
            }
            if let Some(p) = &scope.provenance {
                g.add_iri(&reifier, &logic("provenance"), p);
            }
            if let Some(m) = &scope.module {
                g.add_iri(&reifier, &logic("inModule"), m);
            }
        } else {
            g.add_atomic(&axiom.subject, &axiom.predicate, &axiom.obj);
        }
    }

    // Reasoning contracts. LOSSLESS projection: every contract — whether
    // it carries a preset or only direct facets — is emitted in full as DIRECT
    // facet properties on its subject node, so a re-parse through
    // `extract_contracts` reconstructs the byte-identical `ReasoningContract`
    // (same `sort_key()`).  The values are emitted as plain `logic:<Value>` IRIs;
    // the parser routes them by the FACET PROPERTY (not the value's rdf:type), so
    // the projection need not (and does not) re-emit each value's facet-class type.
    super::presentation::contracts(&mut g, program)?;

    // Rules as logic:Rule nodes with classic reification for head/body.
    for (idx, rule) in program.rules.iter().enumerate() {
        let rule_id = format!("_{:06}", idx + 1);
        let rule_node = format!("{LOGIC_NS}rule/{rule_id}");
        g.add_iri(&rule_node, RDF_TYPE, &logic("Rule"));

        // Head.
        let head = &rule.head;
        let head_node = format!("{LOGIC_NS}rule/{rule_id}/head");
        g.add_iri(&rule_node, &logic("head"), &head_node);
        g.add_iri(&head_node, RDF_TYPE, &format!("{RDF_NS}Statement"));
        add_reified_term(&mut g, &head_node, "subject", &head.subject, false);
        g.add_iri(&head_node, &format!("{RDF_NS}predicate"), &head.predicate);
        add_reified_object(&mut g, &head_node, &head.obj);

        // Body (positive then negated), each polarity sorted independently.
        let positive: Vec<_> = rule.body.iter().filter(|a| !a.negated).collect();
        let negated: Vec<_> = rule.body.iter().filter(|a| a.negated).collect();
        for (link_local, path_seg, atoms) in [
            ("body", "body", &positive),
            ("negatedBody", "negatedBody", &negated),
        ] {
            let mut sorted = atoms.clone();
            sorted.sort_by_cached_key(|a| a.sort_key());
            for (i, ba) in sorted.iter().enumerate() {
                let body_node = format!("{LOGIC_NS}rule/{rule_id}/{path_seg}/{i:04}");
                g.add_iri(&rule_node, &logic(link_local), &body_node);
                g.add_iri(&body_node, RDF_TYPE, &format!("{RDF_NS}Statement"));
                add_reified_term(&mut g, &body_node, "subject", &ba.subject, false);
                g.add_iri(&body_node, &format!("{RDF_NS}predicate"), &ba.predicate);
                add_reified_object(&mut g, &body_node, &ba.obj);
            }
        }

        // Inequality guards.
        for (i, (var_a, var_b)) in rule.distinct_pairs.iter().enumerate() {
            let distinct_node = format!("{LOGIC_NS}rule/{rule_id}/distinctBody/{i:04}");
            g.add_iri(&rule_node, &logic("distinctBody"), &distinct_node);
            g.add_iri(&distinct_node, RDF_TYPE, &format!("{RDF_NS}Statement"));
            g.add_lit(
                &distinct_node,
                &format!("{RDF_NS}subject"),
                RdfLiteral::simple(var_a),
            );
            g.add_lit(
                &distinct_node,
                &format!("{RDF_NS}object"),
                RdfLiteral::simple(var_b),
            );
        }

        // Rule scope.
        let scope = &rule.scope;
        if let Some(sp) = &scope.standpoint {
            g.add_iri(&rule_node, &logic("standpoint"), sp);
        }
        if let Some(t) = &scope.time {
            g.add_lit(&rule_node, &logic("time"), RdfLiteral::simple(t));
        }
        if let Some(c) = scope.confidence {
            g.add_lit(&rule_node, &logic("confidence"), decimal_literal(c));
        }
        if scope.modality != LogicModality::None {
            g.add_iri(
                &rule_node,
                &logic("modality"),
                &logic(scope.modality.as_str()),
            );
        }
        if let Some(p) = &scope.provenance {
            g.add_iri(&rule_node, &logic("provenance"), p);
        }
        if let Some(m) = &scope.module {
            g.add_iri(&rule_node, &logic("inModule"), m);
        }

        // Aggregation (reduce): the function, the aggregated variable, the result variable, and
        // the group keys, carried directly on the rule node (parallel to the scope properties).
        // Default-absent so a non-aggregating rule round-trips byte-identically.
        if let Some(agg) = &rule.aggregation {
            g.add_lit(
                &rule_node,
                &logic("aggregateFunction"),
                RdfLiteral::simple(&agg.function),
            );
            g.add_lit(
                &rule_node,
                &logic("aggregateVariable"),
                RdfLiteral::simple(&agg.aggregate_var),
            );
            g.add_lit(
                &rule_node,
                &logic("aggregateResult"),
                RdfLiteral::simple(&agg.result_var),
            );
            for key in &agg.group_keys {
                g.add_lit(&rule_node, &logic("groupKey"), RdfLiteral::simple(key));
            }
        }
    }

    // Full first-order formulas (the typed full-FOL core beyond the Horn fragment).
    // ExactPreservation: every formula is emitted in full as a reified logic:Formula
    // tree, so nothing is dropped and the canonical target stays lossless.
    for (idx, formula) in program.formulas.iter().enumerate() {
        let f_node = format!("{LOGIC_NS}formula/_{:06}", idx + 1);
        g.add_iri(
            &format!("{GMEOW_NS}logic/gmeow.logic.rdf12"),
            &logic("hasFormula"),
            &f_node,
        );
        emit_formula(&mut g, &f_node, formula);
    }

    // `program.reasoning_programs` (`logic:ReasoningProgram`) is DELIBERATELY not re-emitted
    // here, exactly like `program.constraints` / `program.validation_shapes` /
    // `program.path_shapes` / `program.correspondences` above: none of those collections is
    // re-serialized by this projection either. This projection carries only the content that
    // genuinely CHANGES shape between the source graph and the IR (formula-tree nodes
    // reassembled from term-carrier triples, rule/contract nodes reassembled from reified
    // structural triples) — content the frontend can only reconstruct from a byte-identical
    // re-emission. A `logic:ReasoningProgram`'s clause/query/probe formulas are ALREADY
    // ordinary reified `logic:Formula` trees authored verbatim in the source graph (the same
    // shape `emit_formula` would produce), so the authored triples themselves are the
    // canonical round-trip surface for reasoning-program content — the source graph is
    // preserved verbatim by the surrounding slice pipeline, not reconstructed through this
    // compiled-IR-only projection. Re-deriving a second reified copy here would source-fork
    // the same clause set under a fresh set of minted IRIs, which is the anti-pattern
    // `extract_formulas`'s "referenced" exclusion set (clause/programQuery/verdictProbe
    // roots) exists to prevent on the read side.
    //
    // Exact target: drops nothing, so it interns nothing into the loss store (its
    // read-back is empty and its report/ledger rows carry no `gmeow:lossyDrop`).
    super::presentation::emit(&mut g, &program.presentations)?;
    rdf_result("canonical-rdf12", g, "Canonical RDF 1.2", &[])
}

/// External predicates and logical variables need a declared formula envelope:
/// an arbitrary RDF domain triple alone does not declare a logic assertion.
fn emit_compact_formula(g: &mut TripleSink, axiom: &LogicAxiom) -> Result<(), OverclaimError> {
    let node = format!("{LOGIC_NS}axiom-formula/{}", sha256_12(&axiom.sort_key()));
    g.add_iri(&node, RDF_TYPE, &logic("Formula"));
    g.add_iri(&node, &logic("relation"), &axiom.predicate);
    let subject = AtomicTerm::resource(&axiom.subject);
    for (index, term) in [&subject, &axiom.obj].into_iter().enumerate() {
        let arg = format!("{node}/arg/{index:04}");
        g.add_iri(&node, &logic("argument"), &arg);
        emit_term_index(g, &arg, index);
        match term {
            AtomicTerm::Var(name) => g.add_lit(
                &arg,
                &logic("termVariable"),
                RdfLiteral::simple(name.trim_start_matches('?')),
            ),
            AtomicTerm::Iri(iri) => g.add_iri(&arg, &logic("termIri"), iri),
            AtomicTerm::Literal(value) => g.add_lit(&arg, &logic("termLiteral"), value.clone()),
            AtomicTerm::Blank(_) => {
                return Err(OverclaimError(format!(
                    "compact axiom {} requires an explicit existential formula for its blank term",
                    axiom.sort_key(),
                )));
            }
        }
    }
    Ok(())
}

/// Emit a [`Formula`] as a reified `logic:Formula` tree rooted at `node`. Deterministic
/// minted child IRIs (path segment + zero-padded index) make the serialization stable;
/// commutative connectives sort their operands by content key so emission order does not
/// depend on the stored vector order.
pub(crate) fn emit_formula(g: &mut TripleSink, node: &str, formula: &Formula) {
    g.add_iri(node, RDF_TYPE, &logic("Formula"));
    match formula {
        Formula::Atom { relation, args } => {
            if let Term::Iri(iri) = relation {
                g.add_iri(node, &logic("relation"), iri);
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_node = format!("{node}/arg/{i:04}");
                g.add_iri(node, &logic("argument"), &arg_node);
                emit_term_index(g, &arg_node, i);
                emit_term_value(g, &arg_node, arg);
            }
        }
        Formula::Not(f) => {
            let child = format!("{node}/not");
            g.add_iri(node, &logic("not"), &child);
            emit_formula(g, &child, f);
        }
        Formula::And(fs) => emit_operands(g, node, "and", fs.iter()),
        Formula::Or(fs) => emit_operands(g, node, "or", fs.iter()),
        Formula::Iff(a, b) => {
            emit_operands(g, node, "iff", [a.as_ref(), b.as_ref()].into_iter());
        }
        Formula::Implies(a, b) => {
            let an = format!("{node}/antecedent");
            let cn = format!("{node}/consequent");
            g.add_iri(node, &logic("antecedent"), &an);
            emit_formula(g, &an, a);
            g.add_iri(node, &logic("consequent"), &cn);
            emit_formula(g, &cn, b);
        }
        Formula::Forall { vars, body } => emit_quantifier(g, node, "forall", vars, body),
        Formula::Exists { vars, body } => emit_quantifier(g, node, "exists", vars, body),
    }
}

/// Emit the operands of a commutative connective (`and`/`or`/`iff`), sorted by content
/// key so the minted child IRIs are a deterministic function of the operand SET.
fn emit_operands<'a>(
    g: &mut TripleSink,
    node: &str,
    link: &str,
    operands: impl Iterator<Item = &'a Formula>,
) {
    let mut indexed: Vec<&Formula> = operands.collect();
    indexed.sort_by_cached_key(|f| f.content_key());
    for (i, f) in indexed.iter().enumerate() {
        let child = format!("{node}/{link}/{i:04}");
        g.add_iri(node, &logic(link), &child);
        emit_formula(g, &child, f);
    }
}

/// Emit a quantifier node: its body plus an ordered list of bound-variable carriers.
fn emit_quantifier(g: &mut TripleSink, node: &str, link: &str, vars: &[String], body: &Formula) {
    let body_node = format!("{node}/body");
    g.add_iri(node, &logic(link), &body_node);
    emit_formula(g, &body_node, body);
    for (i, v) in vars.iter().enumerate() {
        let var_node = format!("{node}/var/{i:04}");
        g.add_iri(node, &logic("quantifiedVariable"), &var_node);
        emit_term_index(g, &var_node, i);
        g.add_lit(&var_node, &logic("termVariable"), RdfLiteral::simple(v));
    }
}

/// Emit the zero-based `logic:termIndex` ordinal on a term-carrier node.
fn emit_term_index(g: &mut TripleSink, node: &str, index: usize) {
    g.add_lit(
        node,
        &logic("termIndex"),
        RdfLiteral::typed(index.to_string(), format!("{XSD_NS}nonNegativeInteger")),
    );
}

/// Emit the single term-value property of a term-carrier node, by term kind.
fn emit_term_value(g: &mut TripleSink, node: &str, term: &Term) {
    match term {
        Term::Iri(iri) => g.add_iri(node, &logic("termIri"), iri),
        Term::Var(name) => g.add_lit(node, &logic("termVariable"), RdfLiteral::simple(name)),
        Term::Literal(literal) => {
            g.add_lit(node, &logic("termLiteral"), literal.clone());
        }
        Term::SequenceMarker(name) => {
            g.add_lit(node, &logic("termSequenceMarker"), RdfLiteral::simple(name))
        }
        Term::App { symbol, args } => {
            // A compound function term is carried by a logic:FunctionTerm node the carrier
            // points at via logic:termApplication: one reified logic:functionSymbol plus its
            // ordered logic:argument carriers, emitted with the same index+value machinery a
            // predication's arguments use — so `parse_function_term` reconstructs it and a
            // nested application (`cons(H, cons(1, nil))`) round-trips losslessly.
            let ft = format!("{node}/app");
            g.add_iri(node, &logic("termApplication"), &ft);
            g.add_iri(&ft, RDF_TYPE, &logic("FunctionTerm"));
            g.add_iri(&ft, &logic("functionSymbol"), symbol);
            for (i, arg) in args.iter().enumerate() {
                let arg_node = format!("{ft}/arg/{i:04}");
                g.add_iri(&ft, &logic("argument"), &arg_node);
                emit_term_index(g, &arg_node, i);
                emit_term_value(g, &arg_node, arg);
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// Class-covering formulas → OWL union / disjoint-union
// --------------------------------------------------------------------------- //

/// A recognized class-covering: `whole ⊑ ⊔ members`, lifted from a covering-shaped
/// `logic:Formula` `∀v. whole(v) → (m₁(v) ∨ … ∨ mₙ(v))`. `members` is sorted and
/// deduped so the emitted OWL list is a deterministic function of the covered SET
/// (independent of authored disjunct order — `logic:or` is commutative anyway).
struct Covering {
    whole: String,
    members: Vec<String>,
}

/// If `f` is a unary class-membership predication `Class(Var(v))`, return `Class`'s IRI.
/// This is the atom shape both the antecedent `whole(v)` and each disjunct `mᵢ(v)` take.
fn unary_membership(f: &Formula, v: &str) -> Option<String> {
    let Formula::Atom { relation, args } = f else {
        return None;
    };
    let Term::Iri(cls) = relation else {
        return None;
    };
    match args.as_slice() {
        [Term::Var(arg)] if arg == v => Some(cls.clone()),
        _ => None,
    }
}

/// Recognize the covering shape `∀v. whole(v) → (m₁(v) ∨ … ∨ mₙ(v))` (n ≥ 2, all
/// disjuncts sharing the single bound variable `v`). Returns `None` for any other
/// formula — the recognizer is exact, so a non-covering formula is disclosed as
/// residue rather than silently mis-emitted.
fn recognize_covering(formula: &Formula) -> Option<Covering> {
    let Formula::Forall { vars, body } = formula else {
        return None;
    };
    let [v] = vars.as_slice() else {
        return None;
    };
    let Formula::Implies(antecedent, consequent) = body.as_ref() else {
        return None;
    };
    let whole = unary_membership(antecedent, v)?;
    let Formula::Or(disjuncts) = consequent.as_ref() else {
        return None;
    };
    let mut members = disjuncts
        .iter()
        .map(|d| unary_membership(d, v))
        .collect::<Option<Vec<String>>>()?;
    if members.len() < 2 {
        return None;
    }
    members.sort();
    members.dedup();
    Some(Covering { whole, members })
}

/// The byte-stable content key of a covering — the covered class plus its sorted
/// member set. Feeds the minted list/union-class IRIs so the OWL serialization is
/// identical across regenerate runs.
fn covering_key(cov: &Covering) -> String {
    format!("{}|{}", cov.whole, cov.members.join(","))
}

/// Pre-index every asserted disjointness (`logic:disjointWith` or `owl:disjointWith`,
/// class-valued, either direction) as an unordered `(a, b)` pair with `a < b`. Built once
/// per projection over all `axioms` so a covering's pairwise-disjoint test is O(M²) hash
/// lookups instead of an O(M²·A) rescan of every axiom.
fn disjoint_pair_index(axioms: &[LogicAxiom]) -> HashSet<(&str, &str)> {
    let disjoint = logic("disjointWith");
    let disjoint_owl = owl("disjointWith");
    axioms
        .iter()
        .filter_map(|ax| {
            if ax.predicate != disjoint && ax.predicate != disjoint_owl {
                return None;
            }
            let object = ax.obj.as_iri()?;
            Some(if ax.subject.as_str() < object {
                (ax.subject.as_str(), object)
            } else {
                (object, ax.subject.as_str())
            })
        })
        .collect()
}

/// `true` iff every unordered pair of `members` is asserted disjoint, looked up in the
/// pre-built `disjoint_pairs` index (canonicalized `a < b`). A fully-disjoint covering
/// lowers to `owl:disjointUnionOf` (a partition); otherwise to a plain `owl:unionOf`
/// cover, so a deliberate overlap is never over-claimed as a partition.
fn all_pairwise_disjoint(members: &[String], disjoint_pairs: &HashSet<(&str, &str)>) -> bool {
    (0..members.len())
        .flat_map(|i| ((i + 1)..members.len()).map(move |j| (i, j)))
        .all(|(i, j)| {
            let (a, b) = (members[i].as_str(), members[j].as_str());
            let key = if a < b { (a, b) } else { (b, a) };
            disjoint_pairs.contains(&key)
        })
}

/// Emit an `rdf:List` of member class IRIs (built tail-to-head so each cell's
/// `rdf:rest` points at the already-emitted remainder) and return the head node —
/// a minted, content-derived IRI (never a blank node, so the deterministic-IRI codec
/// keeps the list byte-stable). An empty member set yields `rdf:nil`.
fn emit_class_list(g: &mut TripleSink, base: &str, members: &[String]) -> String {
    // The rdf:first / rdf:rest predicate IRIs are loop-invariant — format them once.
    let rdf_first = format!("{RDF_NS}first");
    let rdf_rest = format!("{RDF_NS}rest");
    let mut rest = format!("{RDF_NS}nil");
    for (i, member) in members.iter().enumerate().rev() {
        let cell = format!("{base}/cell/{i:04}");
        g.add_iri(&cell, &rdf_first, member);
        g.add_iri(&cell, &rdf_rest, &rest);
        rest = cell;
    }
    rest
}

/// Lower a recognized covering to OWL 2 DL. A fully-disjoint covering becomes
/// `whole owl:disjointUnionOf ( … )` (a partition); a covering with a deliberate
/// overlap becomes `whole rdfs:subClassOf [ owl:Class ; owl:unionOf ( … ) ]` — the
/// covering (exhaustiveness) without the disjointness the members do not all carry.
/// Both are faithful OWL 2 DL, so a recognized covering is NOT a lossy drop.
fn emit_covering_owl_dl(g: &mut TripleSink, cov: &Covering, disjoint: bool) {
    let base = format!("{GMEOW_NS}owl/covering/{}", sha256_12(&covering_key(cov)));
    let list_head = emit_class_list(g, &base, &cov.members);
    if disjoint {
        g.add_iri(&cov.whole, &owl("disjointUnionOf"), &list_head);
    } else {
        let union_cls = format!("{base}/union");
        g.add_iri(&union_cls, RDF_TYPE, &owl("Class"));
        g.add_iri(&union_cls, &owl("unionOf"), &list_head);
        g.add_iri(&cov.whole, &rdfs("subClassOf"), &union_cls);
    }
}

/// Project every recognized class-covering `logic:Formula` into OWL 2 DL: a covering
/// lowers to `owl:unionOf` / `owl:disjointUnionOf` (faithful — NOT a lossy drop). A
/// non-covering formula is disclosed as residue by `formula_residue_notes` (via
/// `contract_drop_notes`), which is told coverings ARE representable here so an emitted
/// covering is never also double-counted as a drop.
fn project_formulas_owl_dl(g: &mut TripleSink, program: &LogicProgram) {
    // Index the disjoint pairs once for the whole program, not once per covering.
    let disjoint_pairs = disjoint_pair_index(&program.axioms);
    for formula in &program.formulas {
        if let Some(cov) = recognize_covering(formula) {
            let disjoint = all_pairwise_disjoint(&cov.members, &disjoint_pairs);
            emit_covering_owl_dl(g, &cov, disjoint);
        }
    }
}

/// Project a single [`ReasoningContract`] losslessly as DIRECT facet properties.
///
/// The subject node is the preset's IRI when the contract carries a preset (typed
/// `logic:ReasoningPreset`), else a deterministic, content-free contract node
/// `logic:contract/_NNNNNN` (typed `logic:ReasoningContract`) minted from the
/// contract's canonical position — exactly the `rule/_NNNNNN` scheme used for
/// anonymous rule nodes above.  Because the program's `contracts` vector is
/// canonically sorted, `idx` is a stable function of the program content.
///
/// Every facet selection is emitted as the SAME direct facet property the
/// front-end (`extract_contracts`) reads, so the projection round-trips:
/// single-valued → one `logic:<facetProp> logic:<Value>` triple; set-valued →
/// one triple per member; closure map → a `logic:ClosureEntry` node per entry
/// (`logic:closureKey` string + `logic:closureValue logic:<Value>`) plus the
/// `logic:defaultClosure logic:<Value>` default; complexity →
/// `logic:complexityClass`.
pub(super) fn project_contract(
    g: &mut TripleSink,
    idx: usize,
    contract: &super::super::ir::ReasoningContract,
) {
    let node = match contract.preset {
        Some(preset) => {
            let pid = logic(preset.as_str());
            g.add_iri(&pid, RDF_TYPE, &logic("ReasoningPreset"));
            pid
        }
        None => {
            let node = format!("{LOGIC_NS}contract/_{:06}", idx + 1);
            g.add_iri(&node, RDF_TYPE, &logic("ReasoningContract"));
            node
        }
    };

    project_named_contract(g, &node, contract);
}

pub(super) fn project_named_contract(
    g: &mut TripleSink,
    node: &str,
    contract: &super::super::ir::ReasoningContract,
) {
    g.add_iri(
        node,
        RDF_TYPE,
        &logic(if contract.preset.is_some() {
            "ReasoningPreset"
        } else {
            "ReasoningContract"
        }),
    );
    // Single-valued facets: (property local name, value).
    let singletons: [(&str, &Option<String>); 10] = [
        ("formulaFragment", &contract.formula_fragment),
        ("modelSemantics", &contract.model_semantics),
        ("truthAlgebra", &contract.truth_algebra),
        ("admissibleValuation", &contract.admissible_valuation),
        ("designatedValues", &contract.designated_values),
        ("evolution", &contract.evolution),
        ("argumentation", &contract.argumentation),
        ("revision", &contract.revision),
        ("equalityPolicy", &contract.equality_policy),
        ("defaultClosure", &contract.default_closure),
    ];
    for (prop, value) in singletons {
        if let Some(v) = value {
            g.add_iri(node, &logic(prop), &facet_value_iri(v));
        }
    }

    // Set-valued facets: (property local name, sorted member set).
    let sets: [(&str, &std::collections::BTreeSet<String>); 5] = [
        ("negationOperator", &contract.negation_operators),
        ("contextAxis", &contract.context_axes),
        ("uncertaintyMeasure", &contract.uncertainty_measures),
        ("resourcePolicy", &contract.resource_policies),
        ("projectionTarget", &contract.projection_targets),
    ];
    for (prop, members) in sets {
        for member in members {
            g.add_iri(node, &logic(prop), &facet_value_iri(member));
        }
    }

    // Closure map: one logic:ClosureEntry node per binding (BTreeMap ⇒ sorted),
    // each carrying its key string + closure value individual.
    for (i, (key, val)) in contract.closure_entries.iter().enumerate() {
        let entry = format!("{node}/closureEntry/{i:04}");
        g.add_iri(node, &logic("closureEntry"), &entry);
        g.add_iri(&entry, RDF_TYPE, &logic("ClosureEntry"));
        g.add_lit(&entry, &logic("closureKey"), RdfLiteral::simple(key));
        g.add_iri(&entry, &logic("closureValue"), &facet_value_iri(val));
    }

    // Carried decidability data.
    if let Some(c) = &contract.complexity {
        g.add_lit(
            node,
            &logic("complexityClass"),
            RdfLiteral::simple(c.label()),
        );
    }
}

/// Add a reified `rdf:subject`/`rdf:object` term: a `?`-variable is emitted as a
/// plain Literal (to round-trip), else IRI / literal per `is_literal`.
fn add_reified_term(g: &mut TripleSink, node: &str, role: &str, value: &str, is_literal: bool) {
    let pred = format!("{RDF_NS}{role}");
    // A `?`-variable round-trips as a plain Literal, exactly like an actual
    // literal object; only proper IRIs are emitted as IRIs.
    if value.starts_with('?') || is_literal {
        g.add_lit(node, &pred, RdfLiteral::simple(value));
    } else {
        g.add_iri(node, &pred, value);
    }
}

/// A plain question-mark literal needs an explicit literal carrier in a rule,
/// since a direct plain string in that authored position denotes a variable.
fn add_reified_object(g: &mut TripleSink, node: &str, object: &AtomicTerm) {
    if let AtomicTerm::Literal(literal) = object
        && literal.datatype_iri() == "http://www.w3.org/2001/XMLSchema#string"
        && literal.language.is_none()
        && literal.direction.is_none()
        && literal.lexical_form.starts_with('?')
    {
        let carrier = format!("{node}/object-literal");
        g.add_iri(node, &format!("{RDF_NS}object"), &carrier);
        g.add_lit(&carrier, &logic("termLiteral"), literal.clone());
    } else {
        g.add_atomic(node, &format!("{RDF_NS}object"), object);
    }
}

/// `Literal(value, datatype=xsd:decimal)` with a Python-`str(float)`-style lexical.
fn decimal_literal(value: f64) -> RdfLiteral {
    RdfLiteral::typed(format_decimal(value), format!("{XSD_NS}decimal"))
}

/// Format an f64 the way the lexical of an xsd:decimal literal reads (`0.9`),
/// matching the Python/rdflib decimal serialization for the corpus values.
fn format_decimal(value: f64) -> String {
    let s = format!("{value}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}
