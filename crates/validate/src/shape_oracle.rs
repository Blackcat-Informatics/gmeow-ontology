// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shape-equivalence **oracle** — a comparison-only inverse of the SHACL Core
//! emitter plus the non-vacuous fixture cross-check.
//!
//! This module answers one question about a shape deletion / promotion: does a
//! *projected* [`ValidationShapeIr`] enforce exactly what a *legacy* hand-authored
//! SHACL shape did? It does so on three planes:
//!
//! * **Part A** ([`read_shacl_shape`]) inverts [`gmeow_logic_compile::projections::shapes`]'s
//!   SHACL Core emitter over the covered fragment, lifting one authored `sh:NodeShape`
//!   back into a [`ValidationShapeIr`] PLUS an explicit **residue list** of the genuinely
//!   uncovered constructs it also carried ([`ShapeRead`]). A real legacy shape routinely
//!   MIXES covered constraints (cardinality / `sh:class` / `sh:datatype` / …) with
//!   uncovered structure (`sh:or`, `sh:sparql`, …) and presentation (`sh:message`,
//!   `sh:severity`, `rdfs:label`) on the same node; the reader compares the covered
//!   fragment and flags the residue rather than aborting the whole parse. Nothing is
//!   dropped in silence: presentation/annotation is absorbed or skipped, every other
//!   uncovered predicate lands in [`ShapeRead::unsupported`], and only genuine
//!   malformation is an `Err`.
//! * **Part B** ([`oracle`]) decides equivalence through the Task-1 subsumption lattice
//!   ([`gmeow_logic_compile::projections::subsumption`]): `≡` over the covered `ir`, the
//!   Galois soundness direction (`projected ⊒ legacy`), the residue normal form, and the
//!   `residue_bearing` flag — a residue-bearing legacy class is NOT deletable on the
//!   covered match alone.
//! * **Part C** ([`cross_check`]) runs BOTH shape graphs as real SHACL validators over a
//!   data graph plus one discriminating near-miss per component, asserting identical
//!   finding sets and HARD-FAILING on vacuity (a pass that exercised nothing is not a
//!   pass).
//!
//! # Comparison-only (Principle 4)
//!
//! [`read_shacl_shape`] parses SHACL back INTO an in-memory `ValidationShapeIr` purely
//! so the oracle can compare it. It lives in `gmeow-validate`, is **never** re-exported
//! from `gmeow-logic-compile`, and **no function in this module writes to `slices/**` or
//! the `logic:` canon** — the canonical authoring ground is the `logic:` core, and SHACL
//! is a lossy projection of it, never a parse-back source (Principle 4). The oracle only
//! reads bytes and returns verdicts.

use std::collections::BTreeSet;

use gmeow_errors::Diag;
use gmeow_logic_compile::ir::{
    ConstraintComponent, ConstraintProvenance, PropertyConstraintIr, ShaclNodeKind, ShaclSeverity,
    ShapeTarget, ShapeValue, ValidationShapeIr,
};
use gmeow_logic_compile::projections::subsumption;
use purrdf::{
    DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, TermId, TermRef, TermValue,
    parse_dataset,
};

// ── Vocabulary the reader inverts ──────────────────────────────────────────────

/// The SHACL namespace; every covered property/target predicate is `SH` + a local name.
const SH: &str = "http://www.w3.org/ns/shacl#";
/// `sh:NodeShape` — the class every readable shape must be typed to.
const SH_NODESHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
/// `sh:SPARQLTarget` — the class a value-keyed `sh:target` blank node is typed to.
const SH_SPARQLTARGET: &str = "http://www.w3.org/ns/shacl#SPARQLTarget";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdf:first` / `rdf:rest` / `rdf:nil` — the RDF-list spine of `sh:in` / `sh:languageIn`.
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
/// `rdfs:label` — a presentation predicate the reader skips silently.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `rdfs:isDefinedBy` — an ontology-provenance annotation (which slice defines the shape); it
/// carries no enforcement, so the reader absorbs it exactly like `rdfs:label`.
const RDFS_ISDEFINEDBY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
/// `rdfs:comment` — a documentation annotation the reader absorbs.
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
/// Typed conformance-failure metadata. It does not alter the accepted graph, but the reader
/// preserves it so migration tooling can verify that a projected replacement raises the same
/// failure class and can reject ambiguous distinct declarations.
const GMEOW_ENFORCES_FAILURE_CLASS: &str =
    "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";
/// The SKOS namespace — every `skos:*` annotation is skipped silently.
const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
/// `xsd:string` / `xsd:integer` / `xsd:dateTime` — the datatype IRIs the reader keys on.
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// The SHACL local name of an IRI (`http://www.w3.org/ns/shacl#class` → `class`), or
/// `None` for any IRI outside the SHACL namespace.
fn shacl_local(iri: &str) -> Option<&str> {
    iri.strip_prefix(SH)
}

/// Build a `validate.parse` diagnostic from a preserved condition message. Every hard
/// malformation the reader surfaces routes through this so the oracle reports on the shared
/// diagnostic substrate rather than a bare string.
fn parse_err(detail: String) -> Diag {
    Diag::of_kind(crate::error::Parse { detail })
}

/// A presentation / annotation predicate the reader ABSORBS or SKIPS — never routes to the
/// residue list, because it carries no enforcement (it is projected out of the enforcement
/// key). `sh:message` / `sh:severity` are captured on a property shape and skipped at the
/// node / inner level; the rest are pure annotation everywhere.
fn is_presentation(pred: &str) -> bool {
    pred == RDFS_LABEL
        || pred == RDFS_ISDEFINEDBY
        || pred == RDFS_COMMENT
        || pred.starts_with(SKOS)
        || matches!(
            shacl_local(pred),
            Some(
                "name" | "description" | "order" | "group" | "deactivated" | "message" | "severity"
            )
        )
}

/// Route a predicate the caller did not handle: a presentation/annotation predicate is
/// skipped, everything else (an uncovered structural construct or a truly opaque
/// predicate) is recorded in the residue list — never dropped in silence.
fn route_unhandled(pred: String, unsupported: &mut Vec<String>) {
    if !is_presentation(&pred) {
        unsupported.push(pred);
    }
}

// ── Part A: SHACL-Turtle → ValidationShapeIr reader (comparison-only) ───────────

/// The result of reading one legacy `sh:NodeShape`: the covered fragment lifted into a
/// [`ValidationShapeIr`], plus the sorted, de-duplicated list of genuinely-uncovered
/// constructs the same node carried (`sh:or`, `sh:sparql`, `sh:targetNode`, …). A
/// non-empty `unsupported` means the legacy class is **residue-bearing**: its covered
/// fragment can be compared, but the residue must be authored as a `logic:` constraint
/// before the class is deletable.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeRead {
    /// The covered fragment as an enforcement-comparable IR.
    pub ir: ValidationShapeIr,
    /// The genuinely-uncovered constructs found on the shape (full predicate IRIs),
    /// sorted and de-duplicated. Empty ⇒ the shape is fully covered.
    pub unsupported: Vec<String>,
}

/// Every `(predicate IRI, object id)` on `subject` in the default or any named graph.
/// Predicates in RDF are always IRIs, so a non-IRI predicate (impossible in a
/// well-formed graph) is skipped rather than surfaced.
fn quads_of(ds: &RdfDataset, subject: TermId) -> Vec<(String, TermId)> {
    ds.quads_for_pattern(Some(subject), None, None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.p) {
            TermRef::Iri(p) => Some((p.to_owned(), q.o)),
            _ => None,
        })
        .collect()
}

/// Resolve `id` to an IRI string, hard-failing (`Err`) on any non-IRI term.
fn obj_iri(ds: &RdfDataset, id: TermId, ctx: &str) -> gmeow_errors::Result<String> {
    match ds.resolve(id) {
        TermRef::Iri(s) => Ok(s.to_owned()),
        other => Err(parse_err(format!(
            "{ctx}: expected an IRI object, found {other:?}"
        ))),
    }
}

/// Resolve `id` to a `u32` from an integer literal, hard-failing on a non-integer term.
fn obj_u32(ds: &RdfDataset, id: TermId, ctx: &str) -> gmeow_errors::Result<u32> {
    match ds.resolve(id) {
        TermRef::Literal { lexical, .. } => lexical.parse::<u32>().map_err(|e| {
            parse_err(format!(
                "{ctx}: expected a non-negative integer, found '{lexical}': {e}"
            ))
        }),
        other => Err(parse_err(format!(
            "{ctx}: expected an integer literal, found {other:?}"
        ))),
    }
}

/// Resolve `id` to a boolean from an `xsd:boolean` literal (`true`/`false`).
fn obj_bool(ds: &RdfDataset, id: TermId, ctx: &str) -> gmeow_errors::Result<bool> {
    match ds.resolve(id) {
        TermRef::Literal {
            lexical: "true", ..
        } => Ok(true),
        TermRef::Literal {
            lexical: "false", ..
        } => Ok(false),
        other => Err(parse_err(format!(
            "{ctx}: expected a boolean literal, found {other:?}"
        ))),
    }
}

/// The literal lexical form at `id`, hard-failing on a non-literal term.
fn obj_lexical(ds: &RdfDataset, id: TermId, ctx: &str) -> gmeow_errors::Result<String> {
    match ds.resolve(id) {
        TermRef::Literal { lexical, .. } => Ok(lexical.to_owned()),
        other => Err(parse_err(format!(
            "{ctx}: expected a string literal, found {other:?}"
        ))),
    }
}

/// A `sh:minInclusive` / `sh:maxInclusive` / `sh:minExclusive` / `sh:maxExclusive` bound:
/// a numeric value (an `xsd:integer`/`xsd:decimal`/… literal) or an `xsd:dateTime` lexical.
enum Facet {
    /// A numeric bound → [`ConstraintComponent::NumericRange`].
    Numeric(f64),
    /// An `xsd:dateTime` bound → [`ConstraintComponent::DateTimeRange`].
    DateTime(String),
}

/// Classify a numeric/datetime range-facet object.
fn parse_facet(ds: &RdfDataset, id: TermId, ctx: &str) -> gmeow_errors::Result<Facet> {
    match ds.resolve(id) {
        TermRef::Literal {
            lexical, datatype, ..
        } => {
            let dt = obj_iri(ds, datatype, ctx)?;
            if dt == XSD_DATETIME {
                return Ok(Facet::DateTime(lexical.to_owned()));
            }
            lexical.parse::<f64>().map(Facet::Numeric).map_err(|e| {
                parse_err(format!(
                    "{ctx}: range facet has a non-numeric, non-dateTime literal '{lexical}': {e}"
                ))
            })
        }
        other => Err(parse_err(format!(
            "{ctx}: a range facet must be a literal, found {other:?}"
        ))),
    }
}

/// Walk an RDF list from `head`, returning its member term ids in order. An empty
/// (`rdf:nil`) list yields the empty vector; a malformed node hard-fails.
fn parse_rdf_list(ds: &RdfDataset, head: TermId, ctx: &str) -> gmeow_errors::Result<Vec<TermId>> {
    let mut out = Vec::new();
    let mut cur = head;
    // Cyclic / adversarial lists are bounded — a real `sh:in` is tiny.
    for _ in 0..1_000_000 {
        if let TermRef::Iri(s) = ds.resolve(cur)
            && s == RDF_NIL
        {
            return Ok(out);
        }
        let node = quads_of(ds, cur);
        let first = node
            .iter()
            .find(|(p, _)| p == RDF_FIRST)
            .ok_or_else(|| {
                parse_err(format!(
                    "{ctx}: malformed RDF list (a node has no rdf:first)"
                ))
            })?
            .1;
        let rest = node
            .iter()
            .find(|(p, _)| p == RDF_REST)
            .ok_or_else(|| {
                parse_err(format!(
                    "{ctx}: malformed RDF list (a node has no rdf:rest)"
                ))
            })?
            .1;
        out.push(first);
        cur = rest;
    }
    Err(parse_err(format!("{ctx}: RDF list is too long or cyclic")))
}

/// A single value term (`sh:in` member or `sh:hasValue`) as a [`ShapeValue`]. A plain
/// `xsd:string` literal and a language-tagged literal both fold to `datatype: None` so the
/// read-back mirrors the emitter's [`ShapeValue`] construction.
fn shape_value(ds: &RdfDataset, id: TermId, ctx: &str) -> gmeow_errors::Result<ShapeValue> {
    match ds.resolve(id) {
        TermRef::Iri(s) => Ok(ShapeValue::Iri(s.to_owned())),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => {
            if let Some(lang) = language {
                return Ok(ShapeValue::Literal {
                    lexical: lexical.to_owned(),
                    datatype: None,
                    lang: Some(lang.to_owned()),
                });
            }
            let dt = obj_iri(ds, datatype, ctx)?;
            if dt == XSD_STRING {
                Ok(ShapeValue::Literal {
                    lexical: lexical.to_owned(),
                    datatype: None,
                    lang: None,
                })
            } else {
                Ok(ShapeValue::Literal {
                    lexical: lexical.to_owned(),
                    datatype: Some(dt),
                    lang: None,
                })
            }
        }
        other => Err(parse_err(format!(
            "{ctx}: a value term must be an IRI or a literal, found {other:?}"
        ))),
    }
}

/// Map a `sh:nodeKind` object IRI (`sh:IRI`, `sh:BlankNodeOrIRI`, …) to a [`ShaclNodeKind`].
fn parse_node_kind(iri: &str, ctx: &str) -> gmeow_errors::Result<ShaclNodeKind> {
    match shacl_local(iri) {
        Some("IRI") => Ok(ShaclNodeKind::Iri),
        Some("Literal") => Ok(ShaclNodeKind::Literal),
        Some("BlankNode") => Ok(ShaclNodeKind::BlankNode),
        Some("IRIOrLiteral") => Ok(ShaclNodeKind::IriOrLiteral),
        Some("BlankNodeOrIRI") => Ok(ShaclNodeKind::BlankNodeOrIri),
        Some("BlankNodeOrLiteral") => Ok(ShaclNodeKind::BlankNodeOrLiteral),
        _ => Err(parse_err(format!(
            "{ctx}: unsupported sh:nodeKind object <{iri}>"
        ))),
    }
}

/// Map a `sh:severity` object IRI (`sh:Violation`/`sh:Warning`/`sh:Info`) to a
/// [`ShaclSeverity`], or `None` for an unrecognized severity (absorbed as no-op — severity
/// is projected out of the enforcement key, so an odd value never changes equivalence).
fn parse_severity(iri: &str) -> Option<ShaclSeverity> {
    match shacl_local(iri) {
        Some("Violation") => Some(ShaclSeverity::Violation),
        Some("Warning") => Some(ShaclSeverity::Warning),
        Some("Info") => Some(ShaclSeverity::Info),
        _ => None,
    }
}

/// A cross-predicate accumulator for the value-level component fragment shared by a
/// property shape, a node shape, and an inner (`sh:not` / `sh:qualifiedValueShape`)
/// blank node. `sh:pattern`+`sh:flags`, the numeric/datetime range facets, and
/// `sh:qualifiedValueShape`+`sh:qualified*Count` each span several predicates, so they are
/// gathered here and assembled in [`Self::finish`].
#[derive(Default)]
struct CompAcc {
    comps: Vec<ConstraintComponent>,
    pattern_regex: Option<String>,
    pattern_flags: Option<String>,
    num_min: Option<f64>,
    num_min_inclusive: bool,
    num_max: Option<f64>,
    num_max_inclusive: bool,
    dt_min: Option<String>,
    dt_min_inclusive: bool,
    dt_max: Option<String>,
    dt_max_inclusive: bool,
    qvs_shape: Option<Vec<ConstraintComponent>>,
    qvs_min: Option<u32>,
    qvs_max: Option<u32>,
}

impl CompAcc {
    fn new() -> Self {
        // Absent-bound inclusivity defaults to `true` (SHACL has no exclusive without a
        // bound); the present-bound facet overrides it below.
        Self {
            num_min_inclusive: true,
            num_max_inclusive: true,
            dt_min_inclusive: true,
            dt_max_inclusive: true,
            ..Self::default()
        }
    }

    /// Record a lower-bound facet (numeric or datetime).
    fn set_min(&mut self, facet: Facet, inclusive: bool) {
        match facet {
            Facet::Numeric(n) => {
                self.num_min = Some(n);
                self.num_min_inclusive = inclusive;
            }
            Facet::DateTime(s) => {
                self.dt_min = Some(s);
                self.dt_min_inclusive = inclusive;
            }
        }
    }

    /// Record an upper-bound facet (numeric or datetime).
    fn set_max(&mut self, facet: Facet, inclusive: bool) {
        match facet {
            Facet::Numeric(n) => {
                self.num_max = Some(n);
                self.num_max_inclusive = inclusive;
            }
            Facet::DateTime(s) => {
                self.dt_max = Some(s);
                self.dt_max_inclusive = inclusive;
            }
        }
    }

    /// Feed one `(predicate, object)` pair. Returns `Ok(true)` when the predicate is a
    /// covered value-level component predicate (and it was consumed), `Ok(false)` when the
    /// predicate is not a component predicate at all (the caller decides whether that is a
    /// structural predicate it handles, a presentation predicate to skip, or a residue
    /// construct), and `Err` only on genuine malformation of a covered predicate.
    fn feed(
        &mut self,
        ds: &RdfDataset,
        pred: &str,
        obj: TermId,
        shape: &str,
        unsupported: &mut Vec<String>,
    ) -> gmeow_errors::Result<bool> {
        let ctx = format!("read_shacl_shape: <{shape}>");
        let Some(local) = shacl_local(pred) else {
            return Ok(false);
        };
        match local {
            "class" => self
                .comps
                .push(ConstraintComponent::Class(obj_iri(ds, obj, &ctx)?)),
            "datatype" => self
                .comps
                .push(ConstraintComponent::Datatype(obj_iri(ds, obj, &ctx)?)),
            "nodeKind" => self
                .comps
                .push(ConstraintComponent::NodeKindShacl(parse_node_kind(
                    &obj_iri(ds, obj, &ctx)?,
                    &ctx,
                )?)),
            "in" => {
                let members = parse_rdf_list(ds, obj, &ctx)?
                    .into_iter()
                    .map(|m| shape_value(ds, m, &ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.comps.push(ConstraintComponent::In(members));
            }
            "languageIn" => {
                let langs = parse_rdf_list(ds, obj, &ctx)?
                    .into_iter()
                    .map(|m| obj_lexical(ds, m, &ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.comps.push(ConstraintComponent::LanguageIn(langs));
            }
            "hasValue" => self
                .comps
                .push(ConstraintComponent::HasValue(shape_value(ds, obj, &ctx)?)),
            "minInclusive" => self.set_min(parse_facet(ds, obj, &ctx)?, true),
            "minExclusive" => self.set_min(parse_facet(ds, obj, &ctx)?, false),
            "maxInclusive" => self.set_max(parse_facet(ds, obj, &ctx)?, true),
            "maxExclusive" => self.set_max(parse_facet(ds, obj, &ctx)?, false),
            "pattern" => self.pattern_regex = Some(obj_lexical(ds, obj, &ctx)?),
            "flags" => self.pattern_flags = Some(obj_lexical(ds, obj, &ctx)?),
            "minLength" => self
                .comps
                .push(ConstraintComponent::MinLength(obj_u32(ds, obj, &ctx)?)),
            "maxLength" => self
                .comps
                .push(ConstraintComponent::MaxLength(obj_u32(ds, obj, &ctx)?)),
            "not" => {
                let mut inner = collect_components(ds, obj, shape, unsupported)?;
                if inner.len() != 1 {
                    return Err(parse_err(format!(
                        "{ctx}: sh:not must wrap exactly one component, found {}",
                        inner.len()
                    )));
                }
                self.comps
                    .push(ConstraintComponent::Not(Box::new(inner.remove(0))));
            }
            "qualifiedValueShape" => {
                self.qvs_shape = Some(collect_components(ds, obj, shape, unsupported)?)
            }
            "qualifiedMinCount" => self.qvs_min = Some(obj_u32(ds, obj, &ctx)?),
            "qualifiedMaxCount" => self.qvs_max = Some(obj_u32(ds, obj, &ctx)?),
            // A SHACL-namespace predicate that is NOT a covered component predicate — the
            // caller routes it (presentation skip, structural, or residue).
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Assemble the cross-predicate components and return the full component list.
    fn finish(mut self, shape: &str) -> gmeow_errors::Result<Vec<ConstraintComponent>> {
        match (self.pattern_regex.take(), self.pattern_flags.take()) {
            (Some(regex), flags) => self
                .comps
                .push(ConstraintComponent::Pattern { regex, flags }),
            (None, Some(_)) => {
                return Err(parse_err(format!(
                    "read_shacl_shape: <{shape}> carries sh:flags with no sh:pattern"
                )));
            }
            (None, None) => {}
        }
        if self.num_min.is_some() || self.num_max.is_some() {
            self.comps.push(ConstraintComponent::NumericRange {
                min: self.num_min,
                max: self.num_max,
                min_inclusive: self.num_min_inclusive,
                max_inclusive: self.num_max_inclusive,
            });
        }
        if self.dt_min.is_some() || self.dt_max.is_some() {
            self.comps.push(ConstraintComponent::DateTimeRange {
                min: self.dt_min.take(),
                max: self.dt_max.take(),
                min_inclusive: self.dt_min_inclusive,
                max_inclusive: self.dt_max_inclusive,
            });
        }
        if let Some(shape_inner) = self.qvs_shape.take() {
            self.comps.push(ConstraintComponent::QualifiedValueShape {
                shape: shape_inner,
                min: self.qvs_min,
                max: self.qvs_max,
            });
        } else if self.qvs_min.is_some() || self.qvs_max.is_some() {
            return Err(parse_err(format!(
                "read_shacl_shape: <{shape}> carries sh:qualifiedMinCount/MaxCount with no \
                 sh:qualifiedValueShape"
            )));
        }
        Ok(self.comps)
    }
}

/// Parse a blank node that carries ONLY value-level component predicates (the `sh:not`
/// and `sh:qualifiedValueShape` inner shapes). A presentation predicate is skipped; a
/// genuinely-uncovered construct is routed to `unsupported`; a covered predicate with a
/// malformed object hard-fails.
fn collect_components(
    ds: &RdfDataset,
    subject: TermId,
    shape: &str,
    unsupported: &mut Vec<String>,
) -> gmeow_errors::Result<Vec<ConstraintComponent>> {
    let mut acc = CompAcc::new();
    for (pred, obj) in quads_of(ds, subject) {
        if !acc.feed(ds, &pred, obj, shape, unsupported)? {
            route_unhandled(pred, unsupported);
        }
    }
    acc.finish(shape)
}

/// Parse one `sh:property [ … ]` block into a [`PropertyConstraintIr`], or `Ok(None)` when
/// the property has an uncovered non-inverse complex path (recorded in `unsupported`) that
/// cannot be represented as a single predicate path.
fn parse_property_shape(
    ds: &RdfDataset,
    subject: TermId,
    shape: &str,
    unsupported: &mut Vec<String>,
) -> gmeow_errors::Result<Option<PropertyConstraintIr>> {
    let ctx = format!("read_shacl_shape: <{shape}> property shape");
    let mut path: Option<(String, bool)> = None;
    let mut complex_path = false;
    let mut min_count: Option<u32> = None;
    let mut max_count: Option<u32> = None;
    let mut reifier_shape: Option<String> = None;
    let mut reification_required = false;
    let mut severity: Option<ShaclSeverity> = None;
    let mut message: Option<String> = None;
    let mut acc = CompAcc::new();

    for (pred, obj) in quads_of(ds, subject) {
        match shacl_local(&pred) {
            Some("path") => match ds.resolve(obj) {
                TermRef::Iri(p) => path = Some((p.to_owned(), false)),
                TermRef::Blank { .. } => {
                    let mut inv: Option<String> = None;
                    for (ip, io) in quads_of(ds, obj) {
                        match shacl_local(&ip) {
                            Some("inversePath") => inv = Some(obj_iri(ds, io, &ctx)?),
                            // A sequence / alternative / zeroOrMore… path is uncovered:
                            // record the construct and mark the property complex.
                            _ => {
                                complex_path = true;
                                route_unhandled(format!("sh:path/{ip}"), unsupported);
                            }
                        }
                    }
                    match inv {
                        Some(p) => path = Some((p, true)),
                        None => complex_path = true,
                    }
                }
                other => {
                    return Err(parse_err(format!(
                        "{ctx}: sh:path must be an IRI or an inverse-path blank, found {other:?}"
                    )));
                }
            },
            Some("minCount") => min_count = Some(obj_u32(ds, obj, &ctx)?),
            Some("maxCount") => max_count = Some(obj_u32(ds, obj, &ctx)?),
            Some("reifierShape") => reifier_shape = Some(obj_iri(ds, obj, &ctx)?),
            Some("reificationRequired") => reification_required = obj_bool(ds, obj, &ctx)?,
            Some("severity") => severity = parse_severity(&obj_iri(ds, obj, &ctx)?),
            Some("message") => message = Some(obj_lexical(ds, obj, &ctx)?),
            _ => {
                if !acc.feed(ds, &pred, obj, shape, unsupported)? {
                    route_unhandled(pred, unsupported);
                }
            }
        }
    }

    let components = acc.finish(shape)?;
    // A property with an uncovered complex path (no representable single-predicate path)
    // has its path construct already recorded in `unsupported`; skip building the property.
    let Some((path_iri, inverse)) = path else {
        if complex_path {
            return Ok(None);
        }
        return Err(parse_err(format!("{ctx}: a property shape has no sh:path")));
    };
    // Provenance is emit-dropped (SHACL has no ledger polarity). The constructor requires
    // it Some iff a cardinality is present; the placeholder never changes the enforcement
    // key (`equivalent`/`subsumes` project provenance out), so a fixed value is faithful.
    let provenance =
        (min_count.is_some() || max_count.is_some()).then_some(ConstraintProvenance::OptNative);
    let mut prop =
        PropertyConstraintIr::new(path_iri, min_count, max_count, provenance, components)?;
    if let Some(sev) = severity {
        prop = prop.with_severity(sev);
    }
    if let Some(msg) = message
        && !msg.trim().is_empty()
    {
        prop = prop.with_message(msg)?;
    }
    if inverse {
        prop = prop.inverted();
    }
    if reifier_shape.is_some() || reification_required {
        // `with_reifier` hard-fails on an inverse path — the exact HARD FAIL the emitter's
        // suppression means should never round-trip; propagate it rather than paper over it.
        prop = prop.with_reifier(reifier_shape, reification_required)?;
    }
    Ok(Some(prop))
}

/// Tokenize a SPARQL `sh:select` string into Turtle/SPARQL terms: `<iri>` and `"…"` /
/// `"""…"""` are single tokens; `;` `.` `,` `(` `)` `{` `}` are punctuation tokens; the rest
/// are whitespace-separated words (variables, keywords, CURIEs, the `a` shorthand). Only the
/// small closed grammar the projector emits (and the value-keyed selects the corpus hand-authors)
/// needs to be recognized, so this is intentionally minimal, not a full SPARQL lexer.
fn tokenize_select(s: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '<' {
            let mut t = String::new();
            for ch in chars.by_ref() {
                t.push(ch);
                if ch == '>' {
                    break;
                }
            }
            toks.push(t);
        } else if c == '"' {
            // A `"""…"""` or `"…"` literal — consume through the matching closing quote run.
            let mut t = String::new();
            t.push(chars.next().expect("peeked quote"));
            let triple = chars.peek() == Some(&'"');
            if triple {
                t.push(chars.next().expect("second quote"));
                if chars.peek() == Some(&'"') {
                    t.push(chars.next().expect("third quote"));
                }
            }
            let mut run = 0usize;
            for ch in chars.by_ref() {
                t.push(ch);
                run = if ch == '"' { run + 1 } else { 0 };
                if (triple && run == 3) || (!triple && run == 1) {
                    break;
                }
            }
            toks.push(t);
        } else if matches!(c, ';' | '.' | ',' | '(' | ')' | '{' | '}') {
            toks.push(c.to_string());
            chars.next();
        } else {
            let mut t = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() || matches!(ch, ';' | '.' | ',' | '(' | ')' | '{' | '}' | '<')
                {
                    break;
                }
                t.push(ch);
                chars.next();
            }
            toks.push(t);
        }
    }
    toks
}

/// The value-keyed target inverted from a `sh:select` string: the single `?this <pred> <value>`
/// basic pattern (the exact form [`gmeow_logic_compile::projections::shapes`] emits), returned as
/// `(predicate, value, extra_type_classes)`. `extra_type_classes` are any additional
/// `?this a <Class>` type patterns the hand-authored select also carried (e.g. the mode-scoped
/// commitment's `?this a gmeow:InferenceCommitment`) — the IR value-keyed target cannot hold a
/// class, so they are surfaced to the caller as residue. `None` when the select is not the
/// value-keyed shape at all (no single non-type `?this` pattern, or a non-IRI object).
fn parse_value_key_select(select: &str) -> Option<(String, String, Vec<String>)> {
    let toks = tokenize_select(select);
    // Prefix map: `PREFIX name: <iri>` (SPARQL) — collected across the whole select.
    let mut prefixes: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut i = 0;
    while i + 2 < toks.len() {
        if toks[i].eq_ignore_ascii_case("prefix") {
            let label = toks[i + 1].trim_end_matches(':').to_owned();
            if let Some(iri) = toks[i + 2]
                .strip_prefix('<')
                .and_then(|t| t.strip_suffix('>'))
            {
                prefixes.insert(label, iri.to_owned());
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    let resolve = |t: &str| -> Option<String> {
        if t == "a" {
            return Some(RDF_TYPE.to_owned());
        }
        if let Some(inner) = t.strip_prefix('<').and_then(|x| x.strip_suffix('>')) {
            return Some(inner.to_owned());
        }
        let (label, local) = t.split_once(':')?;
        prefixes.get(label).map(|ns| format!("{ns}{local}"))
    };
    // The WHERE body between the first `{` and the last `}`.
    let open = toks.iter().position(|t| t == "{")?;
    let close = toks.iter().rposition(|t| t == "}")?;
    if close <= open {
        return None;
    }
    let body = &toks[open + 1..close];
    // Walk `subj (pred obj)(; pred obj)* .` patterns; collect only those on `?this`.
    let mut value_keys: Vec<(String, String)> = Vec::new();
    let mut type_classes: Vec<String> = Vec::new();
    let mut j = 0;
    let mut subject: Option<String> = None;
    while j < body.len() {
        let t = &body[j];
        if t == "." {
            subject = None;
            j += 1;
            continue;
        }
        if t == ";" {
            // Reuse the current subject for the next predicate-object pair.
            if subject.is_none() || j + 2 >= body.len() {
                return None;
            }
            j += 1;
        } else {
            subject = Some(t.clone());
            j += 1;
        }
        if j + 1 >= body.len() {
            return None;
        }
        let pred = &body[j];
        let obj = &body[j + 1];
        j += 2;
        // Only `?this`-subject patterns key a value target; any other subject is out of grammar.
        if subject.as_deref() != Some("?this") {
            return None;
        }
        let pred_iri = resolve(pred)?;
        if pred_iri == RDF_TYPE {
            type_classes.push(resolve(obj)?);
        } else {
            // The value object must be an IRI (an `sh:SPARQLTarget` binds an IRI focus node).
            let val = resolve(obj)?;
            if obj.starts_with('"') {
                return None;
            }
            value_keys.push((pred_iri, val));
        }
    }
    // Exactly one non-type value pattern is the value-keyed shape; anything else is out of grammar.
    if value_keys.len() != 1 {
        return None;
    }
    let (predicate, value) = value_keys.into_iter().next().expect("length checked");
    type_classes.sort();
    type_classes.dedup();
    Some((predicate, value, type_classes))
}

/// Invert an `sh:target [ a sh:SPARQLTarget ; sh:select "…" ]` blank node into a
/// [`ShapeTarget::ValueKeyed`]. A non-`sh:SPARQLTarget` target, a target with no `sh:select`, or a
/// `sh:select` that is not the value-keyed single-triple form is routed to `unsupported` (never a
/// hard error), returning `None`. When the select carries extra `?this a <Class>` type patterns
/// (which the IR value-keyed target cannot hold), those are surfaced to `unsupported` too.
fn parse_sparql_target(
    ds: &RdfDataset,
    obj: TermId,
    ctx: &str,
    unsupported: &mut Vec<String>,
) -> gmeow_errors::Result<Option<ShapeTarget>> {
    let inner = quads_of(ds, obj);
    let is_sparql_target = inner.iter().any(|(p, o)| {
        p == RDF_TYPE && matches!(ds.resolve(*o), TermRef::Iri(s) if s == SH_SPARQLTARGET)
    });
    if !is_sparql_target {
        unsupported.push(format!("{SH}target (non-SPARQLTarget target)"));
        return Ok(None);
    }
    let Some((_, sel)) = inner.iter().find(|(p, _)| shacl_local(p) == Some("select")) else {
        unsupported.push(format!("{SH}target (sh:SPARQLTarget without sh:select)"));
        return Ok(None);
    };
    let select = obj_lexical(ds, *sel, ctx)?;
    match parse_value_key_select(&select) {
        Some((predicate, value, type_classes)) => {
            for c in type_classes {
                unsupported.push(format!(
                    "sh:SPARQLTarget additional type constraint ?this a <{c}>"
                ));
            }
            Ok(Some(ShapeTarget::ValueKeyed { predicate, value }))
        }
        None => {
            unsupported.push(format!(
                "{SH}target sh:select (not the value-keyed single-triple form)"
            ));
            Ok(None)
        }
    }
}

/// The direct focus-node target authored on `node` (`sh:targetClass` / `sh:targetSubjectsOf` /
/// `sh:targetObjectsOf`, or a value-keyed `sh:target`), or `None` when it carries none. Used by
/// the owner-walk to adopt an owning node shape's target for an inline/`sh:node` helper shape.
fn direct_target_of(ds: &RdfDataset, node: TermId) -> Option<ShapeTarget> {
    for (pred, o) in quads_of(ds, node) {
        match shacl_local(&pred) {
            Some("targetClass") => {
                if let TermRef::Iri(s) = ds.resolve(o) {
                    return Some(ShapeTarget::Class(s.to_owned()));
                }
            }
            Some("targetSubjectsOf") => {
                if let TermRef::Iri(s) = ds.resolve(o) {
                    return Some(ShapeTarget::SubjectsOf(s.to_owned()));
                }
            }
            Some("targetObjectsOf") => {
                if let TermRef::Iri(s) = ds.resolve(o) {
                    return Some(ShapeTarget::ObjectsOf(s.to_owned()));
                }
            }
            Some("target") => {
                let mut sink = Vec::new();
                if let Ok(Some(t)) = parse_sparql_target(ds, o, "owner-walk", &mut sink) {
                    return Some(t);
                }
            }
            _ => {}
        }
    }
    None
}

/// The subjects that reference `object` via `sh:node` or `sh:property`, sorted deterministically
/// by their resolved term form (a blank id or IRI) so the owner-walk is order-independent.
fn shape_referrers(ds: &RdfDataset, object: TermId) -> Vec<TermId> {
    let mut refs: Vec<TermId> = ds
        .quads_for_pattern(None, None, Some(object), GraphMatch::Any)
        .filter(|q| matches!(ds.resolve(q.p), TermRef::Iri(p) if matches!(shacl_local(p), Some("node") | Some("property"))))
        .map(|q| q.s)
        .collect();
    refs.sort_by_key(|&id| format!("{:?}", ds.resolve(id)));
    refs.dedup();
    refs
}

/// Resolve a target for a targetless inline / `sh:node` / property-only helper shape by walking
/// the inverse `sh:node` / `sh:property` edge to the owning node shape and adopting ITS target.
/// Walks at most two edges (property-blank → owning node shape). `None` for a genuinely orphan
/// top-level targetless shape (no incoming `sh:node`/`sh:property` reaches a targeted owner).
fn target_via_owner(ds: &RdfDataset, subject: TermId) -> Option<ShapeTarget> {
    for r in shape_referrers(ds, subject) {
        if let Some(t) = direct_target_of(ds, r) {
            return Some(t);
        }
        for r2 in shape_referrers(ds, r) {
            if let Some(t) = direct_target_of(ds, r2) {
                return Some(t);
            }
        }
    }
    None
}

/// Try to read a node-level `sh:or` list as the covered property-alternatives disjunction: every
/// list member must be a blank property shape carrying EXACTLY `sh:path <IRI>` and
/// `sh:minCount 1` (presentation predicates absorbed). Returns `Ok(Some(paths))` (two or more,
/// unsorted — the IR normalizes) on the exact form, `Ok(None)` for any other `sh:or` (which the
/// caller records as residue), and `Err` only on a malformed RDF list.
fn parse_or_properties(
    ds: &RdfDataset,
    head: TermId,
    shape: &str,
) -> gmeow_errors::Result<Option<Vec<String>>> {
    let ctx = format!("read_shacl_shape: <{shape}> sh:or");
    let members = parse_rdf_list(ds, head, &ctx)?;
    if members.len() < 2 {
        return Ok(None);
    }
    let mut paths = Vec::with_capacity(members.len());
    for m in members {
        let mut path: Option<String> = None;
        let mut min_one = false;
        for (pred, obj) in quads_of(ds, m) {
            match shacl_local(&pred) {
                Some("path") => match ds.resolve(obj) {
                    TermRef::Iri(p) => path = Some(p.to_owned()),
                    _ => return Ok(None),
                },
                Some("minCount") => {
                    if obj_u32(ds, obj, &ctx)? != 1 {
                        return Ok(None);
                    }
                    min_one = true;
                }
                _ if is_presentation(&pred) => {}
                _ => return Ok(None),
            }
        }
        match (path, min_one) {
            (Some(p), true) => paths.push(p),
            _ => return Ok(None),
        }
    }
    Ok(Some(paths))
}

/// Parse ONE `sh:NodeShape` subject out of an already-parsed RDF dataset into a
/// [`ShapeRead`] — the covered fragment plus its residue list.
///
/// The reader inverts, on the node: `rdf:type sh:NodeShape`, `sh:targetClass` /
/// `sh:targetSubjectsOf` / `sh:targetObjectsOf`, `sh:property`, and node-level `sh:class` /
/// `sh:datatype` / `sh:nodeKind` / `sh:not` / range / `sh:hasValue`. On a property
/// (`sh:property [ … ]`): `sh:path` (with `sh:inversePath`), `sh:minCount`, `sh:maxCount`,
/// `sh:class`, `sh:datatype`, `sh:nodeKind`, `sh:in`, `sh:hasValue`, the numeric/datetime
/// range facets, `sh:pattern` + `sh:flags`, `sh:minLength`, `sh:maxLength`, `sh:languageIn`,
/// `sh:qualifiedValueShape` with `sh:qualifiedMinCount`/`sh:qualifiedMaxCount`, `sh:not`,
/// `sh:reifierShape`, `sh:reificationRequired`, and the presentation `sh:severity` /
/// `sh:message`. Presentation/annotation predicates (`rdfs:label`, `sh:name`,
/// `sh:description`, `sh:order`, `sh:group`, `sh:deactivated`, `skos:*`) are skipped.
///
/// # Comparison-only
///
/// The returned IR exists solely for the [`oracle`] to compare; this function performs no
/// I/O and writes nothing (Principle 4 — SHACL never parses back into the `logic:` canon).
///
/// # Errors
///
/// `Err` only on genuine malformation: the shape IRI is absent, is not a `sh:NodeShape`,
/// has no target, or a covered predicate carries a malformed object (an unparsable RDF
/// list, a bound the constructor rejects such as `min > max`, or a value with both a
/// datatype and a language). Every uncovered construct (`sh:or`, `sh:xone`, `sh:and`,
/// `sh:node`, the `sh:sparql` fragment, `sh:uniqueLang`, `sh:targetNode`, an
/// `sh:SPARQLTarget`, …) is recorded in [`ShapeRead::unsupported`], not surfaced as `Err`.
pub fn read_shacl_shape(
    graph: &RdfDataset,
    node_shape_iri: &str,
) -> gmeow_errors::Result<ShapeRead> {
    let subject = graph
        .term_id_by_value(&TermValue::iri(node_shape_iri))
        .ok_or_else(|| {
            parse_err(format!(
                "read_shacl_shape: shape IRI <{node_shape_iri}> is not present in the graph"
            ))
        })?;

    let mut is_node_shape = false;
    let mut target: Option<ShapeTarget> = None;
    let mut properties: Vec<PropertyConstraintIr> = Vec::new();
    let mut node_acc = CompAcc::new();
    let mut unsupported: Vec<String> = Vec::new();
    let mut failure_classes = BTreeSet::new();

    let set_target = |t: ShapeTarget, cur: &mut Option<ShapeTarget>| -> gmeow_errors::Result<()> {
        if cur.is_some() {
            return Err(parse_err(format!(
                "read_shacl_shape: <{node_shape_iri}> carries more than one target — a shape must \
                 have exactly one focus-node selector"
            )));
        }
        *cur = Some(t);
        Ok(())
    };

    for (pred, obj) in quads_of(graph, subject) {
        if pred == RDF_TYPE {
            // `a sh:NodeShape` marks the shape; any other rdf:type (e.g. also owl:Class) is
            // ignored, not a residue construct.
            if let TermRef::Iri(t) = graph.resolve(obj)
                && t == SH_NODESHAPE
            {
                is_node_shape = true;
            }
            continue;
        }
        if pred == GMEOW_ENFORCES_FAILURE_CLASS {
            failure_classes.insert(obj_iri(graph, obj, node_shape_iri)?);
            continue;
        }
        match shacl_local(&pred) {
            Some("targetClass") => set_target(
                ShapeTarget::Class(obj_iri(graph, obj, node_shape_iri)?),
                &mut target,
            )?,
            Some("targetSubjectsOf") => set_target(
                ShapeTarget::SubjectsOf(obj_iri(graph, obj, node_shape_iri)?),
                &mut target,
            )?,
            Some("targetObjectsOf") => set_target(
                ShapeTarget::ObjectsOf(obj_iri(graph, obj, node_shape_iri)?),
                &mut target,
            )?,
            // A value-keyed `sh:target [ a sh:SPARQLTarget ; sh:select "…" ]`. A recognizable
            // value-keyed select becomes a `ValueKeyed` target; anything else is routed to the
            // residue list (never a hard error) by `parse_sparql_target`.
            Some("target") => {
                if let Some(t) = parse_sparql_target(graph, obj, node_shape_iri, &mut unsupported)?
                {
                    set_target(t, &mut target)?;
                }
            }
            Some("property") => {
                if let Some(p) = parse_property_shape(graph, obj, node_shape_iri, &mut unsupported)?
                {
                    properties.push(p);
                }
            }
            // A NODE-level `sh:or` whose every branch is exactly `[ sh:path <P> ; sh:minCount 1 ]`
            // is the covered property-alternatives disjunction
            // ([`ConstraintComponent::OrProperties`], the projection of a class-level
            // `owl:unionOf` over bare property existentials). Any other `sh:or` (value branches,
            // extra branch constraints) stays genuinely-uncovered residue, exactly as before.
            Some("or") => match parse_or_properties(graph, obj, node_shape_iri)? {
                Some(paths) => node_acc
                    .comps
                    .push(ConstraintComponent::OrProperties(paths)),
                None => route_unhandled(pred, &mut unsupported),
            },
            _ => {
                if !node_acc.feed(graph, &pred, obj, node_shape_iri, &mut unsupported)? {
                    route_unhandled(pred, &mut unsupported);
                }
            }
        }
    }

    if !is_node_shape {
        return Err(parse_err(format!(
            "read_shacl_shape: <{node_shape_iri}> is not typed sh:NodeShape"
        )));
    }
    if failure_classes.len() > 1 {
        return Err(parse_err(format!(
            "read_shacl_shape: <{node_shape_iri}> carries distinct gmeow:enforcesFailureClass values: {failure_classes:?}"
        )));
    }
    // A targetless inline / `sh:node` / property-only helper shape (e.g. one referenced by an
    // owning node shape's `sh:node`) adopts the owning node shape's target by walking the inverse
    // `sh:node` / `sh:property` edge. Only a genuinely orphan top-level targetless node shape (no
    // targeted owner reachable) remains an `Err`.
    let target = match target {
        Some(t) => t,
        None => target_via_owner(graph, subject).ok_or_else(|| {
            parse_err(format!(
                "read_shacl_shape: <{node_shape_iri}> has no sh:targetClass / sh:targetSubjectsOf \
                 / sh:targetObjectsOf and no owning sh:node/sh:property shape to adopt a target from"
            ))
        })?,
    };
    let node_components = node_acc.finish(node_shape_iri)?;
    let mut ir = ValidationShapeIr::new(node_shape_iri, target, properties, None)?
        .with_node_components(node_components)?;
    if let Some(failure_class) = failure_classes.first() {
        ir = ir.with_failure_class(failure_class)?;
    }

    unsupported.sort();
    unsupported.dedup();
    Ok(ShapeRead { ir, unsupported })
}

// ── Part B: the oracle ─────────────────────────────────────────────────────────

/// The oracle's verdict on a legacy-vs-projected shape pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleVerdict {
    /// The covered fragments flag exactly the same focus nodes over every graph (`≡`).
    /// The comparison is over the covered `ir` only — the projected side is generated from
    /// canon and carries no residue, so a residue-bearing legacy shape can still be `≡` on
    /// its covered part (but see [`Self::residue_bearing`]).
    pub equivalent: bool,
    /// The projected shape enforces at least everything the legacy covered fragment does
    /// (`subsumes(projected, legacy.ir)`) — the Galois soundness direction: a deletion that
    /// keeps the projection never loses enforcement on the covered part.
    pub legacy_subsumed_by_projected: bool,
    /// The residue normal form of the LEGACY covered fragment (the constructs no SHACL
    /// surface can faithfully hold), reused from the Task-1 classifier.
    pub residue: Vec<String>,
    /// The genuinely-uncovered constructs the legacy shape also carried (from the read).
    pub unsupported: Vec<String>,
    /// `true` iff `unsupported` is non-empty. A residue-bearing legacy class is **NOT
    /// deletable on the covered `equivalent` match alone** — its uncovered residue must be
    /// authored as a `logic:` constraint (and become part of the projection) first;
    /// otherwise deleting the legacy shape silently loses the residue's enforcement.
    pub residue_bearing: bool,
    /// A deterministic, panic-free human explanation; when not equivalent it names the
    /// first differing enforcement sub-key.
    pub reason: String,
}

/// The deterministic set of enforcement sub-keys of a shape, built from public IR data:
/// one entry for the target, the standpoint, each node component, and each property (its
/// path folded in). Diffing two such sets names the first per-path / per-component
/// difference without reaching the crate-private enforcement-key machinery.
fn enforcement_sub_keys(shape: &ValidationShapeIr) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    keys.insert(format!("target={:?}", shape.target));
    keys.insert(format!("standpoint={:?}", shape.standpoint));
    for c in &shape.node_components {
        keys.insert(format!("node-component={c:?}"));
    }
    for p in &shape.properties {
        keys.insert(format!(
            "path={} min={:?} max={:?} inverse={} reifier={:?} reifreq={} components={:?}",
            p.path,
            p.min_count,
            p.max_count,
            p.inverse,
            p.reifier_shape,
            p.reification_required,
            p.components,
        ));
    }
    keys
}

/// The deterministic reason string for a non-equivalent pair: the lexicographically first
/// enforcement sub-key present in exactly one of the two shapes.
fn diff_reason(legacy: &ValidationShapeIr, projected: &ValidationShapeIr) -> String {
    let l = enforcement_sub_keys(legacy);
    let p = enforcement_sub_keys(projected);
    if let Some(only_legacy) = l.difference(&p).next() {
        return format!("not equivalent: legacy-only enforcement sub-key [{only_legacy}]");
    }
    if let Some(only_projected) = p.difference(&l).next() {
        return format!("not equivalent: projected-only enforcement sub-key [{only_projected}]");
    }
    // The sub-key sets coincide but the canonical enforcement keys disagree — a folding
    // difference the coarse sub-keys did not surface; report it rather than claim equality.
    format!(
        "not equivalent: enforcement keys differ (legacy={}, projected={})",
        subsumption::enforcement_key(legacy),
        subsumption::enforcement_key(projected),
    )
}

/// Decide equivalence and Galois soundness for a legacy read vs a projected shape through
/// the Task-1 subsumption lattice.
///
/// Equivalence is decided over the COVERED fragment only (`legacy.ir` vs `projected`) — the
/// projected side is minted from canon and bears no residue. The verdict carries the
/// legacy read's `unsupported` list and the `residue_bearing` flag so a caller never
/// mistakes a covered-fragment match for deletion clearance: a residue-bearing legacy class
/// still enforces its uncovered residue, which must be canon-grounded before deletion.
/// Never panics.
pub fn oracle(legacy: &ShapeRead, projected: &ValidationShapeIr) -> OracleVerdict {
    let equivalent = subsumption::equivalent(&legacy.ir, projected);
    let legacy_subsumed_by_projected = subsumption::subsumes(projected, &legacy.ir);
    let residue = subsumption::residue_normal_form(&legacy.ir);
    let residue_bearing = !legacy.unsupported.is_empty();
    let reason = if equivalent {
        "equivalent: the legacy covered fragment and the projected shape flag the same focus \
         nodes over every graph"
            .to_owned()
    } else {
        diff_reason(&legacy.ir, projected)
    };
    OracleVerdict {
        equivalent,
        legacy_subsumed_by_projected,
        residue,
        unsupported: legacy.unsupported.clone(),
        residue_bearing,
        reason,
    }
}

// ── Part C: non-vacuous fixture cross-check ────────────────────────────────────

/// A discriminating near-miss: a fresh focus node plus the Turtle triples that make it a
/// focus of the shape and violate exactly one component. Derived from the shape lattice by
/// [`witnesses_for`] so a new component family forces a witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Witness {
    /// The component family this near-miss discriminates (e.g. `sh:class@<path>`).
    pub component: String,
    /// The fresh focus-node IRI the near-miss introduces.
    pub focus: String,
    /// The Turtle triples (absolute IRIs, no prefixes) unioned with the base data graph.
    pub triples: String,
}

/// Mint the next fresh focus IRI.
fn mint(idx: &mut usize) -> String {
    let f = format!("https://gmeow.example/witness/{idx}");
    *idx += 1;
    f
}

/// The triples that make `focus` a focus node of `target` (an instance of the target
/// class, a subject/object of the target predicate, or the value-keyed subject).
fn target_triples(target: &ShapeTarget, focus: &str) -> String {
    match target {
        ShapeTarget::Class(c) => format!("<{focus}> <{RDF_TYPE}> <{c}> .\n"),
        // A DIRECT-class focus is typed the class and NOTHING else — a lone `a <c>` (with no
        // proper-subclass typing) IS a direct instance, so it matches the subclass-excluding
        // sh:SPARQLTarget exactly as a plain class target matches sh:targetClass.
        ShapeTarget::DirectClass(c) => format!("<{focus}> <{RDF_TYPE}> <{c}> .\n"),
        ShapeTarget::SubjectsOf(p) => format!("<{focus}> <{p}> <{focus}/target-object> .\n"),
        ShapeTarget::ObjectsOf(p) => format!("<{focus}/target-subject> <{p}> <{focus}> .\n"),
        ShapeTarget::ValueKeyed { predicate, value } => {
            format!("<{focus}> <{predicate}> <{value}> .\n")
        }
        // A raw SPARQL target selects its focus set with an arbitrary `SELECT ?this WHERE { … }`
        // body that has no OWL/RDFS antecedent and cannot be inverted into witness triples — it is
        // entirely uncovered residue (Part A), never a SHACL-Core-comparable target that reaches
        // this cross-check. If one is ever mis-routed here, it synthesizes no focus node and the
        // vacuity guard in `cross_check` HARD-FAILS (a pass that exercised nothing is not a pass).
        ShapeTarget::Sparql(_) => String::new(),
    }
}

/// A literal whose datatype DIFFERS from `d` (an `xsd:integer` when `d` is `xsd:string`,
/// else a plain `xsd:string`) — the value that violates an `sh:datatype d` constraint.
fn wrong_datatype_literal(d: &str) -> String {
    if d == XSD_STRING {
        format!("\"1\"^^<{XSD_INTEGER}>")
    } else {
        "\"gmeow-wrong-datatype\"".to_owned()
    }
}

/// A term of the WRONG node kind for `k` — the value that violates an `sh:nodeKind k`.
fn wrong_node_kind_value(k: ShaclNodeKind, focus: &str) -> String {
    match k {
        ShaclNodeKind::Iri => "\"literal-not-iri\"".to_owned(),
        ShaclNodeKind::Literal => format!("<{focus}/iri-not-literal>"),
        ShaclNodeKind::BlankNode => format!("<{focus}/iri-not-blank>"),
        // A blank node is neither an IRI nor a literal — violates sh:IRIOrLiteral.
        ShaclNodeKind::IriOrLiteral => "_:gmeow_neither".to_owned(),
        ShaclNodeKind::BlankNodeOrIri => "\"literal-neither\"".to_owned(),
        ShaclNodeKind::BlankNodeOrLiteral => format!("<{focus}/iri-neither>"),
    }
}

/// A discriminating near-miss for one property-level component, or `None` when the
/// component family has no SHACL-observable near-miss (a lossy component the emitter drops,
/// or a facet outside the enumerated {class, datatype, in, nodeKind} witness set). The
/// match is exhaustive so a NEW component variant forces a witness decision here.
fn property_component_witness(
    target: &ShapeTarget,
    path: &str,
    comp: &ConstraintComponent,
    idx: &mut usize,
) -> Option<Witness> {
    let (label, viol): (String, String) = {
        let focus = format!("https://gmeow.example/witness/{}", *idx);
        match comp {
            ConstraintComponent::Class(_) => (
                format!("sh:class@{path}"),
                format!("<{focus}> <{path}> <{focus}/wrong-class-value> .\n"),
            ),
            ConstraintComponent::Datatype(d) => (
                format!("sh:datatype@{path}"),
                format!("<{focus}> <{path}> {} .\n", wrong_datatype_literal(d)),
            ),
            ConstraintComponent::In(_) => (
                format!("sh:in@{path}"),
                format!("<{focus}> <{path}> <{focus}/off-list-value> .\n"),
            ),
            ConstraintComponent::NodeKindShacl(k) => (
                format!("sh:nodeKind@{path}"),
                format!(
                    "<{focus}> <{path}> {} .\n",
                    wrong_node_kind_value(*k, &focus)
                ),
            ),
            // No enumerated near-miss (lossy / non-{class,datatype,in,nodeKind}); explicit
            // arms so a new ConstraintComponent variant is a compile error until classified.
            ConstraintComponent::NumericRange { .. }
            | ConstraintComponent::PrecisionRange { .. }
            | ConstraintComponent::Pattern { .. }
            | ConstraintComponent::MinLength(_)
            | ConstraintComponent::MaxLength(_)
            | ConstraintComponent::LanguageIn(_)
            | ConstraintComponent::DateTimeRange { .. }
            | ConstraintComponent::TerminologyBinding { .. }
            | ConstraintComponent::OrdinalSet { .. }
            | ConstraintComponent::DateTimePattern(_)
            | ConstraintComponent::HasValue(_)
            | ConstraintComponent::QualifiedValueShape { .. }
            | ConstraintComponent::Not(_)
            | ConstraintComponent::Or(_)
            | ConstraintComponent::Xone(_)
            | ConstraintComponent::OrProperties(_) => return None,
        }
    };
    let focus = mint(idx);
    let mut triples = target_triples(target, &focus);
    triples.push_str(&viol);
    Some(Witness {
        component: label,
        focus,
        triples,
    })
}

/// A discriminating near-miss for one focus-node-level component: the focus itself (an
/// IRI, from its target membership) is the near-miss the node constraint flags. `None`
/// for a family with no direct node-level near-miss; exhaustive so a new variant forces a
/// decision.
fn node_component_witness(
    target: &ShapeTarget,
    comp: &ConstraintComponent,
    idx: &mut usize,
) -> Option<Witness> {
    let label = match comp {
        ConstraintComponent::Class(c) => format!("node:sh:class:{c}"),
        ConstraintComponent::Datatype(d) => format!("node:sh:datatype:{d}"),
        ConstraintComponent::NodeKindShacl(k) => format!("node:sh:nodeKind:{}", k.as_str()),
        // A bare target instance carrying NONE of the alternative paths violates the node-level
        // property-alternatives `sh:or` — the focus itself is the discriminating near-miss.
        ConstraintComponent::OrProperties(paths) => {
            format!("node:sh:or-properties:{}", paths.join("|"))
        }
        ConstraintComponent::NumericRange { .. }
        | ConstraintComponent::PrecisionRange { .. }
        | ConstraintComponent::In(_)
        | ConstraintComponent::Pattern { .. }
        | ConstraintComponent::MinLength(_)
        | ConstraintComponent::MaxLength(_)
        | ConstraintComponent::LanguageIn(_)
        | ConstraintComponent::DateTimeRange { .. }
        | ConstraintComponent::TerminologyBinding { .. }
        | ConstraintComponent::OrdinalSet { .. }
        | ConstraintComponent::DateTimePattern(_)
        | ConstraintComponent::HasValue(_)
        | ConstraintComponent::QualifiedValueShape { .. }
        | ConstraintComponent::Not(_)
        | ConstraintComponent::Or(_)
        | ConstraintComponent::Xone(_) => return None,
    };
    let focus = mint(idx);
    let triples = target_triples(target, &focus);
    Some(Witness {
        component: label,
        focus,
        triples,
    })
}

/// Derive one discriminating near-miss per component of `shape` from the shape lattice:
/// a cardinality under-flow, a cardinality over-flow, a wrong `sh:class` instance, a wrong
/// `sh:datatype` value, an off-`sh:in` value, and a node-kind violation — plus node-level
/// class/datatype/nodeKind near-misses. Every witness introduces its OWN fresh focus so the
/// findings never interfere.
pub fn witnesses_for(shape: &ValidationShapeIr) -> Vec<Witness> {
    let mut out = Vec::new();
    let mut idx = 0usize;

    for p in &shape.properties {
        // Cardinality under-flow: a focus with FEWER than `min_count` values (zero, so it
        // violates only sh:minCount — no value-constraint cross-noise).
        if let Some(m) = p.min_count
            && m > 0
        {
            let focus = mint(&mut idx);
            let triples = target_triples(&shape.target, &focus);
            out.push(Witness {
                component: format!("sh:minCount@{}", p.path),
                focus,
                triples,
            });
        }
        // Cardinality over-flow: a focus with `max_count + 1` values on the path.
        if let Some(n) = p.max_count {
            let focus = mint(&mut idx);
            let mut triples = target_triples(&shape.target, &focus);
            for k in 0..=n {
                triples.push_str(&format!("<{focus}> <{}> <{focus}/v{k}> .\n", p.path));
            }
            out.push(Witness {
                component: format!("sh:maxCount@{}", p.path),
                focus,
                triples,
            });
        }
        for c in &p.components {
            if let Some(w) = property_component_witness(&shape.target, &p.path, c, &mut idx) {
                out.push(w);
            }
        }
    }
    for c in &shape.node_components {
        if let Some(w) = node_component_witness(&shape.target, c, &mut idx) {
            out.push(w);
        }
    }
    out
}

/// The comparable finding key of a SHACL result: `(focus node, result path, constraint
/// component)`. The source-shape IRI is DELIBERATELY excluded — it differs between the
/// legacy and projected graphs by construction, so including it would make every
/// comparison trivially diverge.
type FindingKey = (String, Option<String>, String);

/// The finding-key set a shape graph produces over `dataset`.
fn finding_keys(
    dataset: &RdfDataset,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> BTreeSet<FindingKey> {
    crate::store::shacl_validate_dataset(dataset, shapes)
        .result_tuples()
        .into_iter()
        .map(|(focus, path, _value, component, _shape, _severity)| (focus, path, component))
        .collect()
}

/// A deterministic divergence message naming the symmetric difference of two finding sets.
fn divergence_message(
    ctx: &str,
    projected: &BTreeSet<FindingKey>,
    legacy: &BTreeSet<FindingKey>,
) -> String {
    let projected_only: Vec<&FindingKey> = projected.difference(legacy).collect();
    let legacy_only: Vec<&FindingKey> = legacy.difference(projected).collect();
    format!(
        "cross_check: SHACL finding sets DIVERGE on {ctx}: projected-only={projected_only:?} \
         legacy-only={legacy_only:?}"
    )
}

/// Run BOTH shape graphs as SHACL validators over the base data graph and over the data
/// graph unioned with each witness, asserting identical finding sets on every input and
/// HARD-FAILING on vacuity.
///
/// The oracle ([`oracle`]) is the equivalence decision; this cross-check only guards the
/// reader/emitter against the lattice model diverging from real SHACL semantics — it is
/// NEVER a substitute for the oracle, and a vacuous pass is not a pass.
///
/// # Errors
///
/// * either shape graph fails to parse;
/// * the two graphs produce DIFFERENT `(focus, path, component)` finding sets on any input;
/// * `cross_check: vacuous — …` when the run exercises zero focus nodes, or a witness for
///   a component produced no discriminating finding (its near-miss did not violate).
pub fn cross_check(
    projected_shacl_ttl: &str,
    legacy_shacl_ttl: &str,
    data_graph: &RdfDataset,
    witnesses: &[Witness],
) -> gmeow_errors::Result<()> {
    let projected = purrdf::shapes::engine::parse_shapes(projected_shacl_ttl).map_err(|e| {
        parse_err(format!(
            "cross_check: projected shapes failed to parse: {e}"
        ))
    })?;
    let legacy = purrdf::shapes::engine::parse_shapes(legacy_shacl_ttl)
        .map_err(|e| parse_err(format!("cross_check: legacy shapes failed to parse: {e}")))?;

    let mut focus_nodes: BTreeSet<String> = BTreeSet::new();

    // Base run over the data graph alone.
    let base_p = finding_keys(data_graph, &projected);
    let base_l = finding_keys(data_graph, &legacy);
    if base_p != base_l {
        return Err(parse_err(divergence_message(
            "the base data graph",
            &base_p,
            &base_l,
        )));
    }
    for (focus, _, _) in &base_p {
        focus_nodes.insert(focus.clone());
    }

    for w in witnesses {
        let witness_ds = parse_dataset(w.triples.as_bytes(), "text/turtle", None).map_err(|e| {
            parse_err(format!(
                "cross_check: witness '{}' failed to parse: {e}",
                w.component
            ))
        })?;
        let mut builder = RdfDatasetBuilder::new();
        builder.push_dataset(data_graph);
        builder.push_dataset(&witness_ds);
        let dataset = builder.freeze().map_err(|e| {
            parse_err(format!(
                "cross_check: witness '{}' dataset freeze failed: {e}",
                w.component
            ))
        })?;

        let p = finding_keys(&dataset, &projected);
        let l = finding_keys(&dataset, &legacy);
        if p != l {
            return Err(parse_err(divergence_message(
                &format!("witness '{}'", w.component),
                &p,
                &l,
            )));
        }
        for (focus, _, _) in &p {
            focus_nodes.insert(focus.clone());
        }
        // The near-miss MUST flag its own focus (both graphs agree, so checking either is
        // sufficient) — otherwise the witness is non-discriminating and the pass is vacuous.
        let expected_focus = format!("<{}>", w.focus);
        if !p.iter().any(|(focus, _, _)| *focus == expected_focus) {
            return Err(parse_err(format!(
                "cross_check: vacuous — witness '{}' for focus {} produced no discriminating \
                 finding (the near-miss did not violate its component)",
                w.component, w.focus
            )));
        }
    }

    if focus_nodes.is_empty() {
        return Err(parse_err(
            "cross_check: vacuous — the run exercised 0 focus nodes (no shape targeted any node \
             in the data graph or witnesses)"
                .to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_logic_compile::projections::shapes::project_validation_shape_shacl;

    /// The SHACL prefix header the emitter's CURIEs (`sh:` / `xsd:`) resolve against.
    const HEADER: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n";

    fn parse_ttl(ttl: &str) -> std::sync::Arc<RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("test turtle must parse")
    }

    /// Emit `shape` with the production emitter, parse it back with the reader, assert the
    /// round trip has NO residue, and return its covered IR.
    fn read_back(shape: &ValidationShapeIr) -> ValidationShapeIr {
        let ttl = format!("{HEADER}{}", project_validation_shape_shacl(shape));
        let ds = parse_ttl(&ttl);
        let read = read_shacl_shape(&ds, &shape.iri)
            .unwrap_or_else(|e| panic!("read_shacl_shape failed for {}: {e}\n{ttl}", shape.iri));
        assert!(
            read.unsupported.is_empty(),
            "a round-tripped emitter shape must have NO residue, got {:?}\n{ttl}",
            read.unsupported
        );
        read.ir
    }

    /// Emit → read-back → enforcement-equivalent to the original.
    fn assert_round_trips(shape: &ValidationShapeIr) {
        let parsed = read_back(shape);
        assert!(
            subsumption::equivalent(shape, &parsed),
            "round trip not equivalent for {}:\n  original={:?}\n  parsed={:?}",
            shape.iri,
            shape,
            parsed
        );
    }

    #[test]
    fn round_trips_typed_failure_metadata_without_residue() {
        let shape = ValidationShapeIr::new(
            "https://ex/Shape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![],
            None,
        )
        .unwrap()
        .with_failure_class("https://ex/Failure")
        .unwrap();
        let parsed = read_back(&shape);
        assert_eq!(parsed.failure_class.as_deref(), Some("https://ex/Failure"));
    }

    #[test]
    fn duplicate_typed_failure_metadata_is_malformed() {
        let ttl = format!(
            "{HEADER}<https://ex/Shape> a sh:NodeShape ;\n\
             sh:targetClass <https://ex/C> ;\n\
             <{GMEOW_ENFORCES_FAILURE_CLASS}> <https://ex/FailureA>, <https://ex/FailureB> .\n"
        );
        let ds = parse_ttl(&ttl);
        let err = read_shacl_shape(&ds, "https://ex/Shape").unwrap_err();
        assert!(
            err.message()
                .contains("distinct gmeow:enforcesFailureClass")
        );
    }

    #[test]
    fn repeated_identical_typed_failure_metadata_is_one_value() {
        let ttl = format!(
            "{HEADER}<https://ex/Shape> a sh:NodeShape ;\n\
             sh:targetClass <https://ex/C> ;\n\
             <{GMEOW_ENFORCES_FAILURE_CLASS}> <https://ex/Failure>, <https://ex/Failure> .\n"
        );
        let ds = parse_ttl(&ttl);
        let read = read_shacl_shape(&ds, "https://ex/Shape").expect("identical values dedupe");
        assert_eq!(read.ir.failure_class.as_deref(), Some("https://ex/Failure"));
    }

    #[test]
    fn round_trips_cardinality_class_datatype_nodekind_in_not_qvs_reifier() {
        let p_card = PropertyConstraintIr::new(
            "https://ex/a",
            Some(1),
            Some(2),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        )
        .unwrap();
        let p_dt = PropertyConstraintIr::new(
            "https://ex/b",
            None,
            None,
            None,
            vec![ConstraintComponent::Datatype(
                "http://www.w3.org/2001/XMLSchema#string".into(),
            )],
        )
        .unwrap();
        let p_nk = PropertyConstraintIr::new(
            "https://ex/c",
            None,
            None,
            None,
            vec![ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri)],
        )
        .unwrap();
        let p_in = PropertyConstraintIr::new(
            "https://ex/d",
            None,
            None,
            None,
            vec![ConstraintComponent::In(vec![
                ShapeValue::Iri("https://ex/v1".into()),
                ShapeValue::Iri("https://ex/v2".into()),
                ShapeValue::Literal {
                    lexical: "plain".into(),
                    datatype: None,
                    lang: None,
                },
            ])],
        )
        .unwrap();
        let p_not = PropertyConstraintIr::new(
            "https://ex/e",
            None,
            None,
            None,
            vec![ConstraintComponent::Not(Box::new(
                ConstraintComponent::Class("https://ex/Disjoint".into()),
            ))],
        )
        .unwrap();
        let p_qvs = PropertyConstraintIr::new(
            "https://ex/f",
            None,
            None,
            None,
            vec![ConstraintComponent::QualifiedValueShape {
                shape: vec![ConstraintComponent::Class("https://ex/Q".into())],
                min: Some(1),
                max: None,
            }],
        )
        .unwrap();
        let p_reifier = PropertyConstraintIr::new("https://ex/g", None, None, None, vec![])
            .unwrap()
            .with_reifier(Some("https://ex/ReifierShape".into()), true)
            .unwrap();

        let shape = ValidationShapeIr::new(
            "https://ex/BigShape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![p_card, p_dt, p_nk, p_in, p_not, p_qvs, p_reifier],
            None,
        )
        .unwrap()
        .with_node_components(vec![
            ConstraintComponent::Class("https://ex/NodeClass".into()),
            ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri),
        ])
        .unwrap();
        assert_round_trips(&shape);
    }

    #[test]
    fn round_trips_numeric_range() {
        let shape = ValidationShapeIr::new(
            "https://ex/NumShape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/magnitude",
                    None,
                    None,
                    None,
                    vec![
                        ConstraintComponent::NumericRange {
                            min: Some(0.0),
                            max: Some(100.0),
                            min_inclusive: true,
                            max_inclusive: true,
                        },
                        ConstraintComponent::Datatype(
                            "http://www.w3.org/2001/XMLSchema#decimal".into(),
                        ),
                    ],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        assert_round_trips(&shape);
    }

    #[test]
    fn round_trips_has_value() {
        let shape = ValidationShapeIr::new(
            "https://ex/HvShape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    None,
                    None,
                    None,
                    vec![ConstraintComponent::HasValue(ShapeValue::Iri(
                        "https://ex/fixed".into(),
                    ))],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        assert_round_trips(&shape);
    }

    #[test]
    fn round_trips_inverse_path_and_domain_range_targets() {
        let inverse = ValidationShapeIr::new(
            "https://ex/InvShape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    None,
                    Some(1),
                    Some(ConstraintProvenance::OwlRestriction),
                    vec![],
                )
                .unwrap()
                .inverted(),
            ],
            None,
        )
        .unwrap();
        assert_round_trips(&inverse);

        let domain = ValidationShapeIr::new(
            "https://ex/DomainShape",
            ShapeTarget::SubjectsOf("https://ex/p".into()),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(vec![ConstraintComponent::Datatype(
            "http://www.w3.org/2001/XMLSchema#string".into(),
        )])
        .unwrap();
        assert_round_trips(&domain);

        let range = ValidationShapeIr::new(
            "https://ex/RangeShape",
            ShapeTarget::ObjectsOf("https://ex/p".into()),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(vec![ConstraintComponent::Class("https://ex/C".into())])
        .unwrap();
        assert_round_trips(&range);
    }

    #[test]
    fn presentation_message_and_severity_are_absorbed_and_projected_out() {
        // Every property carries sh:message + sh:severity; the read-back must be equivalent
        // to the SAME shape WITHOUT them (presentation is projected out of enforcement).
        let bare = PropertyConstraintIr::new(
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OptNative),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        )
        .unwrap();
        let decorated = bare
            .clone()
            .with_severity(ShaclSeverity::Warning)
            .with_message("every focus must have exactly one D")
            .unwrap();
        let with_pres = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![decorated],
            None,
        )
        .unwrap();
        let without_pres = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![bare],
            None,
        )
        .unwrap();

        let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&with_pres));
        assert!(
            ttl.contains("sh:message") && ttl.contains("sh:severity"),
            "{ttl}"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").unwrap();
        assert!(read.unsupported.is_empty(), "{:?}", read.unsupported);
        assert!(
            subsumption::equivalent(&read.ir, &without_pres),
            "presentation must be projected out: {:?} vs {:?}",
            read.ir,
            without_pres
        );
    }

    #[test]
    fn presentation_annotation_predicates_are_skipped_not_residue() {
        // rdfs:label / sh:name / sh:description / sh:order / skos:* are pure annotation.
        let ttl = format!(
            "{HEADER}@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             <https://ex/S> a sh:NodeShape ;\n    \
             rdfs:label \"a label\" ;\n    \
             sh:name \"a name\" ;\n    \
             sh:description \"a description\" ;\n    \
             sh:order 3 ;\n    \
             skos:note \"a note\" ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:property [ sh:path <https://ex/p> ; sh:class <https://ex/D> ] .\n"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").unwrap();
        assert!(
            read.unsupported.is_empty(),
            "annotation predicates must NOT be residue: {:?}",
            read.unsupported
        );
        assert_eq!(read.ir.properties.len(), 1);
    }

    #[test]
    fn mixed_covered_and_sparql_yields_covered_ir_plus_residue_without_err() {
        // A node shape mixing a covered property (sh:class) with an uncovered sh:sparql
        // constraint AND an sh:or must yield the covered fragment PLUS a residue list.
        let ttl = format!(
            "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:property [ sh:path <https://ex/p> ; sh:class <https://ex/D> ] ;\n    \
             sh:or ( [ sh:class <https://ex/A> ] [ sh:class <https://ex/B> ] ) ;\n    \
             sh:sparql [ sh:select \"SELECT ?this WHERE {{ ?this ?p ?o }}\" ] .\n"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
            .expect("a mixed shape must NOT Err — it yields covered + residue");
        // The covered fragment survived: one property carrying the Class component.
        assert_eq!(read.ir.properties.len(), 1);
        assert_eq!(
            read.ir.properties[0].components,
            vec![ConstraintComponent::Class("https://ex/D".into())]
        );
        // The residue carries the uncovered sh:or and sh:sparql (sorted, deterministic).
        assert!(
            read.unsupported.iter().any(|u| u.contains("shacl#or")),
            "residue must flag sh:or: {:?}",
            read.unsupported
        );
        assert!(
            read.unsupported.iter().any(|u| u.contains("shacl#sparql")),
            "residue must flag sh:sparql: {:?}",
            read.unsupported
        );
        // The oracle carries the residue through and marks the class residue-bearing.
        let projected = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    None,
                    None,
                    None,
                    vec![ConstraintComponent::Class("https://ex/D".into())],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let verdict = oracle(&read, &projected);
        assert!(
            verdict.equivalent,
            "covered fragments must match: {}",
            verdict.reason
        );
        assert!(
            verdict.residue_bearing && !verdict.unsupported.is_empty(),
            "a residue-bearing legacy class is not deletable on the covered match alone: {verdict:?}"
        );
    }

    #[test]
    fn unparseable_sparql_target_is_residue_not_err() {
        // An sh:SPARQLTarget whose select is NOT the value-keyed single-triple form (here two
        // distinct non-type patterns) cannot be inverted → routed to residue, never a hard error.
        // A covered sh:targetClass supplies the focus selector, so the read still succeeds.
        let ttl = format!(
            "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:target [ a sh:SPARQLTarget ; sh:select \"SELECT ?this WHERE {{ ?this <https://ex/k> <https://ex/v> ; <https://ex/k2> <https://ex/v2> }}\" ] .\n"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
            .expect("an unparsable sh:SPARQLTarget alongside a covered target is residue, not Err");
        assert_eq!(read.ir.target, ShapeTarget::Class("https://ex/C".into()));
        assert!(
            read.unsupported.iter().any(|u| u.contains("shacl#target")),
            "the unparsable sh:target must be residue: {:?}",
            read.unsupported
        );
    }

    #[test]
    fn node_level_or_over_min_one_property_branches_reads_as_or_properties() {
        // The exact `sh:or ( [ sh:path P ; sh:minCount 1 ] … )` form is COVERED — it reads into
        // an OrProperties node component (and round-trips the emitter), never residue.
        let ttl = format!(
            "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:or ( [ sh:path <https://ex/frame> ; sh:minCount 1 ] \
                     [ sh:path <https://ex/model> ; sh:minCount 1 ] ) .\n"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").expect("read ok");
        assert!(read.unsupported.is_empty(), "{:?}", read.unsupported);
        assert!(
            read.ir.node_components.iter().any(|c| matches!(
                c,
                ConstraintComponent::OrProperties(paths)
                    if paths == &vec![
                        "https://ex/frame".to_owned(),
                        "https://ex/model".to_owned()
                    ]
            )),
            "{:?}",
            read.ir.node_components
        );
        // Round-trip: emit the read IR and compare with a directly-built projected twin.
        let projected = ValidationShapeIr::new(
            "https://ex/C-shape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(vec![ConstraintComponent::OrProperties(vec![
            "https://ex/model".into(),
            "https://ex/frame".into(),
        ])])
        .unwrap();
        let verdict = oracle(&read, &projected);
        assert!(verdict.equivalent, "{}", verdict.reason);
    }

    #[test]
    fn node_level_or_with_value_branches_stays_residue() {
        // A branch carrying anything beyond `sh:path` + `sh:minCount 1` (here sh:class) is NOT
        // the covered property-alternatives form — the whole sh:or stays residue as before.
        let ttl = format!(
            "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:or ( [ sh:path <https://ex/frame> ; sh:minCount 1 ] \
                     [ sh:path <https://ex/dom> ; sh:minCount 1 ; sh:class <https://ex/PS> ] ) .\n"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").expect("read ok");
        assert!(
            read.unsupported
                .iter()
                .any(|u| u == "http://www.w3.org/ns/shacl#or"),
            "{:?}",
            read.unsupported
        );
        assert!(
            !read
                .ir
                .node_components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::OrProperties(_))),
            "{:?}",
            read.ir.node_components
        );
    }

    #[test]
    fn reader_errs_on_a_missing_shape_iri() {
        let ds = parse_ttl(&format!(
            "{HEADER}<https://ex/S> a sh:NodeShape ; sh:targetClass <https://ex/C> .\n"
        ));
        let err = read_shacl_shape(&ds, "https://ex/DoesNotExist")
            .expect_err("a non-existent shape IRI is genuine malformation");
        assert!(
            err.to_string().contains("not present in the graph"),
            "{err}"
        );
    }

    #[test]
    fn oracle_reports_equivalent_for_the_round_trip() {
        let shape = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    Some(1),
                    Some(1),
                    Some(ConstraintProvenance::OwlRestriction),
                    vec![ConstraintComponent::Class("https://ex/D".into())],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&shape));
        let legacy = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").unwrap();
        let verdict = oracle(&legacy, &shape);
        assert!(verdict.equivalent, "{}", verdict.reason);
        assert!(
            verdict.legacy_subsumed_by_projected,
            "equivalent ⇒ projected ⊒ legacy"
        );
        assert!(
            !verdict.residue_bearing,
            "a clean round trip has no residue"
        );
        assert!(verdict.reason.contains("equivalent"));
    }

    #[test]
    fn oracle_reports_not_equivalent_with_a_meaningful_reason_when_a_component_is_missing() {
        let legacy_ir = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    None,
                    None,
                    None,
                    vec![
                        ConstraintComponent::Class("https://ex/D".into()),
                        ConstraintComponent::MinLength(3),
                    ],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let legacy = ShapeRead {
            ir: legacy_ir,
            unsupported: vec![],
        };
        // The projected shape drops the MinLength(3) component — strictly weaker.
        let projected = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    None,
                    None,
                    None,
                    vec![ConstraintComponent::Class("https://ex/D".into())],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let verdict = oracle(&legacy, &projected);
        assert!(!verdict.equivalent, "a dropped component ⇒ not equivalent");
        assert!(
            verdict.reason.contains("not equivalent") && verdict.reason.contains("path="),
            "the reason must name the differing property path: {}",
            verdict.reason
        );
        assert!(!verdict.legacy_subsumed_by_projected);
    }

    #[test]
    fn cross_check_is_green_when_the_two_graphs_are_the_same_shape() {
        let shape = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    Some(1),
                    Some(1),
                    Some(ConstraintProvenance::OptNative),
                    vec![ConstraintComponent::Datatype(
                        "http://www.w3.org/2001/XMLSchema#string".into(),
                    )],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&shape));
        let witnesses = witnesses_for(&shape);
        assert!(
            !witnesses.is_empty(),
            "a cardinality+datatype shape must yield witnesses"
        );
        let data = parse_ttl("<https://ex/x> <https://ex/q> <https://ex/y> .\n");
        cross_check(&ttl, &ttl, &data, &witnesses)
            .expect("identical shape graphs must produce identical findings");
    }

    #[test]
    fn cross_check_hard_fails_vacuous_when_no_focus_node_is_exercised() {
        let shape = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    Some(1),
                    Some(1),
                    Some(ConstraintProvenance::OptNative),
                    vec![],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&shape));
        let data = parse_ttl("<https://ex/x> <https://ex/q> <https://ex/y> .\n");
        let err = cross_check(&ttl, &ttl, &data, &[])
            .expect_err("a run that exercises no focus node must HARD-FAIL as vacuous");
        let err = err.to_string();
        assert!(
            err.contains("vacuous") && err.contains("0 focus nodes"),
            "{err}"
        );
    }

    /// Comparison-only guard (Principle 4): this module reads bytes and returns verdicts.
    /// No function here — `read_shacl_shape`, `oracle`, `witnesses_for`, `cross_check` —
    /// writes to `slices/**` or the `logic:` canon; the reader NEVER parses SHACL back
    /// into the authoring ground. (Asserted structurally: none of these signatures take a
    /// writable sink or a path, and the module imports no filesystem writer.)
    #[test]
    fn module_is_comparison_only_no_canon_writeback() {
        let shape = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![],
            None,
        )
        .unwrap();
        let parsed = read_back(&shape);
        assert!(subsumption::equivalent(&shape, &parsed));
    }

    #[test]
    fn value_keyed_sparql_target_round_trips() {
        // The projector's `sh:target [ a sh:SPARQLTarget ; sh:select "…" ]` inverts back to the
        // same ValueKeyed target, with NO residue (the exact single-triple form).
        let shape = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::ValueKeyed {
                predicate: "https://ex/kind".into(),
                value: "https://ex/Bp".into(),
            },
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    Some(1),
                    None,
                    Some(ConstraintProvenance::OwlRestriction),
                    vec![],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let read = read_back(&shape);
        assert_eq!(
            read.target,
            ShapeTarget::ValueKeyed {
                predicate: "https://ex/kind".into(),
                value: "https://ex/Bp".into()
            }
        );
        assert!(subsumption::equivalent(&shape, &read));
    }

    #[test]
    fn sparql_target_with_type_pattern_parses_value_key_and_flags_type() {
        // A hand-authored mode-scoped select (`?this a Commitment ; mode abd`) clears the read
        // error: the value-key becomes the target, and the extra `a` type pattern is flagged as
        // residue (the IR value-keyed target cannot hold a class).
        let ttl = format!(
            "{HEADER}\
             <https://ex/AbShape> a sh:NodeShape ;\n\
             \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
             PREFIX g: <https://ex/>\n\
             SELECT ?this WHERE {{ ?this a g:Commitment ; g:mode g:abd . }}\n\
             \"\"\" ] ;\n\
             \x20\x20sh:property [ sh:path <https://ex/explanandum> ; sh:minCount 1 ] .\n"
        );
        let ds = parse_ttl(&ttl);
        let read = read_shacl_shape(&ds, "https://ex/AbShape")
            .unwrap_or_else(|e| panic!("read must not error: {e}\n{ttl}"));
        assert_eq!(
            read.ir.target,
            ShapeTarget::ValueKeyed {
                predicate: "https://ex/mode".into(),
                value: "https://ex/abd".into()
            },
            "value-key extracted from the select"
        );
        assert!(
            read.unsupported
                .iter()
                .any(|u| u.contains("g:Commitment") || u.contains("Commitment")),
            "the extra `a Commitment` type pattern must be flagged: {:?}",
            read.unsupported
        );
    }

    #[test]
    fn inline_sh_node_helper_adopts_owner_target() {
        // A targetless helper shape referenced via a property's `sh:node` adopts the owning node
        // shape's `sh:targetClass` (the FramedIntervalShape fix): no read error, real target.
        let ttl = format!(
            "{HEADER}\
             <https://ex/OwnerShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Owner> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/interval> ; sh:node <https://ex/HelperShape> ] .\n\
             <https://ex/HelperShape> a sh:NodeShape ;\n\
             \x20\x20sh:property [ sh:path <https://ex/frame> ; sh:minCount 1 ] .\n"
        );
        let ds = parse_ttl(&ttl);
        let read = read_shacl_shape(&ds, "https://ex/HelperShape").unwrap_or_else(|e| {
            panic!("targetless helper must adopt owner target, got: {e}\n{ttl}")
        });
        assert_eq!(
            read.ir.target,
            ShapeTarget::Class("https://ex/Owner".into())
        );
    }

    #[test]
    fn tokenize_select_keeps_iris_and_string_literals_whole() {
        let toks =
            tokenize_select("SELECT ?this WHERE { ?this <https://ex/a.b/c> <https://ex/v> }");
        assert!(toks.contains(&"<https://ex/a.b/c>".to_owned()), "{toks:?}");
        assert!(
            !toks.contains(&".".to_owned()),
            "no bare dot inside the IRI: {toks:?}"
        );
    }
}
