// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **inverse-ingest ("put") SPARQL-CONSTRUCT lowering**: the role-swap of the forward
//! [`crate::projections::sparql`] down-projection.
//!
//! The forward `get` leg is `CONSTRUCT { external target atoms } WHERE { gmeow source
//! pattern + display-guards }`. The inverse `put` leg swaps the two roles:
//!
//! ```text
//! CONSTRUCT { gmeow source atoms [+ mint-with-claim envelope] }
//! WHERE     { external template atoms }
//! ```
//!
//! - the put **WHERE** matches the forward's CONSTRUCT-head template atoms (the external
//!   triples, e.g. `?mlsArtifact a mls:Model`) — with NO display-guards (the guards belong
//!   to the forward down-projection's suppression contract, not to ingest);
//! - the put **CONSTRUCT** rebuilds the forward's gmeow source atoms (e.g. `?mlsArtifact a
//!   gmeow:ModelArtifact`). For a `ValidationOnly` up-lift it additionally mints a
//!   provenance envelope (`gmeow:wasGeneratedBy` / `gmeow:mappedFrom` / a
//!   `gmeow:ImportActivity`) so the lifted claim is honestly marked import-derived, not
//!   extracted fact.
//!
//! The up-lift polarity is decided by the SINGLE authority
//! [`crate::projections::put_derivation::classify_put`]: a mnemomorphic witness on an
//! injective-enough rung is a `CompleteOver` recovery; a co-authored ingest claim without a
//! witness is a `ValidationOnly` mint-with-claim; neither contributes nothing (the honest
//! legalization floor). Both legs render from the SAME get-leg model the forward emitter
//! reads ([`crate::projections::get_leg`]), so they cannot drift.

use std::collections::{BTreeMap, BTreeSet};

use crate::projections::correspondence_frontend::CorrespondenceLookup;
use crate::projections::get_leg::{curie, ProfileBinding, ProjectionCell};
use crate::projections::put_derivation::{classify_put, PutClass};
use crate::projections::sparql::{
    atom_triple, prefix_block, suppression_anchors, templates_of, EmittedQuery, SuppressionVocab,
    GENERATED_BANNER,
};

/// Render the gmeow SOURCE atoms of a cell as flat CONSTRUCT-head triple strings — the
/// pattern the forward WHERE reads, minus every guard/OPTIONAL/FILTER/BIND/VALUES. The
/// atoms are flattened (OPTIONAL groups recursed) and each rendered as a bare triple via
/// the SAME [`atom_triple`] the forward branch uses (an empty var map — no language-retag on
/// the source side), so a class binding yields `?anchor a gmeow:<SourceClass>` and a
/// predicate binding `?anchor gmeow:<sourcePred> ?value`.
fn source_atoms(cell: &ProjectionCell, _b: &ProfileBinding) -> Result<Vec<String>, String> {
    let empty: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::new();
    for atom in cell.pattern.flat_atoms() {
        out.push(atom_triple(&atom, &empty)?);
    }
    Ok(out)
}

/// Emit the inverse-ingest ("put") CONSTRUCT query for one profile, or `Ok(None)` when no
/// binding for the profile is put-liftable (every binding classifies `Unsupported`).
///
/// `vocab` and `lookup` are held for signature symmetry with the forward
/// [`crate::projections::sparql`] emitter: the put leg injects NO display-guards (so it does
/// not read the suppression vocabulary) and the overclaim gate is a forward-projection
/// concern, so neither is consulted here — but keeping the parallel signature lets the two
/// legs be driven from the one `lower_sparql` loop.
pub(crate) fn emit_put(
    cells: &[ProjectionCell],
    profile: &str,
    vocab: &SuppressionVocab,
    lookup: &CorrespondenceLookup,
) -> Result<Option<EmittedQuery>, String> {
    let _ = (vocab, lookup);
    let empty: BTreeMap<String, String> = BTreeMap::new();
    let mut construct: Vec<String> = Vec::new();
    let mut branches: Vec<String> = Vec::new();
    let mut seen_branches: BTreeSet<String> = BTreeSet::new();
    let mut any_validation_only = false;
    let mut contributed = false;

    for cell in cells {
        for b in &cell.bindings {
            if b.profile != profile {
                continue;
            }
            // The SINGLE authority for the up-lift polarity: mnemomorphic witness on an
            // injective-enough rung → CompleteOver; a co-authored ingest claim without a
            // witness → ValidationOnly; neither → Unsupported (contributes nothing).
            let class = classify_put(b.mnemomorphic, b.lattice().1, b.ingest_claim.as_slice());
            match class {
                // Neither witness nor claim: the honest floor — carry nothing to the put leg.
                PutClass::Unsupported => continue,
                // A lawful recovery: CONSTRUCT the gmeow source, no provenance envelope.
                PutClass::CompleteOver => {
                    for atom in source_atoms(cell, b)? {
                        if !construct.contains(&atom) {
                            construct.push(atom);
                        }
                    }
                }
                // A minted-with-claim candidate preimage: CONSTRUCT the gmeow source PLUS a
                // provenance envelope per lifted anchor, marking the claim import-derived (a
                // deterministic `_:imp` blank node — no NOW()/ingestedAt) rather than an
                // extracted fact.
                PutClass::ValidationOnly => {
                    any_validation_only = true;
                    for atom in source_atoms(cell, b)? {
                        if !construct.contains(&atom) {
                            construct.push(atom);
                        }
                    }
                    // The forward target the claim is mapped from: the binding's toClass else
                    // its toPredicate, as the CURIE. A ValidationOnly binding with neither has
                    // no nameable provenance source — a HARD FAIL (no silent skip).
                    let target =
                        b.to_class
                            .as_ref()
                            .or(b.to_predicate.as_ref())
                            .ok_or_else(|| {
                                format!(
                                "put emitter: ValidationOnly binding for profile {profile:?} on \
                                 <{}> has neither gmeow:toClass nor gmeow:toPredicate, so its \
                                 mint-with-claim provenance has no nameable forward target",
                                cell.iri
                            )
                            })?;
                    let target_curie = curie(target);
                    for anchor in suppression_anchors(&cell.pattern) {
                        for env in [
                            format!("?{anchor} gmeow:wasGeneratedBy _:imp ."),
                            format!("?{anchor} gmeow:mappedFrom {target_curie} ."),
                            "_:imp a gmeow:ImportActivity .".to_owned(),
                        ] {
                            if !construct.contains(&env) {
                                construct.push(env);
                            }
                        }
                    }
                }
            }
            contributed = true;
            // The WHERE branch: the forward's external template atoms (plain external vars,
            // EMPTY retag map) — the triples this put leg matches to lift the source. Wrapped
            // and deduped exactly as the forward emitter wraps its source branches.
            let where_atoms = templates_of(cell, b, &empty)?;
            let body = where_atoms
                .iter()
                .map(|ln| format!("        {ln}"))
                .collect::<Vec<_>>()
                .join("\n");
            let branch = format!("{{\n{body}\n    }}");
            if seen_branches.insert(branch.clone()) {
                branches.push(branch);
            }
        }
    }

    if !contributed {
        return Ok(None);
    }

    let construct_block = construct
        .iter()
        .map(|t| format!("    {t}"))
        .collect::<Vec<_>>()
        .join("\n");
    let where_clause = branches
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if i == 0 {
                b.clone()
            } else {
                format!("UNION {b}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    let body = format!("CONSTRUCT {{\n{construct_block}\n}}\nWHERE {{\n    {where_clause}\n}}\n");
    // A mixed profile (some CompleteOver, some ValidationOnly bindings) already carries the
    // mint envelopes for its ValidationOnly parts, so the honest, weaker header dominates:
    // any ValidationOnly present ⇒ the mint-with-claim header (never claim pure identity when
    // part of the query is import-derived).
    let header = if any_validation_only {
        format!(
            "# Inverse ingest: {profile} → GMEOW. Mint-with-claim, validation-only — \
             import-derived claim, not extracted fact; subject/tenure not synthesized \
             (residue). {GENERATED_BANNER}\n"
        )
    } else {
        format!(
            "# Inverse ingest: pure {profile} → GMEOW. CompleteOver up-lift — identity on \
             the displayable image of get. {GENERATED_BANNER}\n"
        )
    };
    let prefixes = prefix_block(&body);
    Ok(Some(EmittedQuery {
        query: format!("{header}{prefixes}\n\n{body}"),
        ledger: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CorrespondenceLaw, DischargeVerdict, LawClaimIr, MorphismClass};
    use crate::projections::correspondence_frontend::CorrespondenceLookup;
    use crate::projections::get_leg::{Atom, Item, MappingPattern, ProfileBinding, ProjectionCell};
    use crate::projections::sparql::SuppressionVocab;

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const MLS_MODEL: &str = "http://www.w3.org/ns/mls#Model";
    const GM_MODEL_ARTIFACT: &str = "https://blackcatinformatics.ca/gmeow/ModelArtifact";

    /// A bare `?x a <class>` source atom (non-optional, so it seeds the anchor).
    fn type_atom(subject: &str, class_iri: &str) -> Atom {
        Atom {
            subject_var: subject.to_owned(),
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

    /// A one-atom class cell: the gmeow source `?x a gmeow:ModelArtifact` mapping to the
    /// external `?x a mls:Model` (via `to_class`), authored with the given up-lift knobs.
    fn class_cell(
        relation: &str,
        mnemomorphic: bool,
        ingest_claim: Option<LawClaimIr>,
    ) -> ProjectionCell {
        let pattern = MappingPattern {
            anchor: "x".to_owned(),
            value: None,
            atoms: vec![Item::Atom(type_atom("x", GM_MODEL_ARTIFACT))],
            suppress_when: Vec::new(),
            project_when: Vec::new(),
            exclude_when: Vec::new(),
            filters: Vec::new(),
            binds: Vec::new(),
            mints: Vec::new(),
            edoal_source: None,
            edoal_source_kind: "relation".to_owned(),
            edoal_path: false,
        };
        let binding = ProfileBinding {
            profile: "ml-schema".to_owned(),
            to_predicate: None,
            to_class: Some(MLS_MODEL.to_owned()),
            template_atoms: Vec::new(),
            value_class_map: Vec::new(),
            relation: relation.to_owned(),
            transform: None,
            confidence: None,
            lossy_drops: Vec::new(),
            edoal_target: None,
            edoal_target_kind: None,
            morphism_class: None,
            ingest_claim,
            ingest_residue: Vec::new(),
            mnemomorphic,
        };
        ProjectionCell {
            iri: "https://blackcatinformatics.ca/gmeow/example/mlModelCell".to_owned(),
            label: String::new(),
            pattern,
            bindings: vec![binding],
        }
    }

    fn put_get_claim() -> LawClaimIr {
        LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }
    }

    fn emit(cells: &[ProjectionCell]) -> Option<EmittedQuery> {
        let vocab = SuppressionVocab::empty();
        let lookup = CorrespondenceLookup::default();
        emit_put(cells, "ml-schema", &vocab, &lookup).expect("emit_put must not hard-fail")
    }

    #[test]
    fn validation_only_binding_mints_with_claim_envelope() {
        // Lossy rung (`<=` → LossyLens) + a co-authored ingest claim → ValidationOnly:
        // the source lift PLUS the honest import-derived provenance envelope.
        let cells = [class_cell("<=", false, Some(put_get_claim()))];
        let q = emit(&cells)
            .expect("ValidationOnly contributes a put query")
            .query;

        // The gmeow source lift is reconstructed in the CONSTRUCT head.
        assert!(
            q.contains("?x a gmeow:ModelArtifact ."),
            "missing source lift:\n{q}"
        );
        // The mint-with-claim envelope is present.
        assert!(
            q.contains("gmeow:wasGeneratedBy"),
            "missing wasGeneratedBy:\n{q}"
        );
        assert!(
            q.contains("?x gmeow:mappedFrom mls:Model ."),
            "missing mappedFrom to the external target:\n{q}"
        );
        assert!(
            q.contains("_:imp a gmeow:ImportActivity ."),
            "missing ImportActivity node:\n{q}"
        );
        // It never overclaims equivalence and never stamps a wall-clock time.
        assert!(
            !q.contains("owl:equivalentClass"),
            "must not assert equivalence:\n{q}"
        );
        assert!(
            !q.contains("skos:exactMatch"),
            "must not assert exactMatch:\n{q}"
        );
        assert!(
            !q.contains("NOW("),
            "must not stamp a wall-clock time:\n{q}"
        );
        // The WHERE matches the external target term.
        let where_clause = q.split("WHERE").nth(1).expect("query has a WHERE");
        assert!(
            where_clause.contains("?x a mls:Model ."),
            "WHERE must match the external target:\n{q}"
        );
    }

    #[test]
    fn complete_over_binding_is_the_pure_inverse_no_envelope() {
        // Injective rung (`=` → WellBehavedLens) + a mnemomorphic witness → CompleteOver:
        // the pure inverse, source lift as CONSTRUCT and template as WHERE, no envelope.
        let cells = [class_cell("=", true, None)];
        let q = emit(&cells)
            .expect("CompleteOver contributes a put query")
            .query;

        assert!(
            q.contains("?x a gmeow:ModelArtifact ."),
            "missing source lift:\n{q}"
        );
        let where_clause = q.split("WHERE").nth(1).expect("query has a WHERE");
        assert!(
            where_clause.contains("?x a mls:Model ."),
            "WHERE must match the external template:\n{q}"
        );
        // No provenance envelope for a lawful recovery.
        assert!(
            !q.contains("gmeow:wasGeneratedBy"),
            "a CompleteOver recovery must not mint a provenance envelope:\n{q}"
        );
        assert!(
            !q.contains("gmeow:ImportActivity"),
            "a CompleteOver recovery must not mint an ImportActivity:\n{q}"
        );
        // The header must NOT claim a total `put ∘ get = id_S`; it claims identity only on
        // the displayable image of get.
        assert!(
            q.contains("identity on the displayable image"),
            "header must scope identity to the displayable image:\n{q}"
        );
        assert!(
            !q.contains("= id_S"),
            "header must not assert a total put ∘ get = id_S:\n{q}"
        );
    }

    #[test]
    fn unsupported_binding_contributes_nothing() {
        // Lossy rung, no witness, no claim → Unsupported: the honest floor emits nothing.
        let cells = [class_cell("<=", false, None)];
        assert!(
            emit(&cells).is_none(),
            "an Unsupported-only profile must yield Ok(None)"
        );
    }

    #[test]
    fn emission_is_deterministic_and_clock_free() {
        let cells = [class_cell("<=", false, Some(put_get_claim()))];
        let a = emit(&cells).expect("emits").query;
        let b = emit(&cells).expect("emits").query;
        assert_eq!(a, b, "the same cells must lower to identical bytes");
        assert!(!a.contains("NOW("), "the put leg must be clock-free:\n{a}");
    }

    #[test]
    fn classify_put_is_the_single_authority_for_the_three_polarities() {
        // ValidationOnly: no witness on a lossy rung, but a co-authored claim.
        assert_eq!(
            classify_put(false, MorphismClass::LossyLens, &[put_get_claim()]),
            PutClass::ValidationOnly
        );
        // CompleteOver: a mnemomorphic witness on an injective rung.
        assert_eq!(
            classify_put(true, MorphismClass::WellBehavedLens, &[]),
            PutClass::CompleteOver
        );
        // Unsupported: neither witness nor claim.
        assert_eq!(
            classify_put(false, MorphismClass::LossyLens, &[]),
            PutClass::Unsupported
        );
    }
}
