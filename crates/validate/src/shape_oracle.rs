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

/// The sentinel `sh:select` a TRULY TARGETLESS top-level shape reads to. SHACL gives a shape
/// with no target an EMPTY focus set (it validates nothing), and this select provably binds no
/// `?this`, so [`ShapeTarget::Sparql`] over it is the faithful IR of "no focus nodes" — a
/// verdictable read for a documentation-only marker block instead of a hard read error.
pub const TARGETLESS_SELECT: &str = "SELECT ?this WHERE { FILTER(false) }";

/// The residue marker recorded when a raw (non-value-keyed) `sh:SPARQLTarget` select is adopted
/// as the shape's [`ShapeTarget::Sparql`] target. The select has no OWL/RDFS antecedent, so the
/// WHOLE shape is uncovered residue until an exact `logic:formalizes` record (plus the structural
/// witness cross-check) grounds it.
pub const RAW_SPARQL_TARGET_RESIDUE: &str =
    "http://www.w3.org/ns/shacl#target (raw sh:SPARQLTarget select)";

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
    /// The covered fragment as an enforcement-comparable IR (under the FIRST focus selector in
    /// canonical target order when the shape authored several).
    pub ir: ValidationShapeIr,
    /// The genuinely-uncovered constructs found on the shape (full predicate IRIs),
    /// sorted and de-duplicated. Empty ⇒ the shape is fully covered.
    pub unsupported: Vec<String>,
    /// The ADDITIONAL focus selectors of a multi-target shape (SHACL unions the focus sets of
    /// multi-valued `sh:targetClass` / `sh:targetSubjectsOf` / …), in canonical target order.
    /// The SAME constraint payload applies to each, so a deletion is clear only when EVERY
    /// target's obligation is reproduced — the caller must judge each one. Empty for the
    /// ordinary single-target shape.
    pub extra_targets: Vec<ShapeTarget>,
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
            // `sh:uniqueLang true` is the per-property unique-language facet (no two
            // language-tagged values share a tag); `false` is the vacuous default.
            "uniqueLang" => {
                if obj_lexical(ds, obj, &ctx)? == "true" {
                    self.comps.push(ConstraintComponent::UniqueLang);
                }
            }
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
            // `sh:minCount 0` is a no-op (every focus has at least zero values), so it is
            // normalized to "no minimum" — otherwise a legacy shape that spelled the vacuous
            // bound explicitly would fail to match a projection that (correctly) omits it.
            Some("minCount") => {
                let n = obj_u32(ds, obj, &ctx)?;
                min_count = (n > 0).then_some(n);
            }
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

/// Conjunctively merge property shapes that share the same `(path, inverse)` selector, mirroring
/// the frontend's `merge_same_path_properties`. A hand-authored legacy shape that splits an
/// exactly-one obligation across two `sh:property` blocks (`[ sh:path P ; sh:minCount 1 ; sh:class C ]`
/// and `[ sh:path P ; sh:maxCount 1 ; sh:class C ]`) states the SAME constraint the projector emits
/// as one merged block; reading them unmerged would spuriously red the equivalence oracle (a legacy
/// `min=1,max=None` block never matches the merged projected `min=1,max=1`). The merged cardinality
/// is the tightest of the group's bounds (max of mins, min of maxes) and the union of value
/// components; first-seen path order is preserved for determinism.
fn merge_same_path_property_shapes(
    props: Vec<PropertyConstraintIr>,
) -> gmeow_errors::Result<Vec<PropertyConstraintIr>> {
    let mut order: Vec<(String, bool)> = Vec::new();
    let mut groups: std::collections::BTreeMap<(String, bool), Vec<PropertyConstraintIr>> =
        std::collections::BTreeMap::new();
    for p in props {
        let key = (p.path.clone(), p.inverse);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(p);
    }
    let mut out = Vec::with_capacity(order.len());
    for key in order {
        let mut group = groups.remove(&key).expect("key present");
        if group.len() == 1 {
            out.push(group.pop().expect("one element"));
            continue;
        }
        let mut min_count: Option<u32> = None;
        let mut max_count: Option<u32> = None;
        let mut components: Vec<ConstraintComponent> = Vec::new();
        let mut severity: Option<ShaclSeverity> = None;
        let mut message: Option<String> = None;
        let mut reifier_shape: Option<String> = None;
        let mut reification_required = false;
        for p in &group {
            min_count = match (min_count, p.min_count) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            max_count = match (max_count, p.max_count) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            for c in &p.components {
                if !components.contains(c) {
                    components.push(c.clone());
                }
            }
            severity = severity.or(p.severity);
            message = message.or_else(|| p.message.clone());
            reifier_shape = reifier_shape.or_else(|| p.reifier_shape.clone());
            reification_required = reification_required || p.reification_required;
        }
        let provenance =
            (min_count.is_some() || max_count.is_some()).then_some(ConstraintProvenance::OptNative);
        let (path, inverse) = key;
        let mut merged =
            PropertyConstraintIr::new(path, min_count, max_count, provenance, components)?;
        if inverse {
            merged = merged.inverted();
        }
        if let Some(sev) = severity {
            merged = merged.with_severity(sev);
        }
        if let Some(msg) = message
            && !msg.trim().is_empty()
        {
            merged = merged.with_message(msg)?;
        }
        if reifier_shape.is_some() || reification_required {
            merged = merged.with_reifier(reifier_shape, reification_required)?;
        }
        out.push(merged);
    }
    Ok(out)
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

/// The three ways an `sh:target` blank node reads: a value-keyed select (a covered
/// [`ShapeTarget::ValueKeyed`]), a raw select (routed to [`ShapeTarget::Sparql`] when the shape
/// has no direct focus target — whole-shape residue), or an unreadable target construct (already
/// recorded in the residue list by the parser).
enum SparqlTargetRead {
    /// The exact value-keyed single-triple select form.
    ValueKeyed(ShapeTarget),
    /// A raw `SELECT ?this WHERE { … }` body with no value-keyed inversion.
    Raw(String),
    /// Not a readable `sh:SPARQLTarget` at all (residue already recorded).
    Unreadable,
}

/// Invert an `sh:target [ a sh:SPARQLTarget ; sh:select "…" ]` blank node. The exact value-keyed
/// single-triple form becomes a [`ShapeTarget::ValueKeyed`]; any other select is returned RAW so
/// the caller can either adopt it as the shape's [`ShapeTarget::Sparql`] target (no direct focus
/// target authored) or record it as residue beside a direct target. A non-`sh:SPARQLTarget`
/// target or a target with no `sh:select` is routed to `unsupported` (never a hard error). When a
/// value-keyed select carries extra `?this a <Class>` type patterns (which the IR value-keyed
/// target cannot hold), those are surfaced to `unsupported` too.
fn parse_sparql_target(
    ds: &RdfDataset,
    obj: TermId,
    ctx: &str,
    unsupported: &mut Vec<String>,
) -> gmeow_errors::Result<SparqlTargetRead> {
    let inner = quads_of(ds, obj);
    let is_sparql_target = inner.iter().any(|(p, o)| {
        p == RDF_TYPE && matches!(ds.resolve(*o), TermRef::Iri(s) if s == SH_SPARQLTARGET)
    });
    if !is_sparql_target {
        unsupported.push(format!("{SH}target (non-SPARQLTarget target)"));
        return Ok(SparqlTargetRead::Unreadable);
    }
    let Some((_, sel)) = inner.iter().find(|(p, _)| shacl_local(p) == Some("select")) else {
        unsupported.push(format!("{SH}target (sh:SPARQLTarget without sh:select)"));
        return Ok(SparqlTargetRead::Unreadable);
    };
    let select = obj_lexical(ds, *sel, ctx)?;
    match parse_value_key_select(&select) {
        Some((predicate, value, type_classes)) => {
            for c in type_classes {
                unsupported.push(format!(
                    "sh:SPARQLTarget additional type constraint ?this a <{c}>"
                ));
            }
            Ok(SparqlTargetRead::ValueKeyed(ShapeTarget::ValueKeyed {
                predicate,
                value,
            }))
        }
        None => Ok(SparqlTargetRead::Raw(select)),
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
                match parse_sparql_target(ds, o, "owner-walk", &mut sink) {
                    Ok(SparqlTargetRead::ValueKeyed(t)) => return Some(t),
                    Ok(SparqlTargetRead::Raw(select)) => return Some(ShapeTarget::Sparql(select)),
                    Ok(SparqlTargetRead::Unreadable) | Err(_) => {}
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
    // ALL the direct focus selectors, in canonical target order after the loop. SHACL unions the
    // focus sets of multi-valued `sh:targetClass`/`sh:targetSubjectsOf`/… targets, so a
    // multi-target shape reads as ONE constraint payload under SEVERAL selectors (the first
    // becomes `ir.target`, the rest ride [`ShapeRead::extra_targets`]).
    let mut targets: Vec<ShapeTarget> = Vec::new();
    let mut raw_sparql: Option<String> = None;
    let mut properties: Vec<PropertyConstraintIr> = Vec::new();
    let mut node_acc = CompAcc::new();
    let mut unsupported: Vec<String> = Vec::new();
    let mut failure_classes = BTreeSet::new();

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
            Some("targetClass") => {
                targets.push(ShapeTarget::Class(obj_iri(graph, obj, node_shape_iri)?));
            }
            Some("targetSubjectsOf") => {
                targets.push(ShapeTarget::SubjectsOf(obj_iri(
                    graph,
                    obj,
                    node_shape_iri,
                )?));
            }
            Some("targetObjectsOf") => {
                targets.push(ShapeTarget::ObjectsOf(obj_iri(graph, obj, node_shape_iri)?));
            }
            // An `sh:target [ a sh:SPARQLTarget ; sh:select "…" ]`. A recognizable value-keyed
            // select becomes a `ValueKeyed` target; a raw select is held aside — it becomes the
            // shape's `ShapeTarget::Sparql` target only when no direct focus target is authored
            // (otherwise it stays residue beside the direct target, exactly as before). An
            // unreadable target construct is routed to the residue list (never a hard error).
            Some("target") => {
                match parse_sparql_target(graph, obj, node_shape_iri, &mut unsupported)? {
                    SparqlTargetRead::ValueKeyed(t) => targets.push(t),
                    SparqlTargetRead::Raw(select) => {
                        if raw_sparql.is_some() {
                            // A second raw select cannot be the (single) focus selector too.
                            unsupported.push(format!(
                                "{SH}target sh:select (not the value-keyed single-triple form)"
                            ));
                        } else {
                            raw_sparql = Some(select);
                        }
                    }
                    SparqlTargetRead::Unreadable => {}
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
    // Resolve the focus selectors. Multi-valued direct targets sort into canonical order (the
    // union semantics is order-independent; the first is `ir.target`, the rest are
    // `extra_targets`). A raw `sh:SPARQLTarget` select beside a direct target stays residue (the
    // direct target is the selector, exactly as before); with NO direct target it becomes the
    // shape's `ShapeTarget::Sparql` target and marks the WHOLE shape residue (the select has no
    // OWL/RDFS antecedent). A targetless inline / `sh:node` / property-only helper shape adopts
    // the owning node shape's target by walking the inverse `sh:node` / `sh:property` edge. A
    // top-level shape that carries an UNREADABLE target construct (`sh:targetNode`, an
    // `sh:SPARQLTarget` without a select, …) and no other selector remains an `Err` — its focus
    // set is authored but not representable. Only a TRULY targetless shape (no target construct
    // at all — a documentation-only marker) reads to the empty-focus [`TARGETLESS_SELECT`]
    // sentinel, because SHACL gives such a shape an empty focus set: it enforces nothing.
    targets.sort();
    targets.dedup();
    let mut extra_targets = targets;
    let target = if extra_targets.is_empty() {
        if let Some(select) = raw_sparql {
            unsupported.push(RAW_SPARQL_TARGET_RESIDUE.to_owned());
            ShapeTarget::Sparql(select)
        } else if let Some(t) = target_via_owner(graph, subject) {
            t
        } else if unsupported.iter().any(|u| u.contains("shacl#target")) {
            return Err(parse_err(format!(
                "read_shacl_shape: <{node_shape_iri}> has no sh:targetClass / \
                 sh:targetSubjectsOf / sh:targetObjectsOf and no owning sh:node/sh:property \
                 shape to adopt a target from"
            )));
        } else {
            ShapeTarget::Sparql(TARGETLESS_SELECT.to_owned())
        }
    } else {
        if raw_sparql.is_some() {
            unsupported.push(format!(
                "{SH}target sh:select (not the value-keyed single-triple form)"
            ));
        }
        extra_targets.remove(0)
    };
    let node_components = node_acc.finish(node_shape_iri)?;
    let properties = merge_same_path_property_shapes(properties)?;
    let mut ir = ValidationShapeIr::new(node_shape_iri, target, properties, None)?
        .with_node_components(node_components)?;
    if let Some(failure_class) = failure_classes.first() {
        ir = ir.with_failure_class(failure_class)?;
    }

    unsupported.sort();
    unsupported.dedup();
    Ok(ShapeRead {
        ir,
        unsupported,
        extra_targets,
    })
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
            | ConstraintComponent::UniqueLang
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
        | ConstraintComponent::UniqueLang
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

// ── Part C extension: semantic residue witnesses (sh:node / sh:xone / raw SPARQL targets) ──

/// A semantic near-miss (or near-hit): a fresh focus node, the Turtle triples that put it in the
/// shape's focus set and make it violate (or conform to) one construct, and the EXPECTED verdict.
/// Unlike [`Witness`] (which compares full finding sets between two SHACL-Core-comparable
/// graphs), a semantic witness compares only whether each side FLAGS the focus node — the legacy
/// residue construct (`sh:node` / `sh:xone` / a raw-SPARQL-target block's structural property
/// constraints) and its projected `logic:formalizes` record report through different SHACL
/// component vocabularies, so focus-flag agreement is the comparable surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticWitness {
    /// The construct this witness discriminates (e.g. `sh:xone: two alternatives present`).
    pub label: String,
    /// The fresh focus-node IRI the witness introduces.
    pub focus: String,
    /// The Turtle triples (absolute IRIs, no prefixes) forming the witness data graph.
    pub triples: String,
    /// Whether BOTH the legacy shape and the projected record must flag `focus`.
    pub expect_flagged: bool,
}

/// The witness plan for one legacy shape, split by what each witness exercises so the caller can
/// scope the cross-check: `covered` witnesses exercise the covered property fragment (needed when
/// the shape has NO declarative projected peer — a raw-SPARQL-target block — so the record must
/// reproduce it), `residue` witnesses exercise the `sh:node` / `sh:xone` constructs, and
/// `conforming` witnesses must be flagged by NEITHER side (the guard against a record that flags
/// everything).
#[derive(Debug, Clone, Default)]
pub struct SemanticWitnessPlan {
    /// Witnesses that must be flagged by neither side.
    pub conforming: Vec<SemanticWitness>,
    /// Near-misses of the covered property fragment (cardinality / class / datatype / nodeKind).
    pub covered: Vec<SemanticWitness>,
    /// Near-misses of the machine-readable residue constructs (`sh:node` / `sh:xone`).
    pub residue: Vec<SemanticWitness>,
}

/// How to make a fresh witness focus a member of the shape's focus set.
enum FocusMembership {
    /// A directly-invertible target ([`target_triples`]).
    Target(ShapeTarget),
    /// The conservative skeleton of a raw `sh:SPARQLTarget` select: the focus must carry these
    /// `rdf:type` classes, (when present) sit under this IRI namespace, and (when present) be the
    /// OBJECT of these predicates from a subject of the paired type (`subject a T ; P focus` — an
    /// object-of-property membership like a deception cue). Parsed ONLY to mint focus-MEMBERSHIP
    /// triples — the select body is never used to derive constraint semantics, and an unparsable
    /// select yields no membership at all (clearance is then impossible: the witness cross-check
    /// hard-fails rather than passing vacuously).
    Skeleton {
        namespace: Option<String>,
        types: Vec<String>,
        /// `(subject rdf:type, predicate)` edges: a subject of the given type links to the focus
        /// via the predicate (`?s a T . ?s P ?this`). `None` type ⇒ an untyped subject.
        object_of: Vec<(Option<String>, String)>,
    },
}

impl FocusMembership {
    /// Derive the membership synthesizer from a shape target, or `Err` when no witness focus can
    /// be synthesized (a targetless shape has an empty focus set; an out-of-skeleton select is
    /// opaque).
    fn from_target(target: &ShapeTarget) -> gmeow_errors::Result<Self> {
        match target {
            ShapeTarget::Sparql(select) if select == TARGETLESS_SELECT => Err(parse_err(
                "semantic witnesses: a targetless shape has an EMPTY focus set — nothing to \
                 witness"
                    .to_owned(),
            )),
            ShapeTarget::Sparql(select) => {
                let (namespace, types, object_of) =
                    parse_target_skeleton(select).ok_or_else(|| {
                        parse_err(format!(
                            "semantic witnesses: the sh:SPARQLTarget select is outside the \
                             machine-readable skeleton (type patterns + STRSTARTS namespace \
                             filter + object-of-property membership); cannot synthesize focus \
                             membership: {select}"
                        ))
                    })?;
                Ok(FocusMembership::Skeleton {
                    namespace,
                    types,
                    object_of,
                })
            }
            other => Ok(FocusMembership::Target(other.clone())),
        }
    }

    /// Mint a fresh focus IRI plus the triples that put it in the focus set.
    fn mint(&self, idx: &mut usize) -> (String, String) {
        let n = *idx;
        *idx += 1;
        match self {
            FocusMembership::Target(t) => {
                let focus = format!("https://gmeow.example/witness/{n}");
                let triples = target_triples(t, &focus);
                (focus, triples)
            }
            FocusMembership::Skeleton {
                namespace,
                types,
                object_of,
            } => {
                let focus = format!(
                    "{}gmeow-witness-{n}",
                    namespace
                        .as_deref()
                        .unwrap_or("https://gmeow.example/witness/")
                );
                let mut triples = types
                    .iter()
                    .map(|c| format!("<{focus}> <{RDF_TYPE}> <{c}> .\n"))
                    .collect::<String>();
                // Object-of membership: mint a typed subject linking to the focus.
                for (k, (subj_type, pred)) in object_of.iter().enumerate() {
                    let subj = format!("{focus}/target-subject-{k}");
                    if let Some(t) = subj_type {
                        triples.push_str(&format!("<{subj}> <{RDF_TYPE}> <{t}> .\n"));
                    }
                    triples.push_str(&format!("<{subj}> <{pred}> <{focus}> .\n"));
                }
                (focus, triples)
            }
        }
    }
}

/// Collect the `PREFIX name: <iri>` declarations across a tokenized select.
fn select_prefixes(toks: &[String]) -> std::collections::BTreeMap<String, String> {
    let mut prefixes = std::collections::BTreeMap::new();
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
    prefixes
}

/// The end index (exclusive) of the balanced-paren token range starting at `open` (which must be
/// a `(` token), or `None` when unbalanced.
fn paren_close(toks: &[String], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate().skip(open) {
        match t.as_str() {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The membership skeleton of a raw `sh:SPARQLTarget` select: `(namespace filter on ?this,
/// rdf:type classes ?this must carry, object-of edges `(subject type, predicate)`)`.
type TargetSkeleton = (Option<String>, Vec<String>, Vec<(Option<String>, String)>);

/// Conservatively parse a raw `sh:SPARQLTarget` select into a MEMBERSHIP skeleton:
/// `(namespace filter on ?this, rdf:type classes ?this must carry, object-of edges)`. Accepts
/// EXACTLY the closed grammar the corpus meta-shapes author — `?this a <C> .`, `?this a ?v .`
/// with a `FILTER(?v IN (<C1>, <C2>, …))` domain (the first class is chosen),
/// `FILTER(STRSTARTS(STR(?this), "ns"))`, and OBJECT-OF membership `?s a <T> . ?s <P> ?this .`
/// (the focus is the object of `P` from a subject of type `T`, e.g. a deception cue) — and returns
/// `None` for ANYTHING else, so an opaque select can never mint a false focus member (the caller
/// then hard-fails, it never clears).
fn parse_target_skeleton(select: &str) -> Option<TargetSkeleton> {
    let toks = tokenize_select(select);
    let prefixes = select_prefixes(&toks);
    let resolve = |t: &str| -> Option<String> {
        if let Some(inner) = t.strip_prefix('<').and_then(|x| x.strip_suffix('>')) {
            return Some(inner.to_owned());
        }
        let (label, local) = t.split_once(':')?;
        prefixes.get(label).map(|ns| format!("{ns}{local}"))
    };
    let open = toks.iter().position(|t| t == "{")?;
    let close = toks.iter().rposition(|t| t == "}")?;
    if close <= open {
        return None;
    }
    let body = &toks[open + 1..close];
    let mut types: Vec<String> = Vec::new();
    let mut type_vars: Vec<String> = Vec::new();
    // Non-focus subject variables typed by an IRI class (`?s a <T>`), and object-of edges
    // (`?s <P> ?this`) — resolved against the subject types after the body is scanned.
    let mut subj_types: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut object_edges: Vec<(String, String)> = Vec::new();
    let mut var_domains: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut namespace: Option<String> = None;
    let mut i = 0;
    while i < body.len() {
        let t = &body[i];
        if t == "." {
            i += 1;
            continue;
        }
        if t.eq_ignore_ascii_case("filter") {
            // `FILTER NOT EXISTS { … }` is a subclass-exclusion refinement (a direct-type
            // guard: exclude a focus that is ALSO typed a proper subclass of the target). A
            // fresh witness typed EXACTLY the target class satisfies it, so it never changes
            // focus membership — skip the braced block wholesale.
            if body.get(i + 1).map(|s| s.eq_ignore_ascii_case("not")) == Some(true)
                && body.get(i + 2).map(|s| s.eq_ignore_ascii_case("exists")) == Some(true)
                && body.get(i + 3).map(String::as_str) == Some("{")
            {
                let mut depth = 0usize;
                let mut j = i + 3;
                loop {
                    match body.get(j).map(String::as_str) {
                        Some("{") => depth += 1,
                        Some("}") => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        None => return None,
                        _ => {}
                    }
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            if body.get(i + 1).map(String::as_str) != Some("(") {
                return None;
            }
            let end = paren_close(body, i + 1)?;
            let inner = &body[i + 2..end - 1];
            // FILTER(STRSTARTS(STR(?this), "ns"))
            if inner.len() == 9
                && inner[0].eq_ignore_ascii_case("strstarts")
                && inner[1] == "("
                && inner[2].eq_ignore_ascii_case("str")
                && inner[3] == "("
                && inner[4] == "?this"
                && inner[5] == ")"
                && inner[6] == ","
                && inner[7].starts_with('"')
                && inner[8] == ")"
            {
                namespace = Some(inner[7].trim_matches('"').to_owned());
            }
            // FILTER(?v IN (<C1>, <C2>, …))
            else if inner.len() >= 4
                && inner[0].starts_with('?')
                && inner[1].eq_ignore_ascii_case("in")
                && inner[2] == "("
                && inner[inner.len() - 1] == ")"
            {
                let mut classes = Vec::new();
                for m in inner[3..inner.len() - 1].iter() {
                    if m == "," {
                        continue;
                    }
                    classes.push(resolve(m)?);
                }
                if classes.is_empty() {
                    return None;
                }
                var_domains.insert(inner[0].clone(), classes);
            } else {
                return None;
            }
            i = end;
            continue;
        }
        // A triple pattern `?S <pred> <obj>` with `?S` a variable subject.
        if t.starts_with('?') {
            let (Some(p), Some(o)) = (body.get(i + 1), body.get(i + 2)) else {
                return None;
            };
            // `?S a <C>` / `?S a ?v` — a type triple.
            if p == "a" {
                if t == "?this" {
                    if let Some(var) = o.strip_prefix('?') {
                        type_vars.push(format!("?{var}"));
                    } else {
                        types.push(resolve(o)?);
                    }
                } else if o.starts_with('?') {
                    // A non-focus subject typed by a variable is out of skeleton.
                    return None;
                } else {
                    subj_types.insert(t.clone(), resolve(o)?);
                }
                i += 3;
                continue;
            }
            // `?S <P> ?this` — an object-of membership edge (P an IRI predicate, focus the object).
            if o == "?this" && p != "?this" {
                object_edges.push((t.clone(), resolve(p)?));
                i += 3;
                continue;
            }
            return None;
        }
        return None;
    }
    for v in type_vars {
        let domain = var_domains.remove(&v)?;
        types.push(domain.into_iter().next().expect("non-empty domain"));
    }
    let object_of: Vec<(Option<String>, String)> = object_edges
        .into_iter()
        .map(|(subj, pred)| (subj_types.get(&subj).cloned(), pred))
        .collect();
    if types.is_empty() && namespace.is_none() && object_of.is_empty() {
        return None;
    }
    Some((namespace, types, object_of))
}

/// The parsed structural form of one `sh:xone` branch or `sh:node` inner shape: a list of
/// property constraints (parsed through the SAME [`parse_property_shape`] reader the covered
/// fragment uses).
type BranchProps = Vec<PropertyConstraintIr>;

/// A property-level `sh:node [ … ]` construct: the outer path, and the inner node shape's
/// property constraints the value node must satisfy.
struct NodeUnderPath {
    /// The outer `sh:path` predicate whose values the inner shape constrains.
    path: String,
    /// The inner shape's property constraints.
    inner: BranchProps,
}

/// The machine-readable residue constructs authored on `subject`: the node-level `sh:xone`
/// branches and the property-level `sh:node` inner shapes. Hard-fails on a malformed construct —
/// a residue whose structure cannot be read can never be witness-checked, so it never clears.
struct ResidueConstructs {
    xone_branches: Vec<BranchProps>,
    node_paths: Vec<NodeUnderPath>,
}

/// Parse one `sh:xone` list member: either a wrapper carrying `sh:property [ … ]` children, or a
/// direct property-shape member (`[ sh:path P ; sh:minCount 1 ; … ]`).
fn parse_branch(ds: &RdfDataset, member: TermId, shape: &str) -> gmeow_errors::Result<BranchProps> {
    let mut sink = Vec::new();
    let member_quads = quads_of(ds, member);
    let prop_children: Vec<TermId> = member_quads
        .iter()
        .filter(|(p, _)| shacl_local(p) == Some("property"))
        .map(|(_, o)| *o)
        .collect();
    let mut out = Vec::new();
    if prop_children.is_empty() {
        if let Some(p) = parse_property_shape(ds, member, shape, &mut sink)? {
            out.push(p);
        }
    } else {
        for child in prop_children {
            if let Some(p) = parse_property_shape(ds, child, shape, &mut sink)? {
                out.push(p);
            }
        }
    }
    if out.is_empty() {
        return Err(parse_err(format!(
            "semantic witnesses: <{shape}> carries an sh:xone branch with no readable property \
             constraint"
        )));
    }
    Ok(out)
}

/// Read the residue constructs of `shape_iri` out of its authored graph.
fn residue_constructs(ds: &RdfDataset, shape_iri: &str) -> gmeow_errors::Result<ResidueConstructs> {
    let subject = ds
        .term_id_by_value(&TermValue::iri(shape_iri))
        .ok_or_else(|| {
            parse_err(format!(
                "semantic witnesses: shape IRI <{shape_iri}> is not present in the graph"
            ))
        })?;
    let mut xone_branches = Vec::new();
    let mut node_paths = Vec::new();
    for (pred, obj) in quads_of(ds, subject) {
        match shacl_local(&pred) {
            Some("xone") => {
                let ctx = format!("semantic witnesses: <{shape_iri}> sh:xone");
                for member in parse_rdf_list(ds, obj, &ctx)? {
                    xone_branches.push(parse_branch(ds, member, shape_iri)?);
                }
            }
            Some("property") => {
                // A property shape carrying sh:node: read the outer path and the inner shape.
                let prop_quads = quads_of(ds, obj);
                let Some((_, inner_node)) = prop_quads
                    .iter()
                    .find(|(p, _)| shacl_local(p) == Some("node"))
                else {
                    continue;
                };
                let path = prop_quads
                    .iter()
                    .find(|(p, _)| shacl_local(p) == Some("path"))
                    .and_then(|(_, o)| match ds.resolve(*o) {
                        TermRef::Iri(p) => Some(p.to_owned()),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        parse_err(format!(
                            "semantic witnesses: <{shape_iri}> carries sh:node on a property \
                             shape with no single-predicate sh:path"
                        ))
                    })?;
                let inner = parse_branch(ds, *inner_node, shape_iri)?;
                node_paths.push(NodeUnderPath { path, inner });
            }
            _ => {}
        }
    }
    Ok(ResidueConstructs {
        xone_branches,
        node_paths,
    })
}

/// A Turtle literal for datatype `d` whose lexical form is valid for the common XSD datatypes
/// (so a conforming witness is genuinely conforming). The `tag` keeps repeated mints distinct.
fn typed_literal_for(d: &str, tag: usize) -> String {
    const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
    match d.strip_prefix(XSD) {
        Some("string") => format!("\"gmeow-ok-{tag}\""),
        Some(
            "integer" | "int" | "long" | "short" | "byte" | "nonNegativeInteger"
            | "positiveInteger" | "unsignedInt" | "unsignedLong",
        ) => format!("\"{}\"^^<{d}>", tag + 1),
        Some("decimal" | "float" | "double") => format!("\"{}.5\"^^<{d}>", tag + 1),
        Some("boolean") => format!("\"true\"^^<{d}>"),
        Some("dateTime") => format!("\"2026-01-01T00:00:0{}Z\"^^<{d}>", tag % 10),
        Some("date") => format!("\"2026-01-0{}\"^^<{d}>", (tag % 9) + 1),
        Some("anyURI") => format!("\"https://gmeow.example/ok/{tag}\"^^<{d}>"),
        _ => format!("\"gmeow-ok-{tag}\"^^<{d}>"),
    }
}

/// Render a [`ShapeValue`] as a Turtle object term.
fn shape_value_term(v: &ShapeValue) -> String {
    match v {
        ShapeValue::Iri(i) => format!("<{i}>"),
        ShapeValue::Literal {
            lexical,
            datatype,
            lang,
        } => {
            let quoted = format!("\"{}\"", lexical.replace('\\', "\\\\").replace('"', "\\\""));
            match (datatype, lang) {
                (Some(d), _) => format!("{quoted}^^<{d}>"),
                (None, Some(l)) => format!("{quoted}@{l}"),
                (None, None) => quoted,
            }
        }
    }
}

/// A Turtle object term (plus companion triples) that SATISFIES every component of `pc`, or
/// `None` when no confidently-satisfying value can be constructed. `None` only ever SUPPRESSES a
/// witness — a suppressed witness can prevent clearance, never grant it.
fn satisfying_object(
    pc: &PropertyConstraintIr,
    focus: &str,
    tag: usize,
) -> Option<(String, String)> {
    let mut class_of: Option<&str> = None;
    let mut datatype: Option<&str> = None;
    let mut literal_kind = false;
    for c in &pc.components {
        match c {
            ConstraintComponent::HasValue(v) => return Some((shape_value_term(v), String::new())),
            ConstraintComponent::In(vals) => {
                return vals.first().map(|v| (shape_value_term(v), String::new()));
            }
            ConstraintComponent::Class(c) => class_of = Some(c),
            ConstraintComponent::Datatype(d) => datatype = Some(d),
            ConstraintComponent::NodeKindShacl(
                ShaclNodeKind::Iri | ShaclNodeKind::BlankNodeOrIri | ShaclNodeKind::IriOrLiteral,
            ) => {}
            ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal) => literal_kind = true,
            // BlankNode kinds, patterns, facets, nested shapes: no confident satisfying value.
            _ => return None,
        }
    }
    if let Some(d) = datatype {
        return Some((typed_literal_for(d, tag), String::new()));
    }
    if literal_kind {
        return Some((format!("\"gmeow-ok-{tag}\""), String::new()));
    }
    let v = format!("{focus}/ok-{tag}");
    let extra = class_of
        .map(|c| format!("<{v}> <{RDF_TYPE}> <{c}> .\n"))
        .unwrap_or_default();
    Some((format!("<{v}>"), extra))
}

/// The triples that SATISFY one parsed branch / inner shape on `focus`: a satisfying value for
/// every required member (`sh:minCount ≥ 1` or an `sh:hasValue` obligation), nothing for a
/// `sh:maxCount 0` exclusion. `None` when any required member has no constructible value.
fn satisfy_props(props: &BranchProps, focus: &str, tag: &mut usize) -> Option<String> {
    let mut out = String::new();
    for pc in props {
        if pc.max_count == Some(0) {
            continue;
        }
        let required = pc.min_count.unwrap_or(0) >= 1
            || pc
                .components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::HasValue(_)));
        if !required {
            continue;
        }
        let (term, extra) = satisfying_object(pc, focus, *tag)?;
        *tag += 1;
        out.push_str(&format!("<{focus}> <{}> {term} .\n", pc.path));
        out.push_str(&extra);
    }
    Some(out)
}

/// A violating Turtle object term (plus the witness-label prefix) for one covered value
/// component, or `None` for a component family with no confident near-miss.
fn violating_object(comp: &ConstraintComponent, focus: &str) -> Option<(String, &'static str)> {
    match comp {
        ConstraintComponent::Class(_) => {
            Some((format!("<{focus}/wrong-class-value>"), "sh:class@"))
        }
        ConstraintComponent::Datatype(d) => Some((wrong_datatype_literal(d), "sh:datatype@")),
        ConstraintComponent::NodeKindShacl(k) => {
            Some((wrong_node_kind_value(*k, focus), "sh:nodeKind@"))
        }
        _ => None,
    }
}

/// Build the semantic witness plan for one legacy shape: conforming witnesses (flagged by
/// neither side), covered-fragment near-misses, and residue-construct (`sh:node` / `sh:xone`)
/// near-misses. Witness focus membership is synthesized from the shape's target (for a raw
/// SPARQL target: the conservative type/namespace skeleton — never the constraint semantics).
///
/// # Errors
///
/// * the target admits no focus-membership synthesis (targetless, or an out-of-skeleton select);
/// * a residue construct is malformed (unreadable structure can never be witness-checked);
/// * no conforming witness can be built (an unverifiable baseline is vacuous).
pub fn semantic_witness_plan(
    ds: &RdfDataset,
    shape_iri: &str,
    read: &ShapeRead,
) -> gmeow_errors::Result<SemanticWitnessPlan> {
    let membership = FocusMembership::from_target(&read.ir.target)?;
    let constructs = residue_constructs(ds, shape_iri)?;
    let mut idx = 0usize;

    // The base conformance triples for one focus: a satisfying value for every required covered
    // property (`exclude` omits one path for a minCount near-miss), an inner-satisfying value for
    // every required sh:node path, and full satisfaction of ONE sh:xone branch (`branch`).
    let base = |focus: &str,
                exclude: Option<&str>,
                node_exclude: Option<usize>,
                branch: Option<usize>,
                tag: &mut usize|
     -> Option<String> {
        let mut out = String::new();
        for pc in &read.ir.properties {
            if Some(pc.path.as_str()) == exclude {
                continue;
            }
            if pc.min_count.unwrap_or(0) < 1 {
                continue;
            }
            if let Some(np) = constructs.node_paths.iter().find(|n| n.path == pc.path) {
                let v = format!("{focus}/node-ok-{tag}");
                out.push_str(&format!("<{focus}> <{}> <{v}> .\n", np.path));
                out.push_str(&satisfy_props(&np.inner, &v, tag)?);
            } else {
                let (term, extra) = satisfying_object(pc, focus, *tag)?;
                *tag += 1;
                out.push_str(&format!("<{focus}> <{}> {term} .\n", pc.path));
                out.push_str(&extra);
            }
        }
        // Satisfy the focus-node-level covered constructs (all but `node_exclude`): a `sh:class`
        // types the focus, a node-level property-alternatives `sh:or` needs ONE alternative path
        // present. (`sh:nodeKind` is definitionally met — the minted focus is an IRI.)
        for (ni, nc) in read.ir.node_components.iter().enumerate() {
            if Some(ni) == node_exclude {
                continue;
            }
            match nc {
                ConstraintComponent::Class(c) => {
                    out.push_str(&format!("<{focus}> <{RDF_TYPE}> <{c}> .\n"));
                }
                ConstraintComponent::OrProperties(paths) => {
                    if let Some(p0) = paths.first() {
                        out.push_str(&format!("<{focus}> <{p0}> <{focus}/orprop-{tag}> .\n"));
                        *tag += 1;
                    }
                }
                _ => {}
            }
        }
        if let Some(b) = branch {
            out.push_str(&satisfy_props(&constructs.xone_branches[b], focus, tag)?);
        }
        Some(out)
    };
    let default_branch = (!constructs.xone_branches.is_empty()).then_some(0);

    let mut plan = SemanticWitnessPlan::default();
    let mut tag = 0usize;

    // Conforming baseline: flagged by NEITHER side.
    {
        let (focus, mut triples) = membership.mint(&mut idx);
        let body = base(&focus, None, None, default_branch, &mut tag).ok_or_else(|| {
            parse_err(format!(
                "semantic witnesses: <{shape_iri}> has no constructible conforming baseline \
                 (a covered component admits no confident satisfying value)"
            ))
        })?;
        triples.push_str(&body);
        plan.conforming.push(SemanticWitness {
            label: "conforming".to_owned(),
            focus,
            triples,
            expect_flagged: false,
        });
    }

    // Covered-fragment near-misses.
    for pc in &read.ir.properties {
        if constructs.node_paths.iter().any(|n| n.path == pc.path) {
            continue; // exercised by the sh:node residue witnesses below
        }
        if pc.min_count.unwrap_or(0) >= 1 {
            let (focus, mut triples) = membership.mint(&mut idx);
            if let Some(body) = base(&focus, Some(&pc.path), None, default_branch, &mut tag) {
                triples.push_str(&body);
                plan.covered.push(SemanticWitness {
                    label: format!("sh:minCount@{}", pc.path),
                    focus,
                    triples,
                    expect_flagged: true,
                });
            }
        }
        if let Some(m) = pc.max_count
            && m >= 1
        {
            let (focus, mut triples) = membership.mint(&mut idx);
            let mut over = String::new();
            let mut distinct: BTreeSet<String> = BTreeSet::new();
            for _ in 0..=m {
                if let Some((term, extra)) = satisfying_object(pc, &focus, tag) {
                    tag += 1;
                    distinct.insert(term.clone());
                    over.push_str(&format!("<{focus}> <{}> {term} .\n{extra}", pc.path));
                }
            }
            // Only m+1 DISTINCT satisfying values discriminate sh:maxCount — identical terms
            // (an sh:hasValue path) collapse to one value, so the witness is suppressed.
            if distinct.len() == (m as usize) + 1
                && let Some(body) = base(&focus, Some(&pc.path), None, default_branch, &mut tag)
            {
                triples.push_str(&body);
                triples.push_str(&over);
                plan.covered.push(SemanticWitness {
                    label: format!("sh:maxCount@{}", pc.path),
                    focus,
                    triples,
                    expect_flagged: true,
                });
            }
        }
        for comp in &pc.components {
            let (focus, mut triples) = membership.mint(&mut idx);
            let Some((bad, kind)) = violating_object(comp, &focus) else {
                continue;
            };
            if let Some(body) = base(&focus, Some(&pc.path), None, default_branch, &mut tag) {
                triples.push_str(&body);
                triples.push_str(&format!("<{focus}> <{}> {bad} .\n", pc.path));
                plan.covered.push(SemanticWitness {
                    label: format!("{kind}{}", pc.path),
                    focus,
                    triples,
                    expect_flagged: true,
                });
            }
        }
    }

    // Focus-node-level covered constructs near-misses: a `sh:class` the focus does NOT carry
    // (flagged), and a node-level property-alternatives `sh:or` with NONE of its paths present
    // (flagged — the at-least-one obligation fails). The conforming baseline above already
    // exercises the satisfying case for each (typed / one-alternative-present).
    for (ni, nc) in read.ir.node_components.iter().enumerate() {
        match nc {
            ConstraintComponent::Class(c) => {
                let (focus, mut triples) = membership.mint(&mut idx);
                if let Some(body) = base(&focus, None, Some(ni), default_branch, &mut tag) {
                    triples.push_str(&body);
                    plan.covered.push(SemanticWitness {
                        label: format!("node:sh:class@{c}"),
                        focus,
                        triples,
                        expect_flagged: true,
                    });
                }
            }
            ConstraintComponent::OrProperties(paths) => {
                let (focus, mut triples) = membership.mint(&mut idx);
                if let Some(body) = base(&focus, None, Some(ni), default_branch, &mut tag) {
                    triples.push_str(&body);
                    plan.residue.push(SemanticWitness {
                        label: format!("node:sh:or-properties@{}", paths.join("|")),
                        focus,
                        triples,
                        expect_flagged: true,
                    });
                }
            }
            _ => {}
        }
    }

    // sh:xone near-misses: zero branches satisfied, and (when two branches are constructible)
    // two branches satisfied — the witness that discriminates exactly-one from at-least-one.
    if !constructs.xone_branches.is_empty() {
        let (focus, mut triples) = membership.mint(&mut idx);
        if let Some(body) = base(&focus, None, None, None, &mut tag) {
            triples.push_str(&body);
            plan.residue.push(SemanticWitness {
                label: "sh:xone: no alternative present".to_owned(),
                focus,
                triples,
                expect_flagged: true,
            });
        }
        if constructs.xone_branches.len() >= 2 {
            let (focus, mut triples) = membership.mint(&mut idx);
            if let (Some(body), Some(second)) = (
                base(&focus, None, None, Some(0), &mut tag),
                satisfy_props(&constructs.xone_branches[1], &focus, &mut tag),
            ) {
                triples.push_str(&body);
                triples.push_str(&second);
                plan.residue.push(SemanticWitness {
                    label: "sh:xone: two alternatives present".to_owned(),
                    focus,
                    triples,
                    expect_flagged: true,
                });
            }
            // A second-branch conformer: exactly one (the OTHER) alternative present.
            let (focus, mut triples) = membership.mint(&mut idx);
            if let Some(body) = base(&focus, None, None, Some(1), &mut tag) {
                triples.push_str(&body);
                plan.conforming.push(SemanticWitness {
                    label: "conforming (second alternative)".to_owned(),
                    focus,
                    triples,
                    expect_flagged: false,
                });
            }
        }
    }

    // sh:node near-misses: a value node violating the inner shape, and a value node satisfying it.
    for np in &constructs.node_paths {
        let inner_required = np.inner.iter().any(|pc| {
            pc.min_count.unwrap_or(0) >= 1
                || pc
                    .components
                    .iter()
                    .any(|c| matches!(c, ConstraintComponent::HasValue(_)))
        });
        if inner_required {
            let (focus, mut triples) = membership.mint(&mut idx);
            if let Some(body) = base(&focus, Some(&np.path), None, default_branch, &mut tag) {
                triples.push_str(&body);
                triples.push_str(&format!(
                    "<{focus}> <{}> <{focus}/node-near-miss> .\n",
                    np.path
                ));
                plan.residue.push(SemanticWitness {
                    label: format!("sh:node@{}", np.path),
                    focus,
                    triples,
                    expect_flagged: true,
                });
            }
        }
        let (focus, mut triples) = membership.mint(&mut idx);
        if let (Some(body), Some(inner_ok)) = (
            base(&focus, Some(&np.path), None, default_branch, &mut tag),
            satisfy_props(&np.inner, &format!("{focus}/node-conforms"), &mut tag),
        ) {
            triples.push_str(&body);
            triples.push_str(&format!(
                "<{focus}> <{}> <{focus}/node-conforms> .\n",
                np.path
            ));
            triples.push_str(&inner_ok);
            plan.conforming.push(SemanticWitness {
                label: format!("conforming sh:node@{}", np.path),
                focus,
                triples,
                expect_flagged: false,
            });
        }
    }

    Ok(plan)
}

/// Multi-sibling near-miss witnesses for a negated-conjunction obligation over a focus class:
/// `∀this. C(this) ∧ trigger(this, tv) → (p₁ v₁ ∧ … ∧ pₙ vₙ)`. This is the shape a De-Morgan
/// lowering that splits `¬(p₁ ∧ … ∧ pₙ)` into an UNSCOPED `{¬p₁} UNION … UNION {¬pₙ}` silently
/// mis-projects: each union arm binds no variable, so SPARQL evaluates it independently of the
/// guard join — `$this` is unbound inside the arm and `NOT EXISTS { $this pᵢ vᵢ }` degrades to a
/// GLOBAL existence check that ANY sibling carrying `pᵢ vᵢ` clears.
///
/// For each conjunct k it mints (a) a NEAR-MISS focus that triggers the obligation and satisfies
/// every conjunct BUT k, paired with a SIBLING that carries conjunct k (so the unscoped union
/// arm for k — and every other arm, which the focus itself satisfies — is globally cleared), with
/// `expect_flagged = true`; plus one CONFORMING focus that satisfies every conjunct
/// (`expect_flagged = false`). A scoped `FILTER NOT EXISTS { $this p₁ v₁ . … . $this pₙ vₙ }`
/// record flags each near-miss and passes; an unscoped union record flags none and is caught.
///
/// Object terms (`trigger_obj`, each conjunct object) are already-serialized Turtle terms
/// (`<iri>` or a literal like `'true'^^<…boolean>`), so a boolean-triggered obligation is
/// expressible.
pub fn negated_conjunction_sibling_witnesses(
    focus_class: &str,
    trigger_pred: &str,
    trigger_obj: &str,
    conjuncts: &[(&str, &str)],
) -> Vec<SemanticWitness> {
    let base_ns = "https://gmeow.example/negconj";
    let head = |focus: &str| {
        format!(
            "<{focus}> <{RDF_TYPE}> <{focus_class}> .\n<{focus}> <{trigger_pred}> {trigger_obj} .\n"
        )
    };
    let mut out = Vec::new();

    // Conforming: triggers the obligation and satisfies every conjunct.
    {
        let focus = format!("{base_ns}/conforming");
        let mut triples = head(&focus);
        for (p, v) in conjuncts {
            triples.push_str(&format!("<{focus}> <{p}> {v} .\n"));
        }
        out.push(SemanticWitness {
            label: "negated-conjunction: conforming (all conjuncts)".to_owned(),
            focus,
            triples,
            expect_flagged: false,
        });
    }

    // For each conjunct k: a near-miss focus (all conjuncts but k) + a sibling carrying k.
    for (k, (pk, _vk)) in conjuncts.iter().enumerate() {
        let focus = format!("{base_ns}/near-miss-{k}");
        let sibling = format!("{base_ns}/sibling-{k}");
        let mut triples = head(&focus);
        for (i, (p, v)) in conjuncts.iter().enumerate() {
            if i == k {
                continue;
            }
            triples.push_str(&format!("<{focus}> <{p}> {v} .\n"));
        }
        // The sibling carries the OMITTED conjunct's exact (predicate, value) — the datum that
        // clears the unscoped union arm globally.
        let (pk_v, vk_v) = conjuncts[k];
        let _ = pk;
        triples.push_str(&format!("<{sibling}> <{RDF_TYPE}> <{focus_class}> .\n"));
        triples.push_str(&format!("<{sibling}> <{pk_v}> {vk_v} .\n"));
        out.push(SemanticWitness {
            label: format!("negated-conjunction: near-miss missing conjunct {k} (sibling clears)"),
            focus,
            triples,
            expect_flagged: true,
        });
    }

    out
}

/// The Turtle serialization of the shape subgraphs rooted at `roots` in `ds`: every statement
/// reachable from a root subject through blank-node objects, plus IRI-referenced helper shapes
/// reached through `sh:node` / `sh:property`. Blank labels are namespaced by `blank_prefix` so
/// two serializations can be concatenated into one parseable document.
pub fn shape_subgraph_ttl(ds: &RdfDataset, roots: &[String], blank_prefix: &str) -> String {
    let mut out = String::new();
    let mut blanks: std::collections::BTreeMap<TermId, String> = std::collections::BTreeMap::new();
    let mut next_blank = 0usize;
    let term = |ds: &RdfDataset,
                id: TermId,
                blanks: &mut std::collections::BTreeMap<TermId, String>,
                next_blank: &mut usize|
     -> Option<String> {
        match ds.resolve(id) {
            TermRef::Iri(i) => Some(format!("<{i}>")),
            TermRef::Blank { .. } => Some(
                blanks
                    .entry(id)
                    .or_insert_with(|| {
                        let l = format!("_:{blank_prefix}{next_blank}");
                        *next_blank += 1;
                        l
                    })
                    .clone(),
            ),
            TermRef::Literal {
                lexical,
                datatype,
                language,
                ..
            } => {
                let quoted = format!(
                    "\"{}\"",
                    lexical
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n")
                        .replace('\r', "\\r")
                        .replace('\t', "\\t")
                );
                if let Some(lang) = language {
                    return Some(format!("{quoted}@{lang}"));
                }
                match ds.resolve(datatype) {
                    TermRef::Iri(XSD_STRING) => Some(quoted),
                    TermRef::Iri(d) => Some(format!("{quoted}^^<{d}>")),
                    _ => Some(quoted),
                }
            }
            _ => None,
        }
    };
    let mut queue: Vec<TermId> = roots
        .iter()
        .filter_map(|iri| ds.term_id_by_value(&TermValue::iri(iri)))
        .collect();
    let mut seen: BTreeSet<TermId> = queue.iter().copied().collect();
    while let Some(subject) = queue.pop() {
        for q in ds.quads_for_pattern(Some(subject), None, None, GraphMatch::Any) {
            let (Some(s), Some(p), Some(o)) = (
                term(ds, q.s, &mut blanks, &mut next_blank),
                term(ds, q.p, &mut blanks, &mut next_blank),
                term(ds, q.o, &mut blanks, &mut next_blank),
            ) else {
                continue;
            };
            let line = format!("{s} {p} {o} .\n");
            if !out.contains(&line) {
                out.push_str(&line);
            }
            let follow = match ds.resolve(q.o) {
                TermRef::Blank { .. } => true,
                TermRef::Iri(_) => matches!(
                    ds.resolve(q.p),
                    TermRef::Iri(p) if matches!(shacl_local(p), Some("node") | Some("property"))
                ),
                _ => false,
            };
            if follow && seen.insert(q.o) {
                queue.push(q.o);
            }
        }
    }
    out
}

/// Run the legacy shape graph and its projected `logic:formalizes` record as REAL SHACL
/// validators over every semantic witness, requiring focus-flag agreement with the witness's
/// expectation on BOTH sides.
///
/// The legacy-side check is the soundness guard on witness synthesis itself: a witness the
/// legacy shape does not judge as expected is mis-modelled and DENIES clearance (it never
/// silently passes). The projected-side check is the clearance criterion: a record whose lowered
/// constraint does not reproduce the residue semantics MUST NOT clear.
///
/// # Errors
///
/// * either shape graph fails to parse;
/// * vacuity: the witness set lacks a violating or a conforming witness;
/// * a witness's focus-flag verdict differs from its expectation on either side.
pub fn semantic_cross_check(
    legacy_shacl_ttl: &str,
    projected_shacl_ttl: &str,
    witnesses: &[SemanticWitness],
) -> gmeow_errors::Result<()> {
    if !witnesses.iter().any(|w| w.expect_flagged) || !witnesses.iter().any(|w| !w.expect_flagged) {
        return Err(parse_err(
            "semantic_cross_check: vacuous — the witness set must carry at least one violating \
             AND one conforming witness"
                .to_owned(),
        ));
    }
    let legacy = purrdf::shapes::engine::parse_shapes(legacy_shacl_ttl).map_err(|e| {
        parse_err(format!(
            "semantic_cross_check: legacy shapes failed to parse: {e}"
        ))
    })?;
    let projected = purrdf::shapes::engine::parse_shapes(projected_shacl_ttl).map_err(|e| {
        parse_err(format!(
            "semantic_cross_check: projected record shapes failed to parse: {e}"
        ))
    })?;
    for w in witnesses {
        let ds = parse_dataset(w.triples.as_bytes(), "text/turtle", None).map_err(|e| {
            parse_err(format!(
                "semantic_cross_check: witness '{}' failed to parse: {e}",
                w.label
            ))
        })?;
        let focus = format!("<{}>", w.focus);
        let legacy_flagged = finding_keys(&ds, &legacy)
            .iter()
            .any(|(f, _, _)| *f == focus);
        if legacy_flagged != w.expect_flagged {
            return Err(parse_err(format!(
                "semantic_cross_check: witness '{}' is mis-modelled against the legacy shape \
                 itself (expected flagged={}, observed flagged={legacy_flagged}) — clearance \
                 denied",
                w.label, w.expect_flagged
            )));
        }
        let projected_flagged = finding_keys(&ds, &projected)
            .iter()
            .any(|(f, _, _)| *f == focus);
        if projected_flagged != w.expect_flagged {
            return Err(parse_err(format!(
                "semantic_cross_check: the projected record does not reproduce the residue \
                 semantics on witness '{}' (expected flagged={}, observed \
                 flagged={projected_flagged})",
                w.label, w.expect_flagged
            )));
        }
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
            extra_targets: vec![],
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

    /// A legacy fixture styled after the repo-wide meta-shapes: a raw `sh:SPARQLTarget` (type
    /// pattern + STRSTARTS namespace filter) plus structural property constraints.
    const META_SELECT: &str = "\n\
        SELECT ?this WHERE {\n\
            ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
            FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
        }\n";

    fn meta_shape_ttl() -> String {
        format!(
            "{HEADER}@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/MetaShape> a sh:NodeShape ;\n\
             \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"{META_SELECT}\"\"\" ] ;\n\
             \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] ;\n\
             \x20\x20sh:property [ sh:path <https://ex/role> ; sh:minCount 1 ; sh:nodeKind sh:IRI ] .\n"
        )
    }

    #[test]
    fn raw_sparql_target_shape_reads_as_sparql_target_with_whole_shape_residue() {
        // A meta-shape-styled block with ONLY a raw sh:SPARQLTarget reads (no Err): the select
        // becomes the ShapeTarget::Sparql focus selector, the whole shape is marked residue, and
        // the structural property constraints survive as the covered fragment.
        let ds = parse_ttl(&meta_shape_ttl());
        let read = read_shacl_shape(&ds, "https://ex/MetaShape")
            .expect("a raw-SPARQL-target shape must read, not Err");
        assert!(
            matches!(&read.ir.target, ShapeTarget::Sparql(s) if s.contains("STRSTARTS")),
            "{:?}",
            read.ir.target
        );
        assert!(
            read.unsupported
                .iter()
                .any(|u| u == RAW_SPARQL_TARGET_RESIDUE),
            "the raw target must mark the shape residue-bearing: {:?}",
            read.unsupported
        );
        assert_eq!(read.ir.properties.len(), 2, "{:?}", read.ir.properties);
    }

    #[test]
    fn truly_targetless_doc_shape_reads_to_the_empty_focus_sentinel() {
        // A documentation-only marker (label + comment, no target construct, no constraint)
        // reads to the TARGETLESS_SELECT sentinel with an empty residue list: SHACL gives it an
        // empty focus set, so it enforces nothing.
        let ttl = format!(
            "{HEADER}@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/DocMarker> a sh:NodeShape ;\n\
             \x20\x20rdfs:label \"doc-only marker\" ;\n\
             \x20\x20rdfs:comment \"asserts and enforces nothing\" .\n"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/DocMarker")
            .expect("a truly targetless doc shape must be verdictable, not Err");
        assert_eq!(
            read.ir.target,
            ShapeTarget::Sparql(TARGETLESS_SELECT.to_owned())
        );
        assert!(read.unsupported.is_empty(), "{:?}", read.unsupported);
        assert!(read.ir.properties.is_empty());
    }

    #[test]
    fn targetless_shape_with_unreadable_target_construct_stays_an_err() {
        // sh:targetNode is an authored focus selector the reader cannot represent — such a
        // shape must NOT silently read as targetless.
        let ttl = format!(
            "{HEADER}<https://ex/S> a sh:NodeShape ;\n\
             \x20\x20sh:targetNode <https://ex/n> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n"
        );
        let err = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
            .expect_err("an unreadable target construct must stay a hard read error");
        assert!(err.to_string().contains("no sh:targetClass"), "{err}");
    }

    #[test]
    fn multi_target_shape_reads_all_selectors() {
        let ttl = format!(
            "{HEADER}<https://ex/S> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/B>, <https://ex/A>, <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n"
        );
        let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
            .expect("a multi-target shape must read, not Err");
        assert_eq!(read.ir.target, ShapeTarget::Class("https://ex/A".into()));
        assert_eq!(
            read.extra_targets,
            vec![
                ShapeTarget::Class("https://ex/B".into()),
                ShapeTarget::Class("https://ex/C".into())
            ],
            "canonical order, primary excluded"
        );
    }

    #[test]
    fn target_skeleton_parses_type_pattern_and_namespace() {
        let (ns, types, object_of) =
            parse_target_skeleton(META_SELECT).expect("meta skeleton parses");
        assert_eq!(ns.as_deref(), Some("https://example.test/ns/"));
        assert_eq!(
            types,
            vec!["http://www.w3.org/2002/07/owl#Class".to_owned()]
        );
        assert!(object_of.is_empty());
    }

    #[test]
    fn target_skeleton_parses_object_of_property_membership() {
        // `?event a Event . ?event deceptionCue ?this` — the focus is the OBJECT of deceptionCue
        // from an Event-typed subject (the deception-cue membership shape).
        let select = "SELECT ?this WHERE { \
             ?event a <https://ex/Event> . \
             ?event <https://ex/deceptionCue> ?this . }";
        let (ns, types, object_of) =
            parse_target_skeleton(select).expect("object-of skeleton parses");
        assert!(ns.is_none());
        assert!(types.is_empty());
        assert_eq!(
            object_of,
            vec![(
                Some("https://ex/Event".to_owned()),
                "https://ex/deceptionCue".to_owned()
            )]
        );
    }

    #[test]
    fn target_skeleton_parses_type_variable_with_in_domain() {
        let select = "\n\
            SELECT ?this WHERE {\n\
                ?this a ?t .\n\
                FILTER(?t IN (\n\
                    <http://www.w3.org/2002/07/owl#ObjectProperty>,\n\
                    <http://www.w3.org/2002/07/owl#DatatypeProperty>\n\
                ))\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n";
        let (ns, types, object_of) =
            parse_target_skeleton(select).expect("IN-domain skeleton parses");
        assert_eq!(ns.as_deref(), Some("https://example.test/ns/"));
        assert_eq!(
            types,
            vec!["http://www.w3.org/2002/07/owl#ObjectProperty".to_owned()],
            "the first domain class is the membership type"
        );
        assert!(object_of.is_empty());
    }

    #[test]
    fn target_skeleton_rejects_opaque_selects() {
        // A predicate-namespace guard (`?this ?p ?o` + STRSTARTS on ?p) is OUTSIDE the skeleton:
        // no membership can be synthesized, so no witness can be minted (fail-safe).
        let select = "SELECT ?this WHERE { ?this ?p ?o . \
             FILTER(STRSTARTS(STR(?p), \"https://example.test/primary\")) }";
        assert!(parse_target_skeleton(select).is_none());
    }

    /// The xone fixture: a covered property plus an exactly-one-of-two alternative.
    fn xone_shape_ttl() -> String {
        format!(
            "{HEADER}<https://ex/ParamShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Param> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/name> ; sh:minCount 1 ; sh:maxCount 1 ] ;\n\
             \x20\x20sh:xone (\n\
             \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/value> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:Literal ] ]\n\
             \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/entity> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:IRI ] ]\n\
             \x20\x20) .\n"
        )
    }

    /// The record that CORRECTLY lowers the xone: flags a focus with NEITHER alternative and a
    /// focus with BOTH.
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

    /// The WRONG-SEMANTICS record: an `sh:or` lowering (flags only when NEITHER alternative is
    /// present) — it does NOT reproduce the exactly-one obligation.
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
    fn xone_witness_plan_carries_conforming_and_discriminating_residue_witnesses() {
        let ds = parse_ttl(&xone_shape_ttl());
        let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("fixture reads");
        assert!(
            read.unsupported
                .iter()
                .any(|u| u == "http://www.w3.org/ns/shacl#xone"),
            "{:?}",
            read.unsupported
        );
        let plan = semantic_witness_plan(&ds, "https://ex/ParamShape", &read)
            .expect("the xone construct is machine-readable");
        assert!(
            plan.conforming.iter().any(|w| !w.expect_flagged),
            "{plan:?}"
        );
        assert!(
            plan.residue
                .iter()
                .any(|w| w.label.contains("no alternative")),
            "{plan:?}"
        );
        assert!(
            plan.residue
                .iter()
                .any(|w| w.label.contains("two alternatives")),
            "the exactly-one discriminator must be present: {plan:?}"
        );
    }

    #[test]
    fn semantic_cross_check_accepts_the_faithful_xone_record() {
        let ds = parse_ttl(&xone_shape_ttl());
        let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("fixture reads");
        let plan = semantic_witness_plan(&ds, "https://ex/ParamShape", &read).expect("plan");
        let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/ParamShape".to_owned()], "l");
        let mut witnesses = plan.conforming.clone();
        witnesses.extend(plan.residue.clone());
        semantic_cross_check(&legacy_ttl, XONE_RECORD_TTL, &witnesses)
            .expect("the faithful record reproduces the xone semantics");
    }

    #[test]
    fn semantic_cross_check_rejects_the_or_lowered_xone_record() {
        // The load-bearing falsifiability check: a record that lowers the exactly-one to an
        // at-least-one does NOT flag the two-alternatives near-miss and MUST NOT clear.
        let ds = parse_ttl(&xone_shape_ttl());
        let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("fixture reads");
        let plan = semantic_witness_plan(&ds, "https://ex/ParamShape", &read).expect("plan");
        let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/ParamShape".to_owned()], "l");
        let mut witnesses = plan.conforming.clone();
        witnesses.extend(plan.residue.clone());
        let err = semantic_cross_check(&legacy_ttl, XONE_OR_LOWERED_RECORD_TTL, &witnesses)
            .expect_err("an or-lowered record must not survive the witness cross-check");
        assert!(err.to_string().contains("does not reproduce"), "{err}");
    }

    #[test]
    fn semantic_cross_check_hard_fails_on_a_vacuous_witness_set() {
        let err = semantic_cross_check("", "", &[])
            .expect_err("an empty witness set is vacuous, not a pass");
        assert!(err.to_string().contains("vacuous"), "{err}");
    }

    // The WP:GNG-triad negated-conjunction fixtures: a `CitationAct` asserting `supportsNotability
    // true` must carry all three triad values. `NC_*` model the projected record two ways.
    //
    // The CORRECT scoped lowering: ONE `FILTER NOT EXISTS` over the whole triad conjunction, so
    // `$this` stays bound and the check is per focus node.
    const NC_SCOPED_TTL: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/NotabilityShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/NotabilityShape> ;\n\
        \x20\x20sh:targetClass <https://ex/CitationAct> ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:message \"WP:GNG triad\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { \
        $this <https://ex/supportsNotability> 'true'^^<http://www.w3.org/2001/XMLSchema#boolean> . \
        FILTER NOT EXISTS { $this <https://ex/indep> <https://ex/independent> . \
        $this <https://ex/tier> <https://ex/secondary> . \
        $this <https://ex/cov> <https://ex/significant> . } }\"\"\" ] .\n";
    // The BUGGY unscoped De-Morgan lowering: a UNION of per-conjunct `FILTER NOT EXISTS` arms.
    // Each arm binds nothing, so `$this` is unbound and the check degrades to global existence.
    const NC_UNION_TTL: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/NotabilityShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/NotabilityShape> ;\n\
        \x20\x20sh:targetClass <https://ex/CitationAct> ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:message \"WP:GNG triad\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { \
        $this <https://ex/supportsNotability> 'true'^^<http://www.w3.org/2001/XMLSchema#boolean> . \
        { FILTER NOT EXISTS { $this <https://ex/indep> <https://ex/independent> . } } UNION \
        { FILTER NOT EXISTS { $this <https://ex/tier> <https://ex/secondary> . } } UNION \
        { FILTER NOT EXISTS { $this <https://ex/cov> <https://ex/significant> . } } }\"\"\" ] .\n";

    fn nc_witnesses() -> Vec<SemanticWitness> {
        negated_conjunction_sibling_witnesses(
            "https://ex/CitationAct",
            "https://ex/supportsNotability",
            "'true'^^<http://www.w3.org/2001/XMLSchema#boolean>",
            &[
                ("https://ex/indep", "<https://ex/independent>"),
                ("https://ex/tier", "<https://ex/secondary>"),
                ("https://ex/cov", "<https://ex/significant>"),
            ],
        )
    }

    #[test]
    fn negated_conjunction_witnesses_carry_a_conforming_and_a_multi_sibling_near_miss() {
        let ws = nc_witnesses();
        assert!(ws.iter().any(|w| !w.expect_flagged), "{ws:?}");
        assert_eq!(
            ws.iter().filter(|w| w.expect_flagged).count(),
            3,
            "one multi-sibling near-miss per triad conjunct: {ws:?}"
        );
        // Every near-miss carries a sibling that satisfies the omitted conjunct.
        assert!(
            ws.iter()
                .filter(|w| w.expect_flagged)
                .all(|w| w.triples.contains("/sibling-")),
            "{ws:?}"
        );
    }

    #[test]
    fn semantic_cross_check_accepts_the_scoped_negated_conjunction_record() {
        // The scoped single-NOT-EXISTS record reproduces the per-focus triad obligation.
        semantic_cross_check(NC_SCOPED_TTL, NC_SCOPED_TTL, &nc_witnesses())
            .expect("the scoped record reproduces the negated-conjunction semantics");
    }

    #[test]
    fn semantic_cross_check_rejects_the_unscoped_union_negated_conjunction_projection() {
        // The load-bearing guard: the pre-fix `{¬a} UNION {¬b} UNION {¬c}` lowering loses
        // $this-scoping, so a sibling satisfying one conjunct globally clears the branch and the
        // near-miss is NOT flagged — the oracle MUST catch it (guard against silent recurrence of
        // the orgbook_notability_mutation projector bug).
        let err = semantic_cross_check(NC_SCOPED_TTL, NC_UNION_TTL, &nc_witnesses())
            .expect_err("an unscoped-union projection must not survive the witness cross-check");
        assert!(err.to_string().contains("does not reproduce"), "{err}");
    }

    #[test]
    fn sh_node_witness_plan_discriminates_the_inner_shape() {
        // A StyleGuide-styled property-level sh:node: the inner shape requires a digest on the
        // value node. Both the violating and the conforming witness must be present, and the
        // legacy shape itself must judge them as expected (checked through the cross-check with
        // the legacy graph on BOTH sides — a definitionally-faithful record).
        let ttl = format!(
            "{HEADER}<https://ex/GuideShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Guide> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/for> ; sh:minCount 1 ] ;\n\
             \x20\x20sh:property [\n\
             \x20\x20\x20\x20sh:path <https://ex/exemplifiedBy> ;\n\
             \x20\x20\x20\x20sh:node [ sh:property [ sh:path <https://ex/digest> ; sh:minCount 1 ] ] ;\n\
             \x20\x20] .\n"
        );
        let ds = parse_ttl(&ttl);
        let read = read_shacl_shape(&ds, "https://ex/GuideShape").expect("fixture reads");
        assert!(
            read.unsupported
                .iter()
                .any(|u| u == "http://www.w3.org/ns/shacl#node"),
            "{:?}",
            read.unsupported
        );
        let plan = semantic_witness_plan(&ds, "https://ex/GuideShape", &read).expect("plan");
        assert!(
            plan.residue.iter().any(|w| w.label.contains("sh:node@")),
            "{plan:?}"
        );
        assert!(
            plan.conforming
                .iter()
                .any(|w| w.label.contains("conforming sh:node@")),
            "{plan:?}"
        );
        let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/GuideShape".to_owned()], "l");
        let mut witnesses = plan.conforming.clone();
        witnesses.extend(plan.residue.clone());
        semantic_cross_check(&legacy_ttl, &legacy_ttl, &witnesses)
            .expect("the legacy graph agrees with itself on every witness");
    }

    #[test]
    fn meta_shape_witness_plan_pins_to_structural_property_constraints() {
        let ds = parse_ttl(&meta_shape_ttl());
        let read = read_shacl_shape(&ds, "https://ex/MetaShape").expect("fixture reads");
        let plan = semantic_witness_plan(&ds, "https://ex/MetaShape", &read).expect("plan");
        // Focus membership was synthesized from the skeleton: witnesses live under the namespace.
        for w in plan.conforming.iter().chain(plan.covered.iter()) {
            assert!(w.focus.starts_with("https://example.test/ns/"), "{w:?}");
        }
        assert!(
            plan.covered.iter().any(|w| w
                .label
                .contains("sh:minCount@http://www.w3.org/2000/01/rdf-schema#label")),
            "{plan:?}"
        );
        assert!(
            plan.covered
                .iter()
                .any(|w| w.label.contains("sh:nodeKind@https://ex/role")),
            "{plan:?}"
        );
        // The legacy shape agrees with itself over the full plan (target execution included).
        let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/MetaShape".to_owned()], "l");
        let mut witnesses = plan.conforming.clone();
        witnesses.extend(plan.covered.clone());
        semantic_cross_check(&legacy_ttl, &legacy_ttl, &witnesses)
            .expect("the meta shape agrees with itself on every structural witness");
    }
}
