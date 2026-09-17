// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `logic:Correspondence` carrier lane (C10): project a set of typed
//! [`Correspondence`] IR nodes into a deterministic, sorted, byte-stable RDF
//! named-graph (`graph/correspondence`) and re-derive them back — the inverse the
//! cache uses on a hit.
//!
//! # The load-bearing correctness point
//!
//! A correspondence carries a *typed relation* on the alignment lattice
//! ([`CorrespondenceRelation`]). The projection emits the SSSOM-facing alignment
//! predicate that is **sound for that relation and no stronger**:
//!
//! * `logic:Overlaps` / `logic:RelatedMatch` → `skos:relatedMatch` (a non-committing
//!   association), NEVER `skos:exactMatch` and NEVER `owl:equivalentClass`;
//! * `logic:Equiv` is the only relation that MAY surface `skos:exactMatch`, and even
//!   then a [`MorphismKind::CommitmentShiftingBridge`] forbids it.
//!
//! The §14 affine triangle (`foaf:Person` ⟂ `schema:ContactPoint` co-projecting onto
//! the contact-bearing facet of `gmeow:contact`) is a *vague affine overlap*: the
//! honest canonical object is a `relatedMatch`, not a forced equality. The overclaim
//! gate ([`assert_no_overclaim_correspondence`]) turns the build red if a caller tries
//! to emit equivalence for such a correspondence — "never silently over-align" is a
//! typed property, not a promise.
//!
//! # Zero inter-phase serialization
//!
//! The producing stage constructs the [`CorrespondenceProgram`] once, projects it once
//! (here), and carries BOTH the typed program (a `PipelineHandle::Correspondence`
//! payload) and its backing `graph/correspondence` projection on one content-addressed
//! bundle. A downstream consumer reads the typed handle directly; only the cache
//! boundary re-derives it (via [`parse_correspondence`]) from the backing graph on a
//! hit. The program is never re-serialized between phases.

use std::fmt::Write as _;

use crate::graphutil::{Subject, is_structural_type_predicate, nn, objects, term_as_subject};
use crate::ir::{
    Correspondence, CorrespondenceCaveat, CorrespondenceRelation, LOGIC_NAMESPACE, LegPath,
    MorphismKind, PreservationKind, RecoveryCaseIr, TransactionProgramIr,
};

use gmeow_errors::Diag;

use super::OverclaimError;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const SKOS_RELATED_MATCH: &str = "http://www.w3.org/2004/02/skos/core#relatedMatch";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const GMEOW_NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
#[cfg(test)]
use test_support::XSD_DECIMAL;

/// The local predicates/classes this projection mints under `LOGIC_NAMESPACE`. Kept as
/// small builder functions (string constants would duplicate the namespace prefix).
fn class_program() -> String {
    format!("{LOGIC_NAMESPACE}CorrespondenceProgram")
}
fn class_correspondence() -> String {
    format!("{LOGIC_NAMESPACE}Correspondence")
}
fn class_grounding_correspondence() -> String {
    format!("{LOGIC_NAMESPACE}GroundingCorrespondence")
}
fn class_caveat() -> String {
    format!("{LOGIC_NAMESPACE}CorrespondenceCaveat")
}
fn class_law_claim() -> String {
    format!("{LOGIC_NAMESPACE}LawClaim")
}
fn class_recovery_case() -> String {
    format!("{LOGIC_NAMESPACE}RecoveryCase")
}
fn p_has_correspondence() -> String {
    format!("{LOGIC_NAMESPACE}hasCorrespondence")
}
fn p_has_preservation() -> String {
    format!("{LOGIC_NAMESPACE}hasPreservation")
}
/// The PER-CORRESPONDENCE preservation judgment predicate (`logic:preservationKind`) —
/// DISTINCT from the program-level `logic:hasPreservation` (the whole-lane polarity). A
/// correspondence declaring this carries its own Principle-17 loss residue.
fn p_preservation_kind() -> String {
    format!("{LOGIC_NAMESPACE}preservationKind")
}
fn p_relation() -> String {
    format!("{LOGIC_NAMESPACE}correspondenceRelation")
}
fn p_morphism_class() -> String {
    format!("{LOGIC_NAMESPACE}morphismClass")
}
fn p_morphism_kind() -> String {
    format!("{LOGIC_NAMESPACE}morphismKind")
}
fn p_mnemomorphic() -> String {
    format!("{LOGIC_NAMESPACE}mnemomorphic")
}
fn p_determinacy() -> String {
    format!("{LOGIC_NAMESPACE}hasDeterminacy")
}
fn p_get_leg() -> String {
    format!("{LOGIC_NAMESPACE}getLeg")
}
fn p_put_leg() -> String {
    format!("{LOGIC_NAMESPACE}putLeg")
}
fn p_source_endpoint() -> String {
    format!("{LOGIC_NAMESPACE}sourceEndpoint")
}
fn p_target_endpoint() -> String {
    format!("{LOGIC_NAMESPACE}targetEndpoint")
}
fn p_confidence() -> String {
    format!("{LOGIC_NAMESPACE}confidence")
}
fn p_evidence_strength() -> String {
    format!("{LOGIC_NAMESPACE}evidenceStrength")
}
fn p_weight() -> String {
    format!("{LOGIC_NAMESPACE}weight")
}
fn p_probability() -> String {
    format!("{LOGIC_NAMESPACE}probability")
}
fn p_according_to() -> String {
    format!("{GMEOW_NAMESPACE}accordingTo")
}
fn p_has_law_claim() -> String {
    format!("{LOGIC_NAMESPACE}hasLawClaim")
}
fn p_law_claimed() -> String {
    format!("{LOGIC_NAMESPACE}lawClaimed")
}
fn p_law_verdict() -> String {
    format!("{LOGIC_NAMESPACE}lawDischargeVerdict")
}
fn p_law_condition() -> String {
    format!("{LOGIC_NAMESPACE}lawDischargeCondition")
}
fn p_has_caveat() -> String {
    format!("{LOGIC_NAMESPACE}hasCaveat")
}
fn p_lossy_drop() -> String {
    format!("{LOGIC_NAMESPACE}lossyDrop")
}
fn p_recovery_case() -> String {
    format!("{LOGIC_NAMESPACE}recoveryCase")
}
fn p_recovery_transform() -> String {
    format!("{LOGIC_NAMESPACE}recoveryTransform")
}

/// The single `CorrespondenceProgram` node IRI (one program per build).
fn program_iri() -> String {
    format!("{LOGIC_NAMESPACE}correspondence-program")
}

/// The SSSOM-facing alignment predicate that is **sound for `relation` and no
/// stronger** (the load-bearing decision): only `Equiv` may surface an exact match;
/// every weaker relation surfaces a non-committing `skos:relatedMatch`.
///
/// `None` means "no alignment predicate is emitted" (the `Disjoint` negative pole — an
/// asserted non-alignment never produces a positive match triple).
fn alignment_predicate(
    relation: CorrespondenceRelation,
    kind: MorphismKind,
) -> Option<&'static str> {
    match relation {
        // Only a true equivalence MAY surface exactMatch — and a commitment-shifting
        // bridge demotes even that to a related match (the loss ledger refuses an
        // owl:equivalentClass for a by-reference bridge).
        CorrespondenceRelation::Equiv => match kind {
            MorphismKind::InstitutionMorphism => Some(SKOS_EXACT_MATCH),
            MorphismKind::CommitmentShiftingBridge => Some(SKOS_RELATED_MATCH),
        },
        CorrespondenceRelation::Subsumes
        | CorrespondenceRelation::SubsumedBy
        | CorrespondenceRelation::Overlaps
        | CorrespondenceRelation::RelatedMatch => Some(SKOS_RELATED_MATCH),
        // An asserted non-alignment emits no positive match.
        CorrespondenceRelation::Disjoint => None,
    }
}

/// Whether the relation/kind pair MAY lawfully surface a class equivalence
/// (`owl:equivalentClass` / `skos:exactMatch`). Only a satisfaction-preserving true
/// equivalence may; every weaker or commitment-shifting correspondence may NOT.
fn may_claim_equivalence(relation: CorrespondenceRelation, kind: MorphismKind) -> bool {
    matches!(relation, CorrespondenceRelation::Equiv)
        && matches!(kind, MorphismKind::InstitutionMorphism)
}

/// Enforce the correspondence overclaim contract (LOGIC-CORRESPONDENCE §overclaim→red,
/// take1 §14): a correspondence that is NOT a satisfaction-preserving true equivalence
/// may not emit a class equivalence. A caller asking to surface `owl:equivalentClass`
/// or `skos:exactMatch` for a caveated overlap / affine / bridge correspondence is a
/// BUILD FAILURE — never a silently over-aligned view.
///
/// `wants_equivalence` is the caller's intent (it asked an alignment back-end for an
/// equivalence surface for this correspondence).
pub fn assert_no_overclaim_correspondence(
    correspondence: &Correspondence,
    wants_equivalence: bool,
) -> Result<(), OverclaimError> {
    if wants_equivalence
        && !may_claim_equivalence(correspondence.relation, correspondence.morphism_kind)
    {
        return Err(OverclaimError(format!(
            "Overclaim in correspondence <{}>: declared logic:{} (with logic:{}) but the build \
             asked to emit a class equivalence (owl:equivalentClass / skos:exactMatch). A \
             caveated overlap / affine / bridge correspondence is sound only as skos:relatedMatch; \
             emitting equivalence would over-align it.",
            correspondence.iri,
            correspondence.relation.as_str(),
            correspondence.morphism_kind.as_str(),
        )));
    }
    Ok(())
}

/// A compiled set of [`Correspondence`] nodes plus their caveats and the declared
/// preservation polarity — the typed payload the `PipelineHandle::Correspondence` arm
/// carries (C10). One content identity ([`CorrespondenceProgram::content_key`])
/// across the typed handle and its backing `graph/correspondence` projection.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CorrespondenceProgram {
    /// The correspondences, canonically sorted by IRI at construction.
    pub correspondences: Vec<Correspondence>,
    /// Named sequential obligations; always graded with their owning program.
    pub compositions: Vec<crate::ir::CorrespondenceComposition>,
    /// The declared preservation polarity for this lane (the loss-ledger row): a
    /// caveated overlap is a `SoundUnderApproximation` (it under-approximates the
    /// forced-equality reading it refuses), never `ExactPreservation`.
    pub preservation: PreservationKind,
    /// The leg-program registry: each `logic:getLeg` / `logic:putLeg` IRI resolves to its
    /// realized [`LegPath`] body here, so the round-trip gate composes actual leg bodies
    /// instead of comparing opaque IRIs. Sorted by IRI; **append-only** in the content key
    /// (a leg-free program keys identically to before legs were modelled).
    pub leg_programs: Vec<TransactionProgramIr>,
}

impl CorrespondenceProgram {
    /// Construct, canonicalizing the collections into sorted order so the content
    /// identity is construction-order-independent. The leg-program registry starts empty;
    /// attach it with [`CorrespondenceProgram::with_leg_programs`].
    pub fn new(correspondences: Vec<Correspondence>, preservation: PreservationKind) -> Self {
        let mut correspondences = correspondences;
        correspondences.sort_by(|a, b| a.iri.cmp(&b.iri));
        Self {
            correspondences,
            compositions: Vec::new(),
            preservation,
            leg_programs: Vec::new(),
        }
    }

    /// Attach the canonical declaration collection without reordering any operands.
    pub fn with_compositions(
        mut self,
        mut compositions: Vec<crate::ir::CorrespondenceComposition>,
    ) -> Self {
        compositions.sort();
        self.compositions = compositions;
        self
    }

    /// Attach the leg-program registry, sorted by IRI for a construction-order-independent
    /// identity. Append-only: a program with no leg bodies keys exactly as it did before.
    pub fn with_leg_programs(mut self, leg_programs: Vec<TransactionProgramIr>) -> Self {
        let mut leg_programs = leg_programs;
        leg_programs.sort_by(|a, b| a.iri.cmp(&b.iri));
        self.leg_programs = leg_programs;
        self
    }

    /// Resolve a leg IRI to its realized [`LegPath`] body via the registry, if present.
    pub fn resolve_leg(&self, leg_iri: &str) -> Option<&LegPath> {
        self.leg_programs
            .iter()
            .find(|p| p.iri == leg_iri)
            .map(|p| &p.body)
    }

    /// A deterministic full-content key for the canonically ordered program.
    ///
    /// Correspondences use the IR's single complete semantic key, including leg
    /// selection, standpoint, determinacy, recovery witnesses and law evidence.
    /// Length framing keeps arbitrary caveat text and collection boundaries distinct.
    /// The versioned identity intentionally invalidates older incomplete handle keys.
    pub fn content_key(&self) -> String {
        let mut key = String::from("correspondence-program-v5;");
        let mut component = |value: &str| {
            write!(key, "{}:{value}", value.len()).expect("writing to a String is infallible");
        };
        component(self.preservation.as_str());
        component(&self.correspondences.len().to_string());
        for correspondence in &self.correspondences {
            component(&correspondence.content_key());
        }
        component(&self.compositions.len().to_string());
        for composition in &self.compositions {
            component(&composition.content_key());
        }
        component(&self.leg_programs.len().to_string());
        for leg in &self.leg_programs {
            component(&leg.iri);
            component(&super::paths::leg_path_canonical(&leg.body));
        }
        key
    }
}

// --------------------------------------------------------------------------- //
// N-Triples rendering helpers (mirror the relational-core projection style)
// --------------------------------------------------------------------------- //

/// A content-stable IRI for a law-claim node under a correspondence (so it survives the
/// round-trip distinctly and deterministically).
fn law_claim_iri(corr_iri: &str, index: usize) -> String {
    format!("{corr_iri}/law-claim/{index}")
}

/// The root of a recovery case's `logic:recoveryTransform` formula tree — the ONE genuinely
/// recursive sub-namespace a [`RecoveryCaseIr`] mints (see [`project_correspondence`]'s
/// `{case.iri}/transform` emission and `super::rdf::emit_formula`'s `{node}/...` child-naming
/// scheme). Every node the formula tree mints is a slash-descendant of exactly this root.
fn recovery_transform_root(case_iri: &str) -> String {
    format!("{case_iri}/transform")
}

/// The complete, precisely-enumerated set of RDF subjects a correspondence projection mints
/// and therefore owns — used by the CGIF/CLIF/XCL meta-channel writers to exclude these
/// subjects from a generic flat-axiom re-derivation (which would otherwise duplicate them
/// under a different lexical form, breaking idempotence; see each writer's `meta_predications`).
///
/// Ownership is intentionally **narrow**:
///
/// * every node this projection mints DIRECTLY (the correspondence individual, the singleton
///   `correspondence-program` wrapper, each recovery case, and each law-claim node) is owned by
///   EXACT IRI match only;
/// * the one node this projection mints RECURSIVELY — a recovery case's `recoveryTransform`
///   formula tree — is owned by prefix match rooted at [`recovery_transform_root`], because
///   [`super::rdf::emit_formula`] mints an unbounded, content-keyed tree of `{node}/...`
///   children under that exact root and no other.
///
/// No other slash-suffix of any owned IRI is owned. An unrelated resource that merely shares a
/// correspondence or recovery-case IRI as a *string* prefix (e.g. a domain resource authored at
/// `<case-iri>/unrelated-node`, which is not a `recoveryTransform` descendant) is never treated
/// as correspondence-owned, so its axioms are never silently dropped from a generated dialect.
pub(crate) struct CorrespondenceOwnership {
    exact: std::collections::HashSet<String>,
    transform_roots: Vec<String>,
}

impl CorrespondenceOwnership {
    /// Build the owned-subject set for `correspondences` (one call per writer invocation; the
    /// writers already hold `program.correspondences` on hand).
    pub(crate) fn build(correspondences: &[Correspondence]) -> Self {
        let mut exact = std::collections::HashSet::new();
        exact.insert(program_iri());
        let mut transform_roots = Vec::new();
        for c in correspondences {
            exact.insert(c.iri.clone());
            for index in 0..c.law_claims.len() {
                exact.insert(law_claim_iri(&c.iri, index));
            }
            for case in &c.recovery_cases {
                exact.insert(case.iri.clone());
                transform_roots.push(recovery_transform_root(&case.iri));
            }
        }
        Self {
            exact,
            transform_roots,
        }
    }

    /// Whether `subject` is a correspondence-owned node under the narrow rule documented on
    /// [`CorrespondenceOwnership`].
    pub(crate) fn owns(&self, subject: &str) -> bool {
        self.exact.contains(subject)
            || self.transform_roots.iter().any(|root| {
                subject == root.as_str()
                    || subject
                        .strip_prefix(root.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
    }
}

/// Project a [`CorrespondenceProgram`] into a deterministic, sorted, byte-stable
/// N-Triples graph — the content folded into the `graph/correspondence` named graph.
///
/// The projection HARD-fails (overclaim gate) if any carried correspondence would
/// surface a class equivalence it may not claim — but a [`CorrespondenceProgram`] never
/// asks for equivalence (it emits the relation-sound alignment predicate), so this is a
/// total function for well-formed input. The gate is exercised independently by
/// [`assert_no_overclaim_correspondence`] (a caller asking an alignment back-end for an
/// equivalence surface).
pub fn project_correspondence(program: &CorrespondenceProgram) -> String {
    let dataset = project_correspondence_dataset(program).expect("valid correspondence projection");
    let bytes = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .expect("serialize correspondence projection");
    let text = String::from_utf8(bytes).expect("N-Triples is UTF-8");
    let mut lines: Vec<_> = text.lines().filter(|line| !line.is_empty()).collect();
    lines.sort_unstable();
    lines.dedup();
    format!("{}\n", lines.join("\n"))
}

/// Build the complete correspondence projection once in native RDF. Downstream
/// stages share this dataset; only a terminal text consumer invokes its codec.
pub fn project_correspondence_dataset(
    program: &CorrespondenceProgram,
) -> gmeow_errors::Result<std::sync::Arc<purrdf::RdfDataset>> {
    let prog = program_iri();
    let mut g = super::rdf::TripleSink::default();

    g.add_iri(&prog, RDF_TYPE, &class_program());
    g.add_iri(&prog, &p_has_preservation(), &program.preservation.iri());
    for c in &program.correspondences {
        validate_loss_evidence(c)?;
        g.add_iri(&prog, &p_has_correspondence(), &c.iri);
        g.add_iri(&c.iri, RDF_TYPE, &class_correspondence());
        if c.grounding {
            g.add_iri(&c.iri, RDF_TYPE, &class_grounding_correspondence());
        }
        g.add_iri(&c.iri, &p_relation(), &c.relation.iri());
        g.add_iri(&c.iri, &p_morphism_class(), &c.morphism_class.iri());
        g.add_iri(&c.iri, &p_morphism_kind(), &c.morphism_kind.iri());
        if c.mnemomorphic {
            g.add_lit(
                &c.iri,
                &p_mnemomorphic(),
                purrdf::RdfLiteral::typed("true", XSD_BOOLEAN),
            );
        }
        if let Some(det) = c.determinacy {
            g.add_iri(&c.iri, &p_determinacy(), &det.iri());
        }
        if let Some(leg) = &c.get_leg {
            g.add_iri(&c.iri, &p_get_leg(), leg);
        }
        if let Some(leg) = &c.put_leg {
            g.add_iri(&c.iri, &p_put_leg(), leg);
        }
        if let (Some(source), Some(target)) = (&c.source_endpoint, &c.target_endpoint) {
            g.add_iri(&c.iri, &p_source_endpoint(), source);
            g.add_iri(&c.iri, &p_target_endpoint(), target);
        }
        if let Some(v) = &c.confidence {
            g.add_lit(&c.iri, &p_confidence(), v.literal().clone());
        }
        if let Some(v) = &c.evidence_strength {
            g.add_lit(&c.iri, &p_evidence_strength(), v.literal().clone());
        }
        if let Some(v) = &c.weight {
            g.add_lit(&c.iri, &p_weight(), v.literal().clone());
        }
        if let Some(v) = &c.probability {
            g.add_lit(&c.iri, &p_probability(), v.literal().clone());
        }
        if let Some(at) = &c.according_to {
            g.add_iri(&c.iri, &p_according_to(), at);
        }
        // The per-correspondence preservation judgment (`logic:preservationKind`): the
        // Principle-17 loss residue this cell carries. Emitted only when authored, so a
        // preservation-free correspondence round-trips byte-identically (append-only).
        if let Some(pres) = c.preservation {
            g.add_iri(&c.iri, &p_preservation_kind(), &pres.iri());
        }
        for evidence in &c.loss_evidence {
            g.add_lit(&c.iri, &p_lossy_drop(), evidence.clone());
        }
        // The relation-sound SSSOM alignment surface (the load-bearing decision): the
        // legs co-project onto a shared apex, so the alignment links the two legs to the
        // apex via the relation-sound predicate (relatedMatch for an overlap, NEVER
        // exactMatch / owl:equivalentClass). Emitted leg→apex so the apex is the object.
        if let Some(pred) = alignment_predicate(c.relation, c.morphism_kind) {
            for leg in [c.get_leg.as_deref(), c.put_leg.as_deref()]
                .into_iter()
                .flatten()
            {
                g.add_iri(leg, pred, &c.iri);
            }
        }
        // Law claims, each as a content-IRI node.
        for (index, claim) in c.law_claims.iter().enumerate() {
            let claim_iri = law_claim_iri(&c.iri, index);
            g.add_iri(&c.iri, &p_has_law_claim(), &claim_iri);
            g.add_iri(&claim_iri, RDF_TYPE, &class_law_claim());
            g.add_iri(&claim_iri, &p_law_claimed(), &claim.law.iri());
            g.add_iri(&claim_iri, &p_law_verdict(), &claim.verdict.iri());
            if let Some(cond) = claim.condition {
                g.add_iri(&claim_iri, &p_law_condition(), &cond.iri());
            }
        }
        // First-class, correspondence-owned recovery evidence.  The formula tree uses the
        // same canonical RDF 1.2 emitter as every other logic:Formula; only its ownership
        // differs, so it can round-trip without becoming a second top-level assertion.
        for case in &c.recovery_cases {
            let transform_iri = format!("{}/transform", case.iri);
            g.add_iri(&c.iri, &p_recovery_case(), &case.iri);
            g.add_iri(&case.iri, RDF_TYPE, &class_recovery_case());
            g.add_iri(&case.iri, &p_recovery_transform(), &transform_iri);
            super::rdf::emit_formula(&mut g, &transform_iri, &case.transform);
        }
        // Caveats for this correspondence.
        for evidence in &c.axis_evidence.sources {
            g.add_iri(
                &c.iri,
                &format!("{LOGIC_NAMESPACE}evidenceSource"),
                evidence,
            );
        }
        for (predicate, value) in [
            ("evidenceScale", &c.axis_evidence.scale),
            (
                "crossChainProbabilityModel",
                &c.axis_evidence.probability_model,
            ),
        ] {
            if let Some(value) = value {
                g.add_iri(&c.iri, &format!("{LOGIC_NAMESPACE}{predicate}"), value);
            }
        }
        for caveat in &c.caveats {
            g.add_iri(&c.iri, &p_has_caveat(), &caveat.iri);
            g.add_iri(&caveat.iri, RDF_TYPE, &class_caveat());
            for comment in &caveat.comments {
                g.add_lit(&caveat.iri, RDFS_COMMENT, comment.clone());
            }
        }
    }

    for composition in &program.compositions {
        g.add_iri(
            &prog,
            &format!("{LOGIC_NAMESPACE}hasComposition"),
            &composition.iri,
        );
        g.add_iri(
            &composition.iri,
            RDF_TYPE,
            &format!("{LOGIC_NAMESPACE}CorrespondenceComposition"),
        );
        for rule in &composition.axis_rules {
            g.add_iri(
                &composition.iri,
                &format!("{LOGIC_NAMESPACE}compositionAxisRule"),
                rule,
            );
        }
        for (predicate, value) in [
            (
                "confidenceIndependenceEvidence",
                &composition.confidence_independence,
            ),
            (
                "probabilityIndependenceEvidence",
                &composition.probability_independence,
            ),
        ] {
            if let Some(value) = value {
                g.add_iri(
                    &composition.iri,
                    &format!("{LOGIC_NAMESPACE}{predicate}"),
                    value,
                );
            }
        }
        for (predicate, value) in [
            ("compositionFirst", &composition.first),
            ("compositionSecond", &composition.second),
            ("compositionResult", &composition.composite),
        ] {
            g.add_iri(
                &composition.iri,
                &format!("{LOGIC_NAMESPACE}{predicate}"),
                value,
            );
        }
    }

    // Leg programs are part of the typed correspondence program's content identity.
    // Project their realized bodies into the same graph so cache re-derivation and
    // independently authored programs retain executable composition semantics.
    for leg in &program.leg_programs {
        let mut counter = 0;
        let body = emit_leg_path(&mut g, &leg.iri, &leg.body, &mut counter)?;
        g.add_iri(&leg.iri, &gm("path"), &body);
    }

    g.builder.freeze().map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::Projection {
            detail: format!("freeze correspondence projection: {error}"),
        })
    })
}

// --------------------------------------------------------------------------- //
// Reverse: graph → CorrespondenceProgram (the cache-hit re-derivation)
// --------------------------------------------------------------------------- //

/// A borrowed view over the selected native default graph. PurRDF owns the indexes;
/// constructing this view neither copies source terms nor scans/materializes a dataset.
struct CorrespondenceView<'source> {
    dataset: &'source purrdf::RdfDataset,
}

impl<'source> CorrespondenceView<'source> {
    fn from_dataset(dataset: &'source purrdf::RdfDataset) -> Self {
        Self { dataset }
    }

    fn objects(
        &self,
        subject: &str,
        predicate: &str,
    ) -> impl Iterator<Item = purrdf::TermRef<'source>> + 'source {
        let dataset = self.dataset;
        let pair = dataset
            .term_id_by_iri(subject)
            .zip(dataset.term_id_by_iri(predicate));
        pair.into_iter().flat_map(move |(subject, predicate)| {
            crate::graphutil::default_graph_pattern(dataset, Some(subject), Some(predicate), None)
                .map(move |quad| dataset.resolve(quad.o))
        })
    }

    /// A scalar is either absent or exactly one distinct native RDF value. Physical
    /// duplicate rows have already been deduplicated by the shared graph boundary.
    fn scalar(&self, s: &str, p: &str) -> gmeow_errors::Result<Option<purrdf::TermRef<'source>>> {
        let mut objects = self.objects(s, p);
        let value = objects.next();
        if objects.next().is_some() {
            return Err(field_error(s, p, "requires at most one distinct value"));
        }
        Ok(value)
    }

    fn iri_obj(&self, s: &str, p: &str) -> gmeow_errors::Result<Option<&'source str>> {
        self.scalar(s, p)?
            .map(|object| match object {
                purrdf::TermRef::Iri(iri) => Ok(iri),
                _ => Err(field_error(s, p, "requires an IRI value")),
            })
            .transpose()
    }

    fn required_iri(&self, s: &str, p: &str) -> gmeow_errors::Result<&'source str> {
        self.iri_obj(s, p)?
            .ok_or_else(|| field_error(s, p, "requires one IRI value"))
    }

    fn lit_obj(&self, s: &str, p: &str) -> gmeow_errors::Result<Option<&'source str>> {
        self.scalar(s, p)?
            .map(|object| match object {
                purrdf::TermRef::Literal { lexical, .. } => Ok(lexical),
                _ => Err(field_error(s, p, "requires a literal value")),
            })
            .transpose()
    }

    fn numeric_obj<const UNIT: bool>(
        &self,
        s: &str,
        p: &str,
    ) -> gmeow_errors::Result<Option<crate::ir::NumericLiteral<UNIT>>> {
        self.scalar(s, p)?
            .map(|term| {
                crate::ir::NumericLiteral::from_term(self.dataset, term)
                    .map_err(|error| field_error(s, p, error.to_string()))
            })
            .transpose()
    }

    fn boolean_obj(&self, s: &str, p: &str) -> gmeow_errors::Result<Option<bool>> {
        self.lit_obj(s, p)?
            .map(|lexical| {
                let value = purrdf::xsd::parse(lexical, purrdf::xsd::XsdDatatype::Boolean)
                    .map_err(|error| {
                        field_error(s, p, format!("invalid boolean value: {error}"))
                    })?;
                match value {
                    purrdf::xsd::XsdValue::Boolean(value) => Ok(value),
                    _ => unreachable!("the upstream boolean decoder returns a boolean"),
                }
            })
            .transpose()
    }

    fn enum_obj<T>(
        &self,
        s: &str,
        p: &str,
        parse: impl FnOnce(&str) -> Option<T>,
    ) -> gmeow_errors::Result<Option<T>> {
        self.iri_obj(s, p)?
            .map(|iri| {
                iri.strip_prefix(LOGIC_NAMESPACE)
                    .and_then(parse)
                    .ok_or_else(|| {
                        field_error(s, p, format!("unknown canonical logic value <{iri}>"))
                    })
            })
            .transpose()
    }

    fn required_enum<T>(
        &self,
        s: &str,
        p: &str,
        parse: impl FnOnce(&str) -> Option<T>,
    ) -> gmeow_errors::Result<T> {
        self.enum_obj(s, p, parse)?
            .ok_or_else(|| field_error(s, p, "requires one canonical logic value"))
    }

    fn iri_objs(&self, s: &str, p: &str) -> gmeow_errors::Result<Vec<&'source str>> {
        let mut out: Vec<_> = self
            .objects(s, p)
            .map(|object| match object {
                purrdf::TermRef::Iri(iri) => Ok(iri),
                _ => Err(field_error(
                    s,
                    p,
                    "has a non-IRI member; every declared member is required",
                )),
            })
            .collect::<gmeow_errors::Result<_>>()?;
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    fn types_of(&self, s: &str) -> gmeow_errors::Result<Vec<&'source str>> {
        self.iri_objs(s, RDF_TYPE)
    }

    fn structural_types_of(&self, subject: &str) -> gmeow_errors::Result<Vec<&'source str>> {
        let mut classes = self.types_of(subject)?;
        classes.extend(
            self.iri_objs(subject, "https://blackcatinformatics.ca/logic/instanceOf")?
                .into_iter()
                .filter(|class| {
                    is_structural_type_predicate(
                        "https://blackcatinformatics.ca/logic/instanceOf",
                        class,
                    )
                }),
        );
        classes.sort_unstable();
        classes.dedup();
        Ok(classes)
    }
}

fn field_error(subject: &str, predicate: &str, detail: impl std::fmt::Display) -> Diag {
    Diag::of_kind(crate::error::Correspondence {
        detail: format!("<{subject}> <{predicate}> {detail}"),
    })
}

/// Read ONE [`Correspondence`] node by its IRI from the borrowed native view. The single reader
/// behind both the cache re-derivation and the frontend extractor; returns a hard error
/// (the caller decides whether to propagate it or downgrade it to a diagnostic).
fn read_correspondence(
    reader: &mut crate::frontend::FormulaReader<'_>,
    idx: &CorrespondenceView<'_>,
    corr_iri: &str,
) -> gmeow_errors::Result<Correspondence> {
    let dataset = reader.dataset();
    let relation =
        idx.required_enum(corr_iri, &p_relation(), CorrespondenceRelation::from_local)?;
    let morphism_class = idx.required_enum(
        corr_iri,
        &p_morphism_class(),
        crate::ir::MorphismClass::from_local,
    )?;
    let morphism_kind =
        idx.required_enum(corr_iri, &p_morphism_kind(), MorphismKind::from_local)?;
    let mnemomorphic = idx
        .boolean_obj(corr_iri, &p_mnemomorphic())?
        .unwrap_or(false);
    let determinacy = idx.enum_obj(
        corr_iri,
        &p_determinacy(),
        crate::ir::Determinacy::from_local,
    )?;
    let get_leg = idx.iri_obj(corr_iri, &p_get_leg())?.map(str::to_owned);
    let put_leg = idx.iri_obj(corr_iri, &p_put_leg())?.map(str::to_owned);
    let source_endpoint = idx
        .iri_obj(corr_iri, &p_source_endpoint())?
        .map(str::to_owned);
    let target_endpoint = idx
        .iri_obj(corr_iri, &p_target_endpoint())?
        .map(str::to_owned);
    if source_endpoint.is_some() != target_endpoint.is_some() {
        return Err(Diag::of_kind(crate::error::Correspondence {
            detail: format!(
                "correspondence <{corr_iri}> must carry both sourceEndpoint and targetEndpoint, \
                 or neither"
            ),
        }));
    }
    let grounding = idx
        .structural_types_of(corr_iri)?
        .contains(&class_grounding_correspondence().as_str());
    let according_to = idx.iri_obj(corr_iri, &p_according_to())?.map(str::to_owned);
    // The per-correspondence preservation judgment (`logic:preservationKind`), DISTINCT
    // from the program-level `hasPreservation`. Absent ⇒ the cell authors no rung (None).
    let preservation = idx.enum_obj(corr_iri, &p_preservation_kind(), preservation_from_local)?;

    let claim_nodes = idx.iri_objs(corr_iri, &p_has_law_claim())?;
    let mut law_claims = Vec::new();
    for claim_iri in claim_nodes {
        let law = idx.required_enum(
            claim_iri,
            &p_law_claimed(),
            crate::ir::CorrespondenceLaw::from_local,
        )?;
        let verdict = idx.required_enum(
            claim_iri,
            &p_law_verdict(),
            crate::ir::DischargeVerdict::from_local,
        )?;
        let condition = idx.enum_obj(
            claim_iri,
            &p_law_condition(),
            crate::ir::DischargeCondition::from_local,
        )?;
        law_claims.push(crate::ir::LawClaimIr {
            law,
            verdict,
            condition,
        });
    }

    let mut recovery_cases = Vec::new();
    for case_iri in idx.iri_objs(corr_iri, &p_recovery_case())? {
        if !idx
            .structural_types_of(case_iri)?
            .contains(&class_recovery_case().as_str())
        {
            return Err(Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "recovery case <{case_iri}> on correspondence <{corr_iri}> is not typed logic:RecoveryCase"
                ),
            }));
        }
        let transforms = objects(
            dataset,
            &Subject::Iri(case_iri.to_owned()),
            &nn(&p_recovery_transform()),
        );
        if transforms.len() != 1 {
            return Err(Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "recovery case <{case_iri}> on correspondence <{corr_iri}> requires exactly one logic:recoveryTransform; found {}",
                    transforms.len()
                ),
            }));
        }
        let transform_subject = term_as_subject(&transforms[0]).ok_or_else(|| {
            Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "recovery case <{case_iri}> on correspondence <{corr_iri}> has a non-resource logic:recoveryTransform"
                ),
            })
        })?;
        let transform = reader.read(&transform_subject)?;
        recovery_cases.push(RecoveryCaseIr::new(case_iri, transform)?);
    }

    let mut correspondence = Correspondence::new(
        corr_iri.to_owned(),
        relation,
        morphism_class,
        morphism_kind,
        mnemomorphic,
        determinacy,
        get_leg,
        put_leg,
        law_claims,
        idx.numeric_obj(corr_iri, &p_confidence())?,
        idx.numeric_obj(corr_iri, &p_evidence_strength())?,
        idx.numeric_obj(corr_iri, &p_weight())?,
        idx.numeric_obj(corr_iri, &p_probability())?,
        according_to,
        preservation,
    )?;
    if let (Some(source), Some(target)) = (source_endpoint, target_endpoint) {
        correspondence = correspondence.with_endpoints(source, target)?;
    }
    if grounding {
        correspondence = correspondence.as_grounding();
    }
    correspondence = correspondence.with_recovery_cases(recovery_cases)?;
    correspondence = correspondence.with_caveats(read_caveats(idx, corr_iri)?)?;
    correspondence =
        correspondence.with_loss_evidence(read_literals(idx, corr_iri, &p_lossy_drop())?)?;
    correspondence = correspondence.with_axis_evidence(crate::ir::AxisEvidence::new(
        idx.iri_objs(corr_iri, &format!("{LOGIC_NAMESPACE}evidenceSource"))?
            .into_iter()
            .map(str::to_owned)
            .collect(),
        idx.iri_obj(corr_iri, &format!("{LOGIC_NAMESPACE}evidenceScale"))?
            .map(str::to_owned),
        idx.iri_obj(
            corr_iri,
            &format!("{LOGIC_NAMESPACE}crossChainProbabilityModel"),
        )?
        .map(str::to_owned),
    )?);
    validate_loss_evidence(&correspondence)?;
    Ok(correspondence)
}

/// Loss evidence is independent of relation/class and must agree with the authored
/// judgment before any identity labels or human-readable report notes are added.
pub(crate) fn validate_loss_evidence(c: &Correspondence) -> gmeow_errors::Result<()> {
    let Some(preservation) = c.preservation else {
        if c.loss_evidence.is_empty() {
            return Ok(());
        }
        return Err(field_error(
            &c.iri,
            &p_lossy_drop(),
            "requires logic:preservationKind",
        ));
    };
    for literal in &c.loss_evidence {
        if literal.lexical_form.trim().is_empty() {
            return Err(field_error(
                &c.iri,
                &p_lossy_drop(),
                "requires non-blank evidence",
            ));
        }
        purrdf::RdfLiteral::validate_components(
            literal.datatype_iri(),
            literal.language.as_deref(),
            literal.direction,
        )
        .map_err(|error| field_error(&c.iri, &p_lossy_drop(), error))?;
    }
    let residue: Vec<&str> = c
        .loss_evidence
        .iter()
        .map(|literal| literal.lexical_form.as_str())
        .collect();
    super::assert_no_overclaim(&c.iri, preservation, &residue)
        .map_err(|error| field_error(&c.iri, &p_lossy_drop(), error))
}

/// Read every native literal without choosing a language or discarding components.
fn read_literals(
    idx: &CorrespondenceView<'_>,
    subject: &str,
    predicate: &str,
) -> gmeow_errors::Result<Vec<purrdf::RdfLiteral>> {
    idx.objects(subject, predicate)
        .map(|term| {
            let purrdf::TermRef::Literal {
                lexical,
                datatype,
                language,
                direction,
            } = term
            else {
                return Err(field_error(
                    subject,
                    predicate,
                    "requires every value to be a literal",
                ));
            };
            let purrdf::TermRef::Iri(datatype) = idx.dataset.resolve(datatype) else {
                return Err(field_error(subject, predicate, "requires an IRI datatype"));
            };
            Ok(purrdf::RdfLiteral {
                lexical_form: lexical.to_owned(),
                datatype: Some(datatype.to_owned()),
                language: language.map(str::to_owned),
                direction,
            })
        })
        .collect()
}

/// Read the caveats attached to one correspondence. Each `hasCaveat` object carries an
/// `rdfs:comment`; HARD-fail if the text is absent (a caveat without text is corrupt,
/// never a silently-empty comment — no-optionality).
fn read_caveats(
    idx: &CorrespondenceView<'_>,
    corr_iri: &str,
) -> gmeow_errors::Result<Vec<CorrespondenceCaveat>> {
    let mut caveats = Vec::new();
    for caveat_iri in idx.iri_objs(corr_iri, &p_has_caveat())? {
        let comments = read_literals(idx, caveat_iri, RDFS_COMMENT)?;
        if comments.is_empty() {
            return Err(field_error(
                caveat_iri,
                RDFS_COMMENT,
                "requires at least one comment",
            ));
        }
        caveats.push(CorrespondenceCaveat {
            iri: caveat_iri.to_owned(),
            comments,
        });
    }
    Ok(caveats)
}

/// Re-derive a [`CorrespondenceProgram`] from its backing `graph/correspondence`
/// N-Triples — the inverse of [`project_correspondence`], used by the cache on a hit.
///
/// HARD-fails on a malformed graph (no-optionality): a backing graph that no longer
/// re-derives is a corrupt cache, never a silently-dropped handle.
pub fn parse_correspondence(
    dataset: &purrdf::RdfDataset,
) -> gmeow_errors::Result<CorrespondenceProgram> {
    let mut reader = crate::frontend::FormulaReader::new(dataset);
    let idx = CorrespondenceView::from_dataset(dataset);

    let prog = program_iri();
    let preservation = idx.required_enum(&prog, &p_has_preservation(), preservation_from_local)?;
    if idx.objects(&prog, &p_lossy_drop()).next().is_some() {
        return Err(field_error(
            &prog,
            &p_lossy_drop(),
            "requires correspondence-owned evidence; program-wide loss attribution is unsupported",
        ));
    }

    // Each hasCorrespondence object is a Correspondence subject.
    let mut correspondences = Vec::new();
    for corr_iri in idx.iri_objs(&prog, &p_has_correspondence())? {
        correspondences.push(read_correspondence(&mut reader, &idx, corr_iri)?);
    }

    let compositions = idx
        .iri_objs(&prog, &format!("{LOGIC_NAMESPACE}hasComposition"))?
        .into_iter()
        .map(|iri| read_composition(&idx, iri))
        .collect::<gmeow_errors::Result<_>>()?;
    let leg_programs = extract_present_leg_programs(dataset, &correspondences)?;
    Ok(CorrespondenceProgram::new(correspondences, preservation)
        .with_compositions(compositions)
        .with_leg_programs(leg_programs))
}

/// Extract every authored `logic:Correspondence` individual from a dataset — the
/// frontend's gate input — independent of any program wrapper (a conformance
/// `input.logic.ttl` authors the bare individuals the same shape
/// [`project_correspondence`] emits). Returns the well-formed correspondences (sorted by
/// IRI, canonicalized by the ctor) plus a per-node `(iri, message)` for each malformed
/// one: the caller surfaces those as diagnostics so no node is silently dropped, while a
/// single malformed cell never poisons the rest (the frontend is fail-soft, unlike the
/// hard-fail cache re-derivation in [`parse_correspondence`]).
pub fn extract_correspondences(
    dataset: &purrdf::RdfDataset,
) -> (Vec<Correspondence>, Vec<(String, String)>) {
    extract_correspondences_with_reader(&mut crate::frontend::FormulaReader::new(dataset))
}

/// Reuse the compiler session while keeping every correspondence occurrence owned.
pub(crate) fn extract_correspondences_with_reader(
    reader: &mut crate::frontend::FormulaReader<'_>,
) -> (Vec<Correspondence>, Vec<(String, String)>) {
    let dataset = reader.dataset();
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for subject in crate::graphutil::subjects_of_structural_class(
        dataset,
        &crate::graphutil::Node::iri(class_correspondence()),
    ) {
        match read_source_correspondence(reader, &subject) {
            Ok(c) => ok.push(c),
            Err(msg) => errors.push((
                crate::graphutil::subject_str(&subject),
                msg.message().to_owned(),
            )),
        }
    }
    ok.sort_by(|a, b| a.iri.cmp(&b.iri));
    (ok, errors)
}

/// Read the actual discovered source root. Anonymous correspondence identities are
/// reported as unlowered rather than disappearing from an IRI-only secondary index.
pub(crate) fn read_source_correspondence(
    reader: &mut crate::frontend::FormulaReader<'_>,
    source: &crate::graphutil::Subject,
) -> gmeow_errors::Result<Correspondence> {
    let crate::graphutil::Subject::Iri(iri) = source else {
        return Err(Diag::of_kind(crate::error::Correspondence {
            detail: "anonymous correspondence identity has no lowering to the current named IR"
                .into(),
        }));
    };
    read_correspondence(
        reader,
        &CorrespondenceView::from_dataset(reader.dataset()),
        iri,
    )
}

/// Read one named source declaration through the existing native graph indexes.
pub(crate) fn read_source_composition(
    dataset: &purrdf::RdfDataset,
    source: crate::frontend::SourceNode,
) -> gmeow_errors::Result<crate::ir::CorrespondenceComposition> {
    let purrdf::TermRef::Iri(iri) = dataset.resolve(source.term) else {
        return Err(field_error(
            "anonymous composition",
            "identity",
            "requires a named IRI",
        ));
    };
    read_composition(&CorrespondenceView::from_dataset(dataset), iri)
}

fn read_composition(
    idx: &CorrespondenceView<'_>,
    iri: &str,
) -> gmeow_errors::Result<crate::ir::CorrespondenceComposition> {
    if !crate::graphutil::has_structural_class(
        idx.dataset,
        &crate::graphutil::Subject::Iri(iri.to_owned()),
        &crate::graphutil::Node::iri(format!("{LOGIC_NAMESPACE}CorrespondenceComposition")),
    ) {
        return Err(field_error(
            iri,
            RDF_TYPE,
            "requires an explicit logic:CorrespondenceComposition declaration",
        ));
    }
    crate::ir::CorrespondenceComposition::new(
        iri.to_owned(),
        idx.required_iri(iri, &format!("{LOGIC_NAMESPACE}compositionFirst"))?
            .to_owned(),
        idx.required_iri(iri, &format!("{LOGIC_NAMESPACE}compositionSecond"))?
            .to_owned(),
        idx.required_iri(iri, &format!("{LOGIC_NAMESPACE}compositionResult"))?
            .to_owned(),
    )?
    .with_axis_rules(
        idx.iri_objs(iri, &format!("{LOGIC_NAMESPACE}compositionAxisRule"))?
            .into_iter()
            .map(str::to_owned)
            .collect(),
        idx.iri_obj(
            iri,
            &format!("{LOGIC_NAMESPACE}confidenceIndependenceEvidence"),
        )?
        .map(str::to_owned),
        idx.iri_obj(
            iri,
            &format!("{LOGIC_NAMESPACE}probabilityIndependenceEvidence"),
        )?
        .map(str::to_owned),
    )
}

const GM_NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
fn gm(local: &str) -> String {
    format!("{GM_NAMESPACE}{local}")
}

/// Project one typed leg body into the native correspondence graph. Structural
/// nodes are deterministic descendants of the owning leg, matching the parser's
/// closed path algebra without serializing and reparsing an intermediate syntax.
fn emit_leg_path(
    sink: &mut super::rdf::TripleSink,
    base: &str,
    path: &LegPath,
    counter: &mut usize,
) -> gmeow_errors::Result<String> {
    match path {
        LegPath::Step(predicate) => Ok(predicate.clone()),
        LegPath::Inverse(inner) => {
            let node = format!("{base}/legnode/{counter}");
            *counter += 1;
            sink.add_iri(&node, RDF_TYPE, &gm("InversePath"));
            let inner = emit_leg_path(sink, base, inner, counter)?;
            sink.add_iri(&node, &gm("pathStep"), &inner);
            Ok(node)
        }
        LegPath::Seq(members) => emit_leg_list(
            sink,
            base,
            members,
            &gm("SeqPath"),
            &gm("pathSteps"),
            counter,
        ),
        LegPath::Alt(members) => emit_leg_list(
            sink,
            base,
            members,
            &gm("AltPath"),
            &gm("pathAlts"),
            counter,
        ),
    }
}

fn emit_leg_list(
    sink: &mut super::rdf::TripleSink,
    base: &str,
    members: &[LegPath],
    path_type: &str,
    head_predicate: &str,
    counter: &mut usize,
) -> gmeow_errors::Result<String> {
    if members.is_empty() {
        return Err(field_error(
            base,
            &gm("path"),
            "cannot project an empty path constructor",
        ));
    }
    let node = format!("{base}/legnode/{counter}");
    *counter += 1;
    sink.add_iri(&node, RDF_TYPE, path_type);
    let cells: Vec<_> = members
        .iter()
        .map(|_| {
            let cell = format!("{base}/legcell/{counter}");
            *counter += 1;
            cell
        })
        .collect();
    for (index, member) in members.iter().enumerate() {
        let item = emit_leg_path(sink, base, member, counter)?;
        sink.add_iri(&cells[index], &gm("pathItem"), &item);
        if let Some(next) = cells.get(index + 1) {
            sink.add_iri(&cells[index], &gm("pathNext"), next);
        }
    }
    sink.add_iri(&node, head_predicate, &cells[0]);
    Ok(node)
}

/// Recursively parse a `gm:` leg-path node into a [`LegPath`]. The supported forms (the
/// canonical `logic:` composite-path vocabulary the transaction layer already uses) are:
///
/// * a bare predicate IRI (no `gm:*Path` type) → [`LegPath::Step`];
/// * `gm:InversePath` with `gm:pathStep` → [`LegPath::Inverse`];
/// * `gm:SeqPath` with `gm:pathSteps` (a `gm:pathNext`-linked chain of named nodes) →
///   [`LegPath::Seq`];
/// * `gm:AltPath` with `gm:pathAlts` (likewise) → [`LegPath::Alt`].
///
/// Every selected constructor, member and tail is mandatory. Malformed fields and
/// resource limits return an error; they never shorten the selected program.
fn parse_leg_path<'source>(
    idx: &CorrespondenceView<'source>,
    node: &'source str,
    depth: u32,
    active: &mut std::collections::HashSet<&'source str>,
) -> gmeow_errors::Result<LegPath> {
    if depth > 64 {
        return Err(field_error(
            node,
            &gm("path"),
            "path nesting exceeds the supported depth of 64",
        ));
    }
    if !active.insert(node) {
        return Err(field_error(node, &gm("path"), "cyclic path constructor"));
    }
    let result = (|| {
        let types = idx.types_of(node)?;
        let inverse_type = gm("InversePath");
        let seq_type = gm("SeqPath");
        let alt_type = gm("AltPath");
        let constructors: Vec<_> = types
            .iter()
            .copied()
            .filter(|ty| [inverse_type.as_str(), seq_type.as_str(), alt_type.as_str()].contains(ty))
            .collect();
        if constructors.len() > 1 {
            return Err(field_error(
                node,
                RDF_TYPE,
                "declares conflicting path constructors",
            ));
        }
        if types.iter().any(|ty| {
            ty.starts_with(GM_NAMESPACE)
                && ty.ends_with("Path")
                && ![inverse_type.as_str(), seq_type.as_str(), alt_type.as_str()].contains(ty)
        }) {
            return Err(field_error(
                node,
                RDF_TYPE,
                "declares an unsupported path constructor",
            ));
        }
        let step = idx.iri_obj(node, &gm("pathStep"))?;
        let seq = idx.iri_obj(node, &gm("pathSteps"))?;
        let alt = idx.iri_obj(node, &gm("pathAlts"))?;
        match (constructors.first().copied(), step, seq, alt) {
            (None, None, None, None) => Ok(LegPath::Step(node.to_owned())),
            (Some(ty), Some(step), None, None) if ty == inverse_type => Ok(LegPath::Inverse(
                Box::new(parse_leg_path(idx, step, depth + 1, active)?),
            )),
            (Some(ty), None, Some(head), None) if ty == seq_type => {
                Ok(LegPath::Seq(leg_path_members(idx, head, depth, active)?))
            }
            (Some(ty), None, None, Some(head)) if ty == alt_type => {
                Ok(LegPath::Alt(leg_path_members(idx, head, depth, active)?))
            }
            _ => Err(field_error(
                node,
                &gm("path"),
                "path fields do not match exactly one declared constructor",
            )),
        }
    })();
    active.remove(node);
    result
}

/// Read a nonempty linked member list. A present malformed pathNext is an error,
/// not an absent tail. Both constructor and list-cycle guards retain bounded state.
fn leg_path_members<'source>(
    idx: &CorrespondenceView<'source>,
    head: &'source str,
    depth: u32,
    active: &mut std::collections::HashSet<&'source str>,
) -> gmeow_errors::Result<Vec<LegPath>> {
    let mut out = Vec::new();
    let mut cursor = Some(head);
    let mut visited = std::collections::HashSet::new();
    while let Some(cell) = cursor {
        if !visited.insert(cell) {
            return Err(field_error(
                cell,
                &gm("pathNext"),
                "cyclic path member list",
            ));
        }
        if visited.len() > 256 {
            return Err(field_error(
                head,
                &gm("pathNext"),
                "path member list exceeds the supported length of 256",
            ));
        }
        let item = idx.required_iri(cell, &gm("pathItem"))?;
        out.push(parse_leg_path(idx, item, depth + 1, active)?);
        cursor = idx.iri_obj(cell, &gm("pathNext"))?;
    }
    Ok(out)
}

/// Read each referenced leg once, in IRI order. Failure of any selected program
/// rejects the registry rather than publishing a partial list of executable legs.
///
/// # Errors
/// Refuses missing, malformed or unsupported selected path programs.
pub fn extract_leg_programs(
    dataset: &purrdf::RdfDataset,
    correspondences: &[Correspondence],
) -> gmeow_errors::Result<Vec<TransactionProgramIr>> {
    let leg_iris: std::collections::BTreeSet<_> = correspondences
        .iter()
        .flat_map(|c| c.get_leg.iter().chain(c.put_leg.iter()).map(String::as_str))
        .collect();
    leg_iris
        .into_iter()
        .map(|iri| read_source_leg_program(dataset, iri))
        .collect()
}

/// Reconstruct every referenced leg body actually carried by this graph. A leg IRI
/// without `gm:path` remains an explicit symbolic leg; malformed selected bodies fail.
/// Physical consumers separately require executable bodies for every operation they
/// select, so absence can never degrade a requested composition silently.
fn extract_present_leg_programs(
    dataset: &purrdf::RdfDataset,
    correspondences: &[Correspondence],
) -> gmeow_errors::Result<Vec<TransactionProgramIr>> {
    let idx = CorrespondenceView::from_dataset(dataset);
    let leg_iris: std::collections::BTreeSet<_> = correspondences
        .iter()
        .flat_map(|correspondence| {
            correspondence
                .get_leg
                .iter()
                .chain(correspondence.put_leg.iter())
                .map(String::as_str)
        })
        .collect();
    leg_iris
        .into_iter()
        .filter_map(|iri| match idx.iri_obj(iri, &gm("path")) {
            Ok(Some(_)) => Some(read_source_leg_program(dataset, iri)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

pub(crate) fn read_source_leg_program(
    dataset: &purrdf::RdfDataset,
    iri: &str,
) -> gmeow_errors::Result<TransactionProgramIr> {
    let idx = CorrespondenceView::from_dataset(dataset);
    let path = idx.required_iri(iri, &gm("path"))?;
    let body = parse_leg_path(&idx, path, 0, &mut std::collections::HashSet::new())?;
    Ok(TransactionProgramIr {
        iri: iri.to_owned(),
        body,
    })
}

/// Inverse of [`PreservationKind::as_str`] for the kinds this lane uses.
fn preservation_from_local(local: &str) -> Option<PreservationKind> {
    Some(match local {
        "ExactPreservation" => PreservationKind::Exact,
        "SoundUnderApproximation" => PreservationKind::SoundUnder,
        "CompleteOverApproximation" => PreservationKind::CompleteOver,
        "ValidationOnly" => PreservationKind::ValidationOnly,
        "InconsistencyPreserving" => PreservationKind::InconsistencyPreserving,
        "InconsistencyReflecting" => PreservationKind::InconsistencyReflecting,
        "Unsupported" => PreservationKind::Unsupported,
        _ => return None,
    })
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub use test_support::affine_triangle_worked_example;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod field_tests;

#[cfg(test)]
mod composition_tests;

#[cfg(test)]
mod loss_tests;
