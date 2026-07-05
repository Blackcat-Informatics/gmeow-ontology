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
//! **The epistemic strength of the `ValidationOnly` marking (settled design rationale).** The
//! CONSTRUCT head asserts the gmeow source atoms (`?anchor a gmeow:ModelArtifact`) *directly
//! into the base graph* and qualifies them only with the provenance envelope. The marking is
//! therefore *provenance-annotation strength*: `gmeow:mappedFrom` is an annotation property a
//! reasoner does not consult, so a reasoner sees the class-lift as an asserted base-graph fact
//! carrying out-of-band provenance — not as a claim scoped to its import. This is the honest
//! `ValidationOnly` FLOOR for an up-lift the source cannot itself express: it does not
//! overclaim *equivalence* (no `owl:equivalentClass`/`skos:exactMatch` is emitted; the
//! round-trip gate is correctly NotApplicable under the lossy lens and the overclaim gate stays
//! green), and the import-derivation is disclosed in-band on every minted anchor. Scoping the
//! lifted triples into a named-graph / RDF-star *reasoned* claim — so a reasoner treats them as
//! asserted-by-import rather than asserted-fact — is a strictly stronger, distinct capability
//! that composes this ingest lowering with the reason lane; it is out of scope for this
//! in-band-marking lowering by design, not an unfinished part of it.
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
    atom_triple, local_cell, prefix_block, suppression_anchors, templates_of, EmittedQuery,
    SuppressionVocab, GENERATED_BANNER,
};
use crate::projections::{correspondence_result, ProjectionResult};

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
    // The per-correspondence loss ledger of the inverse-ingest leg. A ValidationOnly
    // mint-with-claim binding carries the authored gmeow:ingestResidue — the honesty
    // disclosure of what the external source cannot express (durable subject, tenure,
    // distribution/versioning framing, attributed provenance) — into a `sparql-put` row
    // so it survives into the shipped bundle. CompleteOver recoveries mint no envelope and
    // Unsupported bindings contribute nothing, so neither adds a residue row.
    let mut ledger: Vec<ProjectionResult> = Vec::new();

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
                    // Provenance-annotation strength — the honest floor, stated precisely.
                    // These three triples land the source lift in the BASE graph and qualify it
                    // ONLY with out-of-band provenance: `gmeow:mappedFrom` is an annotation
                    // property, so a reasoner uses none of the envelope and reads the class-lift
                    // as an asserted base-graph fact. That is the honest legalization for an
                    // up-lift the source cannot itself express: no equivalence is claimed
                    // (`owl:equivalentClass`/`skos:exactMatch` are never emitted — round-trip
                    // NotApplicable under the lossy lens, overclaim gate green) and the
                    // import-derivation is disclosed IN-BAND on every minted anchor. Promoting
                    // the lift to a named-graph / RDF-star reasoned claim (reasoner sees
                    // asserted-by-import, not asserted-fact) is a strictly stronger, distinct
                    // capability that composes ingest with the reason lane — out of scope for
                    // this in-band-marking lowering by design, not an unfinished part of it.
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
                    // Carry the authored ingest-residue disclosure into the loss ledger —
                    // one row per ValidationOnly binding that has residue, mirroring the
                    // forward emitter's one-row-per-binding attribution. This is the honest
                    // record of what the external source cannot express and is minted here
                    // only with claim, never fabricated as extracted fact.
                    if !b.ingest_residue.is_empty() {
                        let key = format!("{}::{}", local_cell(&cell.iri), b.profile);
                        ledger.push(correspondence_result(
                            "sparql-put",
                            &key,
                            b.ingest_residue.clone(),
                        ));
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
        ledger,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        CorrespondenceLaw, DischargeVerdict, LawClaimIr, MorphismClass, PreservationKind,
    };
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
    fn validation_only_binding_carries_residue_into_the_ledger() {
        // A ValidationOnly mint-with-claim binding that has an authored ingest-residue
        // disclosure must surface exactly one loss-ledger row carrying that disclosure
        // verbatim — the honest record of what the external source cannot express.
        let residue = "a bare mls:Model lift carries no durable subject or tenure; \
                       never fabricated"
            .to_owned();
        let mut cell = class_cell("<=", false, Some(put_get_claim()));
        cell.bindings[0].ingest_residue = vec![residue.clone()];
        let emitted = emit(&[cell]).expect("ValidationOnly contributes a put query");

        assert_eq!(
            emitted.ledger.len(),
            1,
            "one ValidationOnly binding with residue must emit exactly one ledger row"
        );
        let row = &emitted.ledger[0];
        assert!(
            row.target.starts_with("sparql-put:"),
            "the residue row must be a sparql-put correspondence row, got {:?}",
            row.target
        );
        assert_eq!(
            row.preservation,
            PreservationKind::ValidationOnly,
            "the inverse-ingest up-lift is validation-only"
        );
        assert!(
            row.actual_drops.contains(&residue),
            "the authored disclosure must survive verbatim into actual_drops:\n{:?}",
            row.actual_drops
        );
        assert!(
            row.actual_drops
                .iter()
                .any(|d| d.starts_with("correspondence: ") && d.ends_with("::ml-schema")),
            "the row must carry its per-correspondence key note:\n{:?}",
            row.actual_drops
        );
    }

    #[test]
    fn complete_over_binding_emits_no_residue_row() {
        // A CompleteOver recovery mints no provenance envelope and carries no residue —
        // even a stray residue on the binding must not produce a ledger row.
        let mut cell = class_cell("=", true, None);
        cell.bindings[0].ingest_residue = vec!["should be ignored".to_owned()];
        let emitted = emit(&[cell]).expect("CompleteOver contributes a put query");
        assert!(
            emitted.ledger.is_empty(),
            "a CompleteOver recovery must not emit a residue row:\n{:?}",
            emitted.ledger
        );
    }

    #[test]
    fn unsupported_binding_emits_no_residue_row() {
        // An Unsupported binding contributes nothing — even with a stray residue set, the
        // profile yields no query at all, so there is no ledger to carry residue.
        let mut cell = class_cell("<=", false, None);
        cell.bindings[0].ingest_residue = vec!["should be ignored".to_owned()];
        assert!(
            emit(&[cell]).is_none(),
            "an Unsupported-only profile must yield Ok(None), carrying no residue"
        );
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
