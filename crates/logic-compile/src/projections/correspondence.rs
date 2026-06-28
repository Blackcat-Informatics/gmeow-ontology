// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `logic:Correspondence` carrier lane (#1132 C10): project a set of typed
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

use std::collections::BTreeMap;

use crate::ir::{
    Correspondence, CorrespondenceRelation, MorphismKind, PreservationKind, LOGIC_NAMESPACE,
};

use super::OverclaimError;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const SKOS_RELATED_MATCH: &str = "http://www.w3.org/2004/02/skos/core#relatedMatch";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

/// The local predicates/classes this projection mints under `LOGIC_NAMESPACE`. Kept as
/// small builder functions (string constants would duplicate the namespace prefix).
fn class_program() -> String {
    format!("{LOGIC_NAMESPACE}CorrespondenceProgram")
}
fn class_correspondence() -> String {
    format!("{LOGIC_NAMESPACE}Correspondence")
}
fn class_caveat() -> String {
    format!("{LOGIC_NAMESPACE}CorrespondenceCaveat")
}
fn class_law_claim() -> String {
    format!("{LOGIC_NAMESPACE}LawClaim")
}
fn p_has_correspondence() -> String {
    format!("{LOGIC_NAMESPACE}hasCorrespondence")
}
fn p_has_preservation() -> String {
    format!("{LOGIC_NAMESPACE}hasPreservation")
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
    format!("{LOGIC_NAMESPACE}accordingTo")
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

/// A caveat on a correspondence: the human-readable warning that the two terms are not
/// entity-equivalent (the §14 "they are not equivalent and neither subsumes the other"
/// note). Carried by-reference (an IRI) plus its definition text so the projection emits
/// it deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrespondenceCaveat {
    /// IRI of the caveat individual.
    pub iri: String,
    /// The caveat's `skos:definition`-equivalent text (rendered as `rdfs:comment`).
    pub text: String,
}

/// A compiled set of [`Correspondence`] nodes plus their caveats and the declared
/// preservation polarity — the typed payload the `PipelineHandle::Correspondence` arm
/// carries (#1132 C10). One content identity ([`CorrespondenceProgram::content_key`])
/// across the typed handle and its backing `graph/correspondence` projection.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrespondenceProgram {
    /// The correspondences, canonically sorted by IRI at construction.
    pub correspondences: Vec<Correspondence>,
    /// The caveats, canonically sorted by IRI at construction. A correspondence
    /// references its caveats via [`Correspondence`]'s caveat link (`logic:hasCaveat`),
    /// emitted positionally from this sorted list keyed by correspondence IRI.
    pub caveats: Vec<(String, CorrespondenceCaveat)>,
    /// The declared preservation polarity for this lane (the loss-ledger row): a
    /// caveated overlap is a `SoundUnderApproximation` (it under-approximates the
    /// forced-equality reading it refuses), never `ExactPreservation`.
    pub preservation: PreservationKind,
}

impl CorrespondenceProgram {
    /// Construct, canonicalizing the collections into sorted order so the content
    /// identity is construction-order-independent.
    pub fn new(
        correspondences: Vec<Correspondence>,
        caveats: Vec<(String, CorrespondenceCaveat)>,
        preservation: PreservationKind,
    ) -> Self {
        let mut correspondences = correspondences;
        correspondences.sort_by(|a, b| a.iri.cmp(&b.iri));
        let mut caveats = caveats;
        caveats.sort_by(|a, b| (a.0.as_str(), a.1.iri.as_str()).cmp(&(&b.0, &b.1.iri)));
        Self {
            correspondences,
            caveats,
            preservation,
        }
    }

    /// A deterministic, order-independent content key for the whole program — the
    /// content identity shared with the backing projection.
    ///
    /// The key includes the FULL correspondence payload in stable canonical order:
    /// IRI, relation, morphism class, morphism kind, and every numeric coefficient.
    /// Two correspondences that differ only in relation or morphism cannot collide.
    pub fn content_key(&self) -> String {
        let corr = self
            .correspondences
            .iter()
            .map(|c| {
                // Encode every field that distinguishes one correspondence from another.
                // Stable order: IRI | relation | morphism_class | morphism_kind |
                //               confidence | evidence_strength | weight | probability
                let confidence = c.confidence.map(decimal_lexical).unwrap_or_default();
                let evidence_strength =
                    c.evidence_strength.map(decimal_lexical).unwrap_or_default();
                let weight = c.weight.map(decimal_lexical).unwrap_or_default();
                let probability = c.probability.map(decimal_lexical).unwrap_or_default();
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}",
                    c.iri,
                    c.relation.as_str(),
                    c.morphism_class.as_str(),
                    c.morphism_kind.as_str(),
                    confidence,
                    evidence_strength,
                    weight,
                    probability,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let caveats = self
            .caveats
            .iter()
            .map(|(owner, c)| format!("{owner}=>{}={}", c.iri, c.text))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "PRESERVATION={}\nCORRESPONDENCES={corr}\nCAVEATS={caveats}",
            self.preservation.as_str(),
        )
    }
}

// --------------------------------------------------------------------------- //
// N-Triples rendering helpers (mirror the relational-core projection style)
// --------------------------------------------------------------------------- //

fn nt_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn triple_iri(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .")
}
fn triple_str(subject: &str, predicate: &str, lexical: &str) -> String {
    format!("<{subject}> <{predicate}> \"{}\" .", nt_escape(lexical))
}
fn triple_bool(subject: &str, predicate: &str, value: bool) -> String {
    format!("<{subject}> <{predicate}> \"{value}\"^^<{XSD_BOOLEAN}> .")
}

fn expand_scientific_decimal(raw: &str) -> String {
    let (mantissa, exponent) = raw
        .split_once('e')
        .or_else(|| raw.split_once('E'))
        .expect("scientific decimal contains an exponent separator");
    let exponent: isize = exponent
        .parse()
        .expect("f64 Display exponent is an integer");
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{int_part}{frac_part}");
    let decimal_pos = int_part.len() as isize + exponent;
    let mut body = if decimal_pos <= 0 {
        format!("0.{}{}", "0".repeat((-decimal_pos) as usize), digits)
    } else if decimal_pos as usize >= digits.len() {
        format!(
            "{}{}",
            digits,
            "0".repeat(decimal_pos as usize - digits.len())
        )
    } else {
        let idx = decimal_pos as usize;
        format!("{}.{}", &digits[..idx], &digits[idx..])
    };
    if body.contains('.') {
        while body.ends_with('0') {
            body.pop();
        }
        if body.ends_with('.') {
            body.pop();
        }
    }
    if body == "0" {
        "0".to_owned()
    } else {
        format!("{sign}{body}")
    }
}

fn decimal_lexical(value: f64) -> String {
    assert!(
        value.is_finite(),
        "logic decimal projection requires a finite f64, got {value}"
    );
    let value = if value == 0.0 { 0.0 } else { value };
    let raw = value.to_string();
    if raw.contains('e') || raw.contains('E') {
        expand_scientific_decimal(&raw)
    } else {
        raw
    }
}

fn triple_decimal(subject: &str, predicate: &str, value: f64) -> String {
    let v = decimal_lexical(value);
    format!("<{subject}> <{predicate}> \"{v}\"^^<{XSD_DECIMAL}> .")
}

/// A content-stable IRI for a law-claim node under a correspondence (so it survives the
/// round-trip distinctly and deterministically).
fn law_claim_iri(corr_iri: &str, index: usize) -> String {
    format!("{corr_iri}/law-claim/{index}")
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
    let prog = program_iri();
    let mut lines: Vec<String> = Vec::new();

    lines.push(triple_iri(&prog, RDF_TYPE, &class_program()));
    lines.push(triple_iri(
        &prog,
        &p_has_preservation(),
        &program.preservation.iri(),
    ));
    // The loss-ledger row for the lane: the structural drop a caveated overlap declares
    // (it refuses the forced-equality reading), so the polarity is never silent.
    if program.preservation != PreservationKind::Exact {
        lines.push(triple_str(
            &prog,
            &p_lossy_drop(),
            "a caveated overlap is projected as skos:relatedMatch, never skos:exactMatch / \
             owl:equivalentClass; the forced-equality reading is refused (under-approximation)",
        ));
    }

    for c in &program.correspondences {
        lines.push(triple_iri(&prog, &p_has_correspondence(), &c.iri));
        lines.push(triple_iri(&c.iri, RDF_TYPE, &class_correspondence()));
        lines.push(triple_iri(&c.iri, &p_relation(), &c.relation.iri()));
        lines.push(triple_iri(
            &c.iri,
            &p_morphism_class(),
            &c.morphism_class.iri(),
        ));
        lines.push(triple_iri(
            &c.iri,
            &p_morphism_kind(),
            &c.morphism_kind.iri(),
        ));
        if c.mnemomorphic {
            lines.push(triple_bool(&c.iri, &p_mnemomorphic(), true));
        }
        if let Some(det) = c.determinacy {
            lines.push(triple_iri(&c.iri, &p_determinacy(), &det.iri()));
        }
        if let Some(leg) = &c.get_leg {
            lines.push(triple_iri(&c.iri, &p_get_leg(), leg));
        }
        if let Some(leg) = &c.put_leg {
            lines.push(triple_iri(&c.iri, &p_put_leg(), leg));
        }
        if let Some(v) = c.confidence {
            lines.push(triple_decimal(&c.iri, &p_confidence(), v));
        }
        if let Some(v) = c.evidence_strength {
            lines.push(triple_decimal(&c.iri, &p_evidence_strength(), v));
        }
        if let Some(v) = c.weight {
            lines.push(triple_decimal(&c.iri, &p_weight(), v));
        }
        if let Some(v) = c.probability {
            lines.push(triple_decimal(&c.iri, &p_probability(), v));
        }
        if let Some(at) = &c.according_to {
            lines.push(triple_iri(&c.iri, &p_according_to(), at));
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
                lines.push(triple_iri(leg, pred, &c.iri));
            }
        }
        // Law claims, each as a content-IRI node.
        for (index, claim) in c.law_claims.iter().enumerate() {
            let claim_iri = law_claim_iri(&c.iri, index);
            lines.push(triple_iri(&c.iri, &p_has_law_claim(), &claim_iri));
            lines.push(triple_iri(&claim_iri, RDF_TYPE, &class_law_claim()));
            lines.push(triple_iri(&claim_iri, &p_law_claimed(), &claim.law.iri()));
            lines.push(triple_iri(
                &claim_iri,
                &p_law_verdict(),
                &claim.verdict.iri(),
            ));
            if let Some(cond) = claim.condition {
                lines.push(triple_iri(&claim_iri, &p_law_condition(), &cond.iri()));
            }
        }
        // Caveats for this correspondence.
        for (owner, caveat) in &program.caveats {
            if owner == &c.iri {
                lines.push(triple_iri(&c.iri, &p_has_caveat(), &caveat.iri));
                lines.push(triple_iri(&caveat.iri, RDF_TYPE, &class_caveat()));
                lines.push(triple_str(&caveat.iri, RDFS_COMMENT, &caveat.text));
            }
        }
    }

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

// --------------------------------------------------------------------------- //
// Reverse: graph → CorrespondenceProgram (the cache-hit re-derivation)
// --------------------------------------------------------------------------- //

/// Re-derive a [`CorrespondenceProgram`] from its backing `graph/correspondence`
/// N-Triples — the inverse of [`project_correspondence`], used by the cache on a hit.
///
/// HARD-fails on a malformed graph (no-optionality): a backing graph that no longer
/// re-derives is a corrupt cache, never a silently-dropped handle.
pub fn parse_correspondence(
    dataset: &gmeow_rdf::RdfDataset,
) -> Result<CorrespondenceProgram, String> {
    use gmeow_rdf::RdfTerm;

    // Index (subject, predicate) once so every reverse-lookup is cheap even on a
    // large graph/correspondence projection.
    let mut by_sp: BTreeMap<(String, String), Vec<RdfTerm>> = BTreeMap::new();
    for quad in dataset.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        by_sp
            .entry((subject.clone(), quad.predicate.clone()))
            .or_default()
            .push(quad.object.clone());
    }

    let iri_obj = |s: &str, p: &str| -> Option<String> {
        by_sp
            .get(&(s.to_owned(), p.to_owned()))?
            .iter()
            .find_map(|o| match o {
                RdfTerm::Iri(i) => Some(i.clone()),
                _ => None,
            })
    };
    let lit_obj = |s: &str, p: &str| -> Option<String> {
        by_sp
            .get(&(s.to_owned(), p.to_owned()))?
            .iter()
            .find_map(|o| match o {
                RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
                _ => None,
            })
    };
    let decimal_obj =
        |s: &str, p: &str| -> Option<f64> { lit_obj(s, p).and_then(|v| v.parse().ok()) };
    let iri_objs = |s: &str, p: &str| -> Vec<String> {
        let mut out: Vec<String> = by_sp
            .get(&(s.to_owned(), p.to_owned()))
            .into_iter()
            .flat_map(|objects| objects.iter())
            .filter_map(|o| match o {
                RdfTerm::Iri(i) => Some(i.clone()),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    };

    let local =
        |iri: &str| -> String { iri.strip_prefix(LOGIC_NAMESPACE).unwrap_or(iri).to_owned() };

    let prog = program_iri();
    let preservation = match iri_obj(&prog, &p_has_preservation()) {
        Some(iri) => preservation_from_iri(&iri)
            .ok_or_else(|| format!("unknown preservation kind <{iri}>"))?,
        None => return Err("graph/correspondence carries no hasPreservation".to_owned()),
    };

    // Each hasCorrespondence object is a Correspondence subject.
    let mut correspondences = Vec::new();
    let mut caveats: Vec<(String, CorrespondenceCaveat)> = Vec::new();
    for corr_iri in iri_objs(&prog, &p_has_correspondence()) {
        let relation = iri_obj(&corr_iri, &p_relation())
            .and_then(|i| CorrespondenceRelation::from_local(&local(&i)))
            .ok_or_else(|| format!("correspondence <{corr_iri}> has no/unknown relation"))?;
        let morphism_class = iri_obj(&corr_iri, &p_morphism_class())
            .and_then(|i| crate::ir::MorphismClass::from_local(&local(&i)))
            .ok_or_else(|| format!("correspondence <{corr_iri}> has no/unknown morphismClass"))?;
        let morphism_kind = iri_obj(&corr_iri, &p_morphism_kind())
            .and_then(|i| MorphismKind::from_local(&local(&i)))
            .ok_or_else(|| format!("correspondence <{corr_iri}> has no/unknown morphismKind"))?;
        let mnemomorphic = lit_obj(&corr_iri, &p_mnemomorphic())
            .map(|v| v == "true")
            .unwrap_or(false);
        let determinacy = iri_obj(&corr_iri, &p_determinacy())
            .and_then(|i| crate::ir::Determinacy::from_local(&local(&i)));
        let get_leg = iri_obj(&corr_iri, &p_get_leg());
        let put_leg = iri_obj(&corr_iri, &p_put_leg());
        let according_to = iri_obj(&corr_iri, &p_according_to());

        // Law claims (re-read by their per-correspondence node IRIs, sorted by node IRI
        // so re-derivation is order-stable; the Correspondence ctor re-canonicalizes).
        let mut claim_nodes = iri_objs(&corr_iri, &p_has_law_claim());
        claim_nodes.sort();
        let mut law_claims = Vec::new();
        for claim_iri in claim_nodes {
            let law = iri_obj(&claim_iri, &p_law_claimed())
                .and_then(|i| crate::ir::CorrespondenceLaw::from_local(&local(&i)))
                .ok_or_else(|| format!("law-claim <{claim_iri}> has no/unknown lawClaimed"))?;
            let verdict = iri_obj(&claim_iri, &p_law_verdict())
                .and_then(|i| crate::ir::DischargeVerdict::from_local(&local(&i)))
                .ok_or_else(|| format!("law-claim <{claim_iri}> has no/unknown verdict"))?;
            let condition = iri_obj(&claim_iri, &p_law_condition())
                .and_then(|i| crate::ir::DischargeCondition::from_local(&local(&i)));
            law_claims.push(crate::ir::LawClaimIr {
                law,
                verdict,
                condition,
            });
        }

        let correspondence = Correspondence::new(
            corr_iri.clone(),
            relation,
            morphism_class,
            morphism_kind,
            mnemomorphic,
            determinacy,
            get_leg,
            put_leg,
            law_claims,
            decimal_obj(&corr_iri, &p_confidence()),
            decimal_obj(&corr_iri, &p_evidence_strength()),
            decimal_obj(&corr_iri, &p_weight()),
            decimal_obj(&corr_iri, &p_probability()),
            according_to,
        )?;
        correspondences.push(correspondence);

        // Caveats: each hasCaveat object carries an rdfs:comment.
        // HARD-FAIL if the text is absent — a caveat without text is a corrupt graph,
        // never a silently-empty comment (no-optionality).
        for caveat_iri in iri_objs(&corr_iri, &p_has_caveat()) {
            let text = lit_obj(&caveat_iri, RDFS_COMMENT).ok_or_else(|| {
                format!(
                    "caveat <{caveat_iri}> on correspondence <{corr_iri}> has no rdfs:comment \
                     text; a corrupt graph must not silently produce an empty caveat"
                )
            })?;
            caveats.push((
                corr_iri.clone(),
                CorrespondenceCaveat {
                    iri: caveat_iri,
                    text,
                },
            ));
        }
    }

    Ok(CorrespondenceProgram::new(
        correspondences,
        caveats,
        preservation,
    ))
}

/// Inverse of [`PreservationKind::as_str`] for the kinds this lane uses.
fn preservation_from_iri(iri: &str) -> Option<PreservationKind> {
    let local = iri.strip_prefix(LOGIC_NAMESPACE).unwrap_or(iri);
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

// --------------------------------------------------------------------------- //
// The §14 affine-triangle worked example
// --------------------------------------------------------------------------- //

/// The §14 worked example (`docs/APPLIED_CATEGORY_THEORY/take1.md`): `foaf:Person`
/// (an agent) and `schema:ContactPoint` (a contact channel) **co-project onto the
/// contact-bearing facet of `gmeow:contact`** — not peers, not subsets, not equivalent.
/// The honest canonical object is a *vague affine overlap*, not a forced equality.
///
/// Builds the [`CorrespondenceProgram`] carrying exactly this one correspondence (with
/// its caveat) so the lane flows end-to-end through the bundle carrier. The generated
/// alignment surface MUST be `skos:relatedMatch` (NEVER `skos:exactMatch`, NEVER
/// `owl:equivalentClass`), and the lane declares its `SoundUnderApproximation`
/// preservation polarity in the loss ledger.
pub fn affine_triangle_worked_example() -> CorrespondenceProgram {
    use crate::ir::{
        CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeVerdict, LawClaimIr,
        MorphismClass, MorphismKind,
    };

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    let corr_iri = format!("{GMEOW}example/gmeowContactCorrespondence");
    // The two legs are affine optics onto the shared apex `gmeow:contact`.
    let get_leg = format!("{GMEOW}example/foafPersonToGmeowContactFacet");
    let put_leg = format!("{GMEOW}example/schemaContactPointToGmeowContactFacet");
    let caveat_iri = format!("{corr_iri}/caveat");

    let correspondence = Correspondence::new(
        corr_iri.clone(),
        CorrespondenceRelation::Overlaps,
        MorphismClass::AffineCorrespondence,
        MorphismKind::InstitutionMorphism,
        false,
        Some(Determinacy::Vague),
        Some(get_leg),
        Some(put_leg),
        // An affine co-projection claims GetPut (acquisition stability) but the law is
        // left unverified (honest unknown), never asserted discharged on a vague overlap.
        vec![LawClaimIr {
            law: CorrespondenceLaw::GetPut,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
        Some(0.72),
        None,
        None,
        None,
        None,
    )
    .expect("the §14 affine-triangle correspondence is well-formed");

    let caveat = CorrespondenceCaveat {
        iri: caveat_iri,
        text: "foaf:Person denotes an agent/person; schema:ContactPoint denotes a contact \
               channel/role. Both project through the contact-bearing facet of gmeow:contact; \
               they are not equivalent and neither subsumes the other."
            .to_owned(),
    };

    CorrespondenceProgram::new(
        vec![correspondence],
        vec![(corr_iri, caveat)],
        // A caveated overlap under-approximates the forced-equality reading it refuses.
        PreservationKind::SoundUnder,
    )
}

#[cfg(test)]
mod tests;
