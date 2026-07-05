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
