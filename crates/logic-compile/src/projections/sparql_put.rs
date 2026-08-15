// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **inverse-ingest ("put") SPARQL-CONSTRUCT lowering**: the role-swap of the forward
//! [`crate::projections::sparql`] down-projection.
//!
//! The forward `get` leg is `CONSTRUCT { external target atoms } WHERE { gmeow source
//! pattern + display-guards }`. The inverse `put` leg swaps the two roles:
//!
//! ```text
//! CONSTRUCT { gmeow source atoms  (CompleteOver: asserted fact)
//!           | reified claim cells (ValidationOnly: reasoner-inert) + one import activity }
//! WHERE     { external template atoms }
//! ```
//!
//! - the put **WHERE** matches the forward's CONSTRUCT-head template atoms (the external
//!   triples, e.g. `?mlsArtifact a mls:Model`) — with NO display-guards (the guards belong
//!   to the forward down-projection's suppression contract, not to ingest);
//! - the put **CONSTRUCT** renders the up-lift at the polarity the single
//!   [`crate::projections::reified_claim::AssertionPolarity`] authority assigns.
//!
//! **The two epistemic treatments, decided by polarity — not by hand.** A `CompleteOver`
//! recovery ([`AssertionPolarity::AssertBase`]) rebuilds the gmeow source atoms (`?anchor a
//! gmeow:ModelArtifact`) *directly into the base graph*: lossless recovery under a discharged
//! `put ∘ get = id` genuinely IS fact. A `ValidationOnly` lift
//! ([`AssertionPolarity::ReifyClaim`]) — an up-lift the source cannot itself express — is NOT
//! asserted: each source atom is carried as a `gmeow:StatementMetadata` reified RDF-1.2 claim
//! (`qSubject`/`qPredicate`/`qObject` triad, `gmeow:mappedFrom` on its annotation list), built
//! by the shared [`crate::projections::reified_claim::reified_claim_head`]. Because no EL/DL/RL
//! rule keys on the reification vocabulary and the reason lane drops every non-IRI-object quad,
//! the reified cell materializes NOTHING — the object-level relation is only *named* as the IRI
//! value of `qPredicate`, never asserted as a triple predicate. The interior triple is
//! deliberately absent from the CONSTRUCT head, so a reasoner treats the lift as a candidate
//! preimage scoped to its import, never as extracted fact. Every claim is
//! `gmeow:wasGeneratedBy` the ONE deterministic per-profile `gmeow:ImportActivity` (a stable
//! IRI that coalesces across solutions — never a per-solution blank, never `gmeow:ingestedAt`
//! /NOW()), typed and tied to its `gmeow:SoftwareAgent` importer.
//!
//! The up-lift polarity is decided by the SINGLE authority
//! [`crate::projections::put_derivation::classify_put`]: a mnemomorphic witness on an
//! injective-enough rung is a `CompleteOver` recovery; a co-authored ingest claim without a
//! witness is a `ValidationOnly` mint-with-claim; neither contributes nothing (the honest
//! legalization floor). Both legs render from the SAME get-leg model the forward emitter
//! reads ([`crate::projections::get_leg`]), so they cannot drift.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::Diag;

use crate::projections::correspondence_frontend::CorrespondenceLookup;
use crate::projections::get_leg::{Atom, ProfileBinding, ProjectionCell, curie, sparql_string};
use crate::projections::put_derivation::classify_put;
use crate::projections::reified_claim::{
    AssertionPolarity, ClaimAnnotation, ClaimObject, GM_MAPPED_FROM, IriStyle, ReifiedClaim,
    reified_claim_head,
};
use crate::projections::sparql::{
    EmittedQuery, GENERATED_BANNER, SuppressionVocab, atom_triple, local_cell, prefix_block,
    templates_of,
};
use crate::projections::{ProjectionResult, correspondence_result};

/// Render the gmeow SOURCE atoms of a cell as flat CONSTRUCT-head triple strings — the
/// pattern the forward WHERE reads, minus every guard/OPTIONAL/FILTER/BIND/VALUES. The
/// atoms are flattened (OPTIONAL groups recursed) and each rendered as a bare triple via
/// the SAME [`atom_triple`] the forward branch uses (an empty var map — no language-retag on
/// the source side), so a class binding yields `?anchor a gmeow:<SourceClass>` and a
/// predicate binding `?anchor gmeow:<sourcePred> ?value`.
fn source_atoms(cell: &ProjectionCell, _b: &ProfileBinding) -> gmeow_errors::Result<Vec<String>> {
    let empty: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::new();
    for atom in cell.pattern.flat_atoms() {
        out.push(atom_triple(&atom, &empty)?);
    }
    Ok(out)
}

/// The fixed `gmeow:SoftwareAgent` that performs inverse-ingest put projection — the importer the
/// coalesced [`import_activity_triples`] node is `gmeow:wasAssociatedWith`.
const IMPORTER_AGENT_IRI: &str =
    "<https://blackcatinformatics.ca/gmeow/agent/put-projection-importer>";

/// The one deterministic per-profile import-activity IRI. Legible and IRI-legal (profile names
/// are unique, so no content-address is needed); rendered as a full IRI because it carries a
/// path segment no CURIE local name may hold. Every solution of the put CONSTRUCT binds this SAME
/// IRI, so the activity coalesces onto exactly one node instead of a per-solution blank.
fn import_activity_iri(profile: &str) -> String {
    format!("<https://blackcatinformatics.ca/gmeow/import/{profile}>")
}

/// The provenance triples for the one coalesced import activity: typed `gmeow:ImportActivity`, a
/// static deterministic label, and tied to its `gmeow:SoftwareAgent` importer via
/// `gmeow:wasAssociatedWith` (maximal grounding). Emitted ONCE into the CONSTRUCT head — no
/// `gmeow:ingestedAt`/NOW(), so the emitter stays clock-free and deterministic.
fn import_activity_triples(profile: &str) -> Vec<String> {
    let import = import_activity_iri(profile);
    vec![
        format!("{import} a gmeow:ImportActivity ."),
        format!("{import} rdfs:label \"inverse-ingest of {profile} into GMEOW\" ."),
        format!("{import} gmeow:wasAssociatedWith {IMPORTER_AGENT_IRI} ."),
        format!("{IMPORTER_AGENT_IRI} a gmeow:SoftwareAgent ."),
    ]
}

/// Build the reified `gmeow:StatementMetadata` claim for one gmeow source atom of a
/// `ValidationOnly` cell: the reified subject/predicate/object triad + a `gmeow:mappedFrom`
/// annotation naming the forward target + `gmeow:wasGeneratedBy` the coalesced import activity.
/// `idx` makes the cell/annotation blank labels unique across the whole CONSTRUCT.
///
/// # Errors
///
/// Hard-fails (no-optionality) on an atom that cannot be honestly reified as a single
/// `qPredicate` IRI — a property path, an alternation, or a predicate variable has no single
/// predicate IRI to name, and an atom with no object cannot be reified — so none is ever
/// silently coerced.
fn reified_claim_of_atom(
    atom: &Atom,
    target_curie: &str,
    import_iri: &str,
    idx: usize,
) -> gmeow_errors::Result<ReifiedClaim> {
    if atom.path.is_some() || !atom.path_alts.is_empty() || atom.predicate_var.is_some() {
        return Err(Diag::of_kind(crate::error::Put {
            detail: format!(
                "put emitter: ValidationOnly source atom on ?{} is a property path / alternation / \
                 predicate variable, which has no single qPredicate IRI to reify honestly",
                atom.subject_var
            ),
        }));
    }
    let predicate = atom.predicate.clone().ok_or_else(|| {
        Diag::of_kind(crate::error::Put {
            detail: format!(
                "put emitter: ValidationOnly source atom on ?{} has no predicate IRI to reify",
                atom.subject_var
            ),
        })
    })?;
    let object = if let Some(value) = &atom.object_value {
        ClaimObject::Iri(curie(value))
    } else if let Some((lex, lang)) = &atom.object_literal {
        ClaimObject::Literal(match lang.as_deref() {
            Some(tag) => format!("{}@{tag}", sparql_string(lex)),
            None => sparql_string(lex),
        })
    } else if let Some(var) = &atom.object_var {
        ClaimObject::Iri(format!("?{var}"))
    } else {
        return Err(Diag::of_kind(crate::error::Put {
            detail: format!(
                "put emitter: ValidationOnly source atom on ?{} has no object to reify",
                atom.subject_var
            ),
        }));
    };
    Ok(ReifiedClaim {
        cell_label: format!("cell{idx}"),
        subject: format!("?{}", atom.subject_var),
        predicate,
        object,
        annotations: vec![ClaimAnnotation {
            label: format!("mapann{idx}"),
            property: GM_MAPPED_FROM.to_owned(),
            value: target_curie.to_owned(),
        }],
        generated_by: Some(import_iri.to_owned()),
    })
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
    loss: &mut crate::loss_ledger::LossLedger,
) -> gmeow_errors::Result<Option<EmittedQuery>> {
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
    // A monotone per-query counter that gives every reified claim (and its annotation nodes) a
    // blank label unique across the whole CONSTRUCT, so distinct claims never share a `_:cell`.
    let mut claim_idx = 0usize;

    for cell in cells {
        for b in &cell.bindings {
            if b.profile != profile {
                continue;
            }
            // The SINGLE authority for the up-lift polarity: mnemomorphic witness on an
            // injective-enough rung → CompleteOver; a co-authored ingest claim without a
            // witness → ValidationOnly; neither → Unsupported (contributes nothing).
            let class = classify_put(b.mnemomorphic, b.lattice().1, b.ingest_claim.as_slice());
            // The SECOND leg of the classify_put morphism: how the up-lift lands in RDF. The
            // single-authority AssertionPolarity map turns the three put-classes into the three
            // renderings — assert-as-fact, reify-as-claim, or withhold — so C1/C2/C3 are one
            // mechanism, not three hand-branches.
            match AssertionPolarity::of(class) {
                // Neither witness nor claim: the honest floor — carry nothing to the put leg.
                AssertionPolarity::Withhold => continue,
                // A lawful recovery: CONSTRUCT the gmeow source, no provenance envelope.
                AssertionPolarity::AssertBase => {
                    for atom in source_atoms(cell, b)? {
                        if !construct.contains(&atom) {
                            construct.push(atom);
                        }
                    }
                }
                // A minted-with-claim candidate preimage. The source cannot itself express the
                // lifted atom, so it is carried as a reasoner-INERT `gmeow:StatementMetadata`
                // reified claim (built by the shared `reified_claim_head`), never asserted as an
                // extracted base-graph fact. Every claim carries `gmeow:mappedFrom` on its
                // annotation list and `gmeow:wasGeneratedBy` the one coalesced per-profile import
                // activity — no interior triple, no NOW()/ingestedAt.
                AssertionPolarity::ReifyClaim => {
                    any_validation_only = true;
                    // The forward target the claim is mapped from: the binding's toClass else
                    // its toPredicate, as the CURIE. A ValidationOnly binding with neither has
                    // no nameable provenance source — a HARD FAIL (no silent skip).
                    let target = b.to_class.as_ref().or(b.to_predicate.as_ref()).ok_or_else(
                        || {
                            Diag::of_kind(crate::error::Put {
                                detail: format!(
                                    "put emitter: ValidationOnly binding for profile {profile:?} on \
                                     <{}> has neither gmeow:toClass nor gmeow:toPredicate, so its \
                                     mint-with-claim provenance has no nameable forward target",
                                    cell.iri
                                ),
                            })
                        },
                    )?;
                    let target_curie = curie(target);
                    let import_iri = import_activity_iri(profile);
                    // Reify each gmeow SOURCE atom of the cell as a StatementMetadata claim,
                    // generated by the single coalesced import activity. The interior triple
                    // (`?x a gmeow:<Class>`) is DELIBERATELY absent from the CONSTRUCT head — the
                    // reasoner materializes nothing from a reified cell (no EL/DL/RL rule keys on
                    // the reification vocabulary), so the lift is carried at full fidelity yet is
                    // never treated as fact. Promoting a genuinely-recoverable lift to an asserted
                    // fact is the `AssertBase`/CompleteOver arm above, decided by the single
                    // `classify_put` authority, never by hand here.
                    for atom in cell.pattern.flat_atoms() {
                        let claim =
                            reified_claim_of_atom(&atom, &target_curie, &import_iri, claim_idx)?;
                        for line in reified_claim_head(&claim, IriStyle::Curie) {
                            if !construct.contains(&line) {
                                construct.push(line);
                            }
                        }
                        claim_idx += 1;
                    }
                    // The ONE deterministic per-profile import activity, emitted once into the
                    // CONSTRUCT head (identical IRI across every solution → exactly one node, no
                    // per-solution blank, no NOW()). Typed, labelled, and tied to its software
                    // importer agent — maximally grounded provenance.
                    for t in import_activity_triples(profile) {
                        if !construct.contains(&t) {
                            construct.push(t);
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
                            loss,
                            "sparql-put",
                            &key,
                            b.ingest_residue.clone(),
                            None,
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
    // Balanced UNION tree, not a flat left-associative chain — see
    // [`super::sparql::balanced_union`] for why (the evaluator's graph-pattern nesting bound).
    let where_clause = super::sparql::balanced_union(&branches);
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
    use crate::projections::put_derivation::PutClass;
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
            edoal_source_kind: None,
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
            emit_sssom: false,
            sssom_predicate: None,
            sssom_file: None,
        };
        ProjectionCell {
            iri: "https://blackcatinformatics.ca/gmeow/example/mlModelCell".to_owned(),
            label: String::new(),
            pattern,
            bindings: vec![binding],
            grounding: None,
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
        // The residue rows are interned into the shared loss store; a caller that only
        // inspects the query text uses a throwaway store. Tests that assert on the interned
        // residue use `emit_with_loss` and read it back through `projection_drops_for`.
        emit_with_loss(cells).0
    }

    fn emit_with_loss(
        cells: &[ProjectionCell],
    ) -> (Option<EmittedQuery>, crate::loss_ledger::LossLedger) {
        let vocab = SuppressionVocab::empty();
        let lookup = CorrespondenceLookup::default();
        let mut loss = crate::loss_ledger::LossLedger::new();
        let emitted = emit_put(cells, "ml-schema", &vocab, &lookup, &mut loss)
            .expect("emit_put must not hard-fail");
        (emitted, loss)
    }

    /// The CONSTRUCT-head text of a query (between the first `CONSTRUCT {` and its closing `}`).
    fn construct_head(q: &str) -> &str {
        let after = q
            .split("CONSTRUCT {")
            .nth(1)
            .expect("query has a CONSTRUCT");
        after.split('}').next().expect("CONSTRUCT head is closed")
    }

    /// A `?x <predicate> ?obj` source atom.
    fn predicate_atom(subject: &str, predicate_iri: &str, object: &str) -> Atom {
        Atom {
            subject_var: subject.to_owned(),
            predicate: Some(predicate_iri.to_owned()),
            predicate_var: None,
            path: None,
            path_alts: Vec::new(),
            object_var: Some(object.to_owned()),
            object_value: None,
            object_literal: None,
            optional: false,
        }
    }

    #[test]
    fn complete_over_multi_atom_emits_every_source_atom_bare_and_no_envelope() {
        // R2 — the emitter is what ships, not the round-trip gate (which compares LegPath
        // bodies, not emitted bytes). For a multi-atom CompleteOver cell the CONSTRUCT head must
        // be EXACTLY the flat source atoms — every one recovered, none reified, no ImportActivity
        // envelope — so an author can read the emitted `.put.rq` and see precisely what a lift
        // reconstructs. (A source pattern that hides an unrecoverable guard atom would therefore
        // surface here as a spurious CONSTRUCT-head triple, which is how the SIOC audit caught
        // the non-mnemomorphic mapSiocTopic.)
        let gm_pred = "https://blackcatinformatics.ca/gmeow/relatedThread";
        let pattern = MappingPattern {
            anchor: "x".to_owned(),
            value: Some("y".to_owned()),
            atoms: vec![
                Item::Atom(type_atom("x", GM_MODEL_ARTIFACT)),
                Item::Atom(predicate_atom("x", gm_pred, "y")),
            ],
            suppress_when: Vec::new(),
            project_when: Vec::new(),
            exclude_when: Vec::new(),
            filters: Vec::new(),
            binds: Vec::new(),
            mints: Vec::new(),
            edoal_source: None,
            edoal_source_kind: None,
            edoal_path: false,
        };
        let mut cell = class_cell("=", true, None);
        cell.pattern = pattern;
        let q = emit(&[cell]).expect("CompleteOver emits").query;
        let head = construct_head(&q);

        // Every source atom appears verbatim in the CONSTRUCT head.
        assert!(
            head.contains("?x a gmeow:ModelArtifact ."),
            "missing type atom:\n{head}"
        );
        assert!(
            head.contains("?x gmeow:relatedThread ?y ."),
            "missing predicate atom:\n{head}"
        );
        // No reification / provenance envelope on a lawful recovery.
        assert!(
            !head.contains("gmeow:StatementMetadata"),
            "no reified claim on a recovery:\n{head}"
        );
        assert!(
            !head.contains("gmeow:ImportActivity"),
            "no import activity on a recovery:\n{head}"
        );
        assert!(
            !head.contains("gmeow:wasGeneratedBy"),
            "no provenance edge on a recovery:\n{head}"
        );
        // The head is EXACTLY the two source atoms — nothing spurious.
        let triples: Vec<&str> = head
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(
            triples.len(),
            2,
            "exactly the two source atoms, no extra:\n{head}"
        );
    }

    #[test]
    fn validation_only_binding_reifies_the_lift_as_an_inert_claim() {
        // Lossy rung (`<=` → LossyLens) + a co-authored ingest claim → ValidationOnly:
        // the source lift is carried as a reasoner-INERT gmeow:StatementMetadata reified
        // claim, NOT asserted as a base-graph fact.
        let cells = [class_cell("<=", false, Some(put_get_claim()))];
        let q = emit(&cells)
            .expect("ValidationOnly contributes a put query")
            .query;
        let head = construct_head(&q);

        // The bare interior triple (`?x a gmeow:ModelArtifact`) is DELIBERATELY absent — the
        // lift must never be asserted as fact. This is the load-bearing C2 assertion.
        assert!(
            !head.contains("?x a gmeow:ModelArtifact"),
            "the interior class triple must NOT be asserted:\n{q}"
        );
        // The reified StatementMetadata triad is present, naming the class lift.
        assert!(
            head.contains("rdf:type gmeow:StatementMetadata"),
            "no cell type:\n{q}"
        );
        assert!(head.contains("gmeow:qSubject ?x"), "no qSubject:\n{q}");
        assert!(
            head.contains("gmeow:qPredicate rdf:type"),
            "no qPredicate:\n{q}"
        );
        assert!(
            head.contains("gmeow:qObject gmeow:ModelArtifact"),
            "qObject must name the lifted class:\n{q}"
        );
        // mappedFrom moved onto the annotation list (AnnotationProperty-clean).
        assert!(
            head.contains("gmeow:annProperty gmeow:mappedFrom"),
            "no mappedFrom ann:\n{q}"
        );
        assert!(
            head.contains("gmeow:annValue mls:Model"),
            "no mappedFrom value:\n{q}"
        );
        // Provenance: the cell is wasGeneratedBy the one coalesced import activity.
        assert!(
            head.contains(
                "gmeow:wasGeneratedBy <https://blackcatinformatics.ca/gmeow/import/ml-schema>"
            ),
            "cell must be wasGeneratedBy the per-profile import IRI:\n{q}"
        );
        // Exactly ONE ImportActivity node (R3: emit-side cardinality, not runtime dedup).
        assert_eq!(
            head.matches("a gmeow:ImportActivity").count(),
            1,
            "exactly one ImportActivity node in the CONSTRUCT head:\n{q}"
        );
        // Maximally-grounded, typed + labelled + agent-tied provenance (U2, unconditional).
        assert!(head.contains("rdfs:label \"inverse-ingest of ml-schema into GMEOW\""));
        assert!(
            head.contains(
                "gmeow:wasAssociatedWith \
                 <https://blackcatinformatics.ca/gmeow/agent/put-projection-importer>"
            ),
            "import activity must be tied to its software agent:\n{q}"
        );
        assert!(
            head.contains(
                "<https://blackcatinformatics.ca/gmeow/agent/put-projection-importer> \
                 a gmeow:SoftwareAgent"
            ),
            "the importer agent must be typed:\n{q}"
        );
        // No per-solution blank, no overclaim, no wall-clock.
        assert!(!q.contains("_:imp"), "no per-solution import blank:\n{q}");
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
        // The WHERE still matches the external target term.
        let where_clause = q.split("WHERE").nth(1).expect("query has a WHERE");
        assert!(
            where_clause.contains("?x a mls:Model ."),
            "WHERE must match the external target:\n{q}"
        );
    }

    #[test]
    fn mixed_profile_asserts_recovery_bare_and_reifies_the_lossy_lift_under_one_import() {
        // R5: a profile carrying BOTH a CompleteOver cell (mnemomorphic `=`) and a
        // ValidationOnly cell (`<=` + claim) must assert the recovery's source atoms BARE
        // (direct fact) while reifying the lossy lift as an inert claim — under exactly ONE
        // shared ImportActivity node.
        let recovery = class_cell("=", true, None);
        let mut lossy = class_cell("<=", false, Some(put_get_claim()));
        // Distinguish the lossy cell's anchor/iri so it is a separate contribution.
        lossy.pattern.anchor = "y".to_owned();
        lossy.pattern.atoms = vec![Item::Atom(type_atom("y", GM_MODEL_ARTIFACT))];
        lossy.iri = "https://blackcatinformatics.ca/gmeow/example/mlLossyCell".to_owned();
        let q = emit(&[recovery, lossy])
            .expect("mixed profile emits a query")
            .query;
        let head = construct_head(&q);

        // The CompleteOver recovery asserts its source atom directly (fact).
        assert!(
            head.contains("?x a gmeow:ModelArtifact ."),
            "the CompleteOver recovery must assert its source atom bare:\n{q}"
        );
        // The ValidationOnly lift is reified, NOT asserted.
        assert!(
            !head.contains("?y a gmeow:ModelArtifact"),
            "the ValidationOnly lift must not be asserted:\n{q}"
        );
        assert!(
            head.contains("gmeow:qSubject ?y"),
            "the lossy lift must be reified:\n{q}"
        );
        // Exactly one shared ImportActivity across both cells.
        assert_eq!(
            head.matches("a gmeow:ImportActivity").count(),
            1,
            "both cells share exactly one ImportActivity node:\n{q}"
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
        let (emitted, loss) = emit_with_loss(&[cell]);
        let emitted = emitted.expect("ValidationOnly contributes a put query");

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
        // The drops are read back from the ONE loss store, keyed by the row's target focus;
        // the per-run actual notes come back `actual: `-prefixed (the report's exact form).
        let drops = loss.projection_drops_for(&row.target);
        assert!(
            drops.contains(&format!("actual: {residue}")),
            "the authored disclosure must survive verbatim into the loss store:\n{drops:?}"
        );
        assert!(
            drops
                .iter()
                .any(|d| d.starts_with("actual: correspondence: ") && d.ends_with("::ml-schema")),
            "the row must carry its per-correspondence key note:\n{drops:?}"
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
