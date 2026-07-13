// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared **`gmeow:StatementMetadata` reified-claim** template — the single definition of
//! the RDF-1.2 reified up-lift claim, plus the `AssertionPolarity` that decides *when* an
//! up-lift is asserted as fact versus carried as a reified claim.
//!
//! ## The second leg of the `classify_put` morphism
//!
//! [`crate::projections::put_derivation::classify_put`] is the single authority mapping a
//! correspondence's `(mnemomorphic, rung, law-claims)` to a [`PutClass`]. That is only half the
//! decision; the other half is *how the lifted atom lands in RDF*. [`AssertionPolarity`] is that
//! second leg:
//!
//! - [`AssertionPolarity::AssertBase`] — a `CompleteOver` recovery. `put ∘ get = id` is
//!   discharged, so the recovered source atom genuinely IS fact: assert it directly into the
//!   base graph.
//! - [`AssertionPolarity::ReifyClaim`] — a `ValidationOnly` lift. The source cannot itself
//!   express the atom, so it is carried as a **reasoner-inert** `gmeow:StatementMetadata`
//!   reified claim (a candidate preimage under `PutGet`, `ObligationUnknown`), never asserted
//!   as extracted fact.
//! - [`AssertionPolarity::Withhold`] — an `Unsupported` floor: carry nothing (loss ledger only).
//!
//! Both the committed `.put.rq` emitter ([`crate::projections::sparql_put`]) and the native
//! put executor (`gmeow-pipeline`'s `put_executor::claim_query`) render the reified claim through
//! [`reified_claim_head`], so the two surfaces cannot drift: the `.put.rq` projection is a
//! genuine projection of the native executor's reference semantics, not a parallel re-invention.
//!
//! ## Why the reified claim is inert
//!
//! No fixed EL/DL/RL rule keys on any reification predicate ([`GM_Q_SUBJECT`],
//! [`GM_Q_PREDICATE`], [`GM_Q_OBJECT`], …), and the reason lane drops every non-IRI-object quad
//! before rule evaluation. A `gmeow:StatementMetadata` cell therefore loads into the EDB but
//! fires nothing: the object-level relation is only *named* as the IRI value of `qPredicate`,
//! never asserted as a triple predicate. The lift is carried with maximal fidelity yet
//! materializes no new fact — the honest `ValidationOnly` treatment.

use crate::projections::get_leg::curie;
use crate::projections::put_derivation::PutClass;

// ── The reified-claim vocabulary (single source of truth) ────────────────────────────
//
// `gmeow-pipeline` re-exports these from here (see `up_projection_corpus.rs`) so the reified
// claim's vocabulary — emitted by this builder, consumed by the down-projection reification
// handler, and seeded as the non-gated `STATEMENT_METADATA_TERMS` passthrough — has ONE
// definition, never a per-crate copy.

/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// `gmeow:StatementMetadata` — the reified-claim cell type.
pub const GM_STATEMENT_METADATA: &str = "https://blackcatinformatics.ca/gmeow/StatementMetadata";
/// `gmeow:qSubject` — the reified subject.
pub const GM_Q_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/qSubject";
/// `gmeow:qPredicate` — the reified predicate (always an IRI).
pub const GM_Q_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/qPredicate";
/// `gmeow:qObject` — the reified object when it is an IRI (or class/var IRI).
pub const GM_Q_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/qObject";
/// `gmeow:qObjectLiteral` — the reified object when it is a literal.
pub const GM_Q_OBJECT_LITERAL: &str = "https://blackcatinformatics.ca/gmeow/qObjectLiteral";
/// `gmeow:annotation` — links a cell to one annotation node.
pub const GM_ANNOTATION: &str = "https://blackcatinformatics.ca/gmeow/annotation";
/// `gmeow:annProperty` — the annotation's property (an AnnotationProperty IRI).
pub const GM_ANN_PROPERTY: &str = "https://blackcatinformatics.ca/gmeow/annProperty";
/// `gmeow:annValue` — the annotation's value.
pub const GM_ANN_VALUE: &str = "https://blackcatinformatics.ca/gmeow/annValue";
/// `gmeow:mappedFrom` — the forward-projection target the claim is mapped from.
pub const GM_MAPPED_FROM: &str = "https://blackcatinformatics.ca/gmeow/mappedFrom";
/// `gmeow:wasGeneratedBy` — links a reified claim to the import activity that generated it.
pub const GM_WAS_GENERATED_BY: &str = "https://blackcatinformatics.ca/gmeow/wasGeneratedBy";

/// The RDF assertion polarity an up-lift renders to — the second leg of the
/// [`classify_put`](crate::projections::put_derivation::classify_put) morphism: given the
/// [`PutClass`], *how* the lifted atom lands in RDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionPolarity {
    /// A lossless `CompleteOver` recovery: assert the source atom directly into the base graph
    /// (`put ∘ get = id` is discharged, so it genuinely is fact).
    AssertBase,
    /// A lossy `ValidationOnly` lift: carry it as a reasoner-inert `gmeow:StatementMetadata`
    /// reified claim, never an extracted fact.
    ReifyClaim,
    /// An `Unsupported` floor: carry nothing (loss ledger only).
    Withhold,
}

impl AssertionPolarity {
    /// The polarity of a classified up-lift — the total, single-authority map from the three
    /// [`PutClass`] cases to the three RDF renderings. Every put surface branches on this, so a
    /// new rung or ingest target picks up the correct assert-vs-reify-vs-withhold rendering for
    /// free.
    #[must_use]
    pub(crate) fn of(class: PutClass) -> Self {
        match class {
            PutClass::CompleteOver => Self::AssertBase,
            PutClass::ValidationOnly => Self::ReifyClaim,
            PutClass::Unsupported => Self::Withhold,
        }
    }
}

/// How the fixed reification-vocabulary IRIs are rendered in the emitted SPARQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IriStyle {
    /// Full `<iri>` form — for a query executed directly (no prefix block), e.g. the native
    /// `put_executor`.
    Full,
    /// Prefixed `gmeow:local` CURIE form — for a committed `.put.rq` file whose header carries a
    /// prefix block, e.g. the `sparql_put` emitter.
    Curie,
}

impl IriStyle {
    /// Render one IRI in this style.
    #[must_use]
    pub fn render(self, iri: &str) -> String {
        match self {
            Self::Full => format!("<{iri}>"),
            Self::Curie => curie(iri),
        }
    }
}

/// The object slot of a reified claim (mirrors the three claim shapes: a class/IRI object, a
/// variable object treated as an IRI, or a literal object).
#[derive(Debug, Clone)]
pub enum ClaimObject {
    /// `gmeow:qObject <term>` — `term` is an already-rendered IRI, class CURIE, or `?var`
    /// bound to an IRI object.
    Iri(String),
    /// `gmeow:qObjectLiteral <term>` — `term` is an already-rendered `?var` bound to a literal.
    Literal(String),
}

/// One reified-claim annotation entry: `_:ann gmeow:annProperty <property> ; gmeow:annValue …`.
#[derive(Debug, Clone)]
pub struct ClaimAnnotation {
    /// The blank-node label for this annotation node (unique within the enclosing CONSTRUCT).
    pub label: String,
    /// The annotation property IRI (e.g. `GM_MAPPED_FROM` or `GM_CONFIDENCE`).
    pub property: String,
    /// The already-rendered annotation value term (an IRI/CURIE, or a typed/plain literal).
    pub value: String,
}

/// A single `gmeow:StatementMetadata` reified claim to render.
#[derive(Debug, Clone)]
pub struct ReifiedClaim {
    /// The blank-node label for the cell (unique within the enclosing CONSTRUCT).
    pub cell_label: String,
    /// The already-rendered reified subject term (an `?var` or IRI/CURIE).
    pub subject: String,
    /// The reified predicate IRI (always an IRI; rendered in the chosen [`IriStyle`]).
    pub predicate: String,
    /// The reified object slot.
    pub object: ClaimObject,
    /// The annotation nodes hung off the cell (at least `mappedFrom`; optionally `confidence`).
    pub annotations: Vec<ClaimAnnotation>,
    /// The `gmeow:wasGeneratedBy` provenance link — the deterministic import-activity IRI this
    /// claim was generated by. `None` for surfaces that carry provenance separately.
    pub generated_by: Option<String>,
}

/// Render one reified claim's CONSTRUCT-head triples — the SINGLE definition of the
/// `gmeow:StatementMetadata` reified-claim template, shared by every put surface. Returns one
/// complete `subject predicate object .` line per triple (no `;` continuation), so the block
/// is order-robust and reads cleanly in a committed `.put.rq`.
///
/// The caller supplies pre-rendered terms (subject/object/annotation values) and unique blank
/// labels; this builder renders only the fixed reification vocabulary in `style` and lays out
/// the reified triad + annotation list + optional provenance edge.
#[must_use]
pub fn reified_claim_head(claim: &ReifiedClaim, style: IriStyle) -> Vec<String> {
    let cell = format!("_:{}", claim.cell_label);
    let (obj_pred, obj_term) = match &claim.object {
        ClaimObject::Iri(term) => (GM_Q_OBJECT, term.clone()),
        ClaimObject::Literal(term) => (GM_Q_OBJECT_LITERAL, term.clone()),
    };
    let mut lines = vec![
        format!(
            "{cell} {} {} .",
            style.render(RDF_TYPE),
            style.render(GM_STATEMENT_METADATA)
        ),
        format!("{cell} {} {} .", style.render(GM_Q_SUBJECT), claim.subject),
        format!(
            "{cell} {} {} .",
            style.render(GM_Q_PREDICATE),
            style.render(&claim.predicate)
        ),
        format!("{cell} {} {obj_term} .", style.render(obj_pred)),
    ];
    for ann in &claim.annotations {
        let ann_node = format!("_:{}", ann.label);
        lines.push(format!(
            "{cell} {} {ann_node} .",
            style.render(GM_ANNOTATION)
        ));
        lines.push(format!(
            "{ann_node} {} {} .",
            style.render(GM_ANN_PROPERTY),
            style.render(&ann.property)
        ));
        lines.push(format!(
            "{ann_node} {} {} .",
            style.render(GM_ANN_VALUE),
            ann.value
        ));
    }
    if let Some(import_iri) = &claim.generated_by {
        lines.push(format!(
            "{cell} {} {import_iri} .",
            style.render(GM_WAS_GENERATED_BY)
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polarity_is_the_total_map_from_the_three_put_classes() {
        assert_eq!(
            AssertionPolarity::of(PutClass::CompleteOver),
            AssertionPolarity::AssertBase
        );
        assert_eq!(
            AssertionPolarity::of(PutClass::ValidationOnly),
            AssertionPolarity::ReifyClaim
        );
        assert_eq!(
            AssertionPolarity::of(PutClass::Unsupported),
            AssertionPolarity::Withhold
        );
    }

    #[test]
    fn full_style_renders_a_type_object_claim_with_provenance() {
        let claim = ReifiedClaim {
            cell_label: "cell".to_owned(),
            subject: "?s".to_owned(),
            predicate: RDF_TYPE.to_owned(),
            object: ClaimObject::Iri(
                "<https://blackcatinformatics.ca/gmeow/ModelArtifact>".to_owned(),
            ),
            annotations: vec![ClaimAnnotation {
                label: "mapann".to_owned(),
                property: GM_MAPPED_FROM.to_owned(),
                value: "<http://www.w3.org/ns/mls#Model>".to_owned(),
            }],
            generated_by: Some(
                "<https://blackcatinformatics.ca/gmeow/import/ml-schema>".to_owned(),
            ),
        };
        let lines = reified_claim_head(&claim, IriStyle::Full);
        let block = lines.join("\n");
        assert!(block.contains("_:cell <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/StatementMetadata> ."));
        assert!(block.contains("<https://blackcatinformatics.ca/gmeow/qSubject> ?s ."));
        assert!(block.contains("<https://blackcatinformatics.ca/gmeow/qObject> <https://blackcatinformatics.ca/gmeow/ModelArtifact> ."));
        assert!(block.contains("<https://blackcatinformatics.ca/gmeow/mappedFrom> ."));
        assert!(block.contains("<https://blackcatinformatics.ca/gmeow/wasGeneratedBy> <https://blackcatinformatics.ca/gmeow/import/ml-schema> ."));
    }

    #[test]
    fn curie_style_renders_a_literal_object_claim() {
        let claim = ReifiedClaim {
            cell_label: "cell0".to_owned(),
            subject: "?s".to_owned(),
            predicate: "https://blackcatinformatics.ca/gmeow/label".to_owned(),
            object: ClaimObject::Literal("?o".to_owned()),
            annotations: vec![ClaimAnnotation {
                label: "mapann0".to_owned(),
                property: GM_MAPPED_FROM.to_owned(),
                value: "skos:prefLabel".to_owned(),
            }],
            generated_by: None,
        };
        let block = reified_claim_head(&claim, IriStyle::Curie).join("\n");
        assert!(block.contains("_:cell0 rdf:type gmeow:StatementMetadata ."));
        assert!(block.contains("gmeow:qObjectLiteral ?o ."));
        assert!(
            !block.contains("wasGeneratedBy"),
            "no provenance edge when None"
        );
    }
}
