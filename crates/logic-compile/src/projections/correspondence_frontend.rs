// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `dsl/mappings/` **correspondence frontend** (#1092 F5): materialize ONE typed
//! [`Correspondence`] IR node per authored alignment cell, so the carrier holds a real
//! [`CorrespondenceProgram`] instead of an empty `LogicProgram.correspondences` whose
//! ledger is reconstructed ad hoc downstream.
//!
//! Two cell kinds feed the program, each reusing the SAME typed derivation its dialect
//! lowering already trusts — never a forked mapping:
//!
//! * a `gmeow:TermEquivalence` cell (the SSSOM 1:1 band): its relation + morphism class
//!   come from [`sssom::sssom_band`] (so the typed node and the rendered SSSOM TSV agree
//!   by construction), its confidence from `gmeow:confidence`, and its evidence strength
//!   from the justification band ([`evidence_strength_of_justification`]);
//! * a `gmeow:ProjectionMapping` per-profile binding (the EDOAL/SPARQL get leg): its
//!   `(relation, morphism class, morphism kind)` come from [`ProfileBinding::lattice`],
//!   its get leg references the cell's pattern, and its confidence from the binding.
//!
//! The correspondence IRI is content-addressed (`sha256` of the cell's identifying
//! fields), so re-running the transpiler over the same corpus mints byte-identical node
//! identities — the program keys stably across builds and the cache boundary.
//!
//! # Scope (F5 Task 1)
//!
//! This ONLY materializes the typed set; it does NOT re-seat how the four dialect
//! lowerings derive their relations (that is Task 2). The four rendered artifacts stay
//! byte-identical — this lane only ADDS the carried [`CorrespondenceProgram`].

use sha2::{Digest, Sha256};

use crate::ingest::DslView;
use crate::ir::{Correspondence, MorphismKind, PreservationKind, LOGIC_NAMESPACE};
use crate::projections::correspondence::CorrespondenceProgram;
use crate::projections::get_leg::{projections, ProfileBinding};
use crate::projections::sssom::{equivalence_cells, sssom_band};

/// The semapv justification under which a curator established a mapping — the
/// provenance-derived warrant the SSSOM cell carries. We map it to an
/// `evidenceStrength` band: a manually-curated mapping is a modest, non-zero warrant; a
/// lexical/structural heuristic would be weaker. An unknown/absent justification yields
/// `None` (never a fabricated number — the axis stays unset).
fn evidence_strength_of_justification(justification: Option<&str>) -> Option<f64> {
    let local = justification?.rsplit(['#', '/', ':']).next().unwrap_or("");
    Some(match local {
        // A human curator's deliberate assertion — a modest, non-zero warrant.
        "ManualMappingCuration" => 0.5,
        // Lexical/structural heuristics are weaker warrants than manual curation.
        "LexicalMatching" | "LexicalSimilarityThresholdMatching" => 0.3,
        "StructuralMatching" => 0.3,
        // An unrecognized justification: leave the axis unset rather than invent a value.
        _ => return None,
    })
}

/// A content-addressed correspondence IRI under `LOGIC_NAMESPACE` for the cell keyed by
/// `key`. The `sha256(key)[:16]` digest mirrors the established content-IRI minting
/// (`projections::mod` / `rdf.rs`), so the identity is stable, collision-free, and
/// IRI-legal. `tag` segments the two cell kinds so a term-equivalence and a projection
/// binding can never collide on the same digest.
fn correspondence_iri(tag: &str, key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{LOGIC_NAMESPACE}correspondence/{tag}/{hex}")
}

/// Transpile the authored `dsl/mappings/` cells into a typed [`CorrespondenceProgram`]:
/// one [`Correspondence`] per `gmeow:TermEquivalence` cell and one per
/// `gmeow:ProjectionMapping` per-profile binding.
///
/// `dsl_view` carries the alignment + mapping DSL; `onto_view` is accepted for symmetry
/// with the dialect lowerings (the EDOAL/SPARQL get-leg model reads it for ranges), so a
/// future enrichment of the materialized nodes from the ontology has the handle without a
/// signature change. The four dialect outputs are unaffected by this transpilation.
///
/// # Errors
///
/// Propagates a malformed `gmeow:ProjectionMapping` (the get-leg parser's hard error) or a
/// rejected [`Correspondence::new`] invariant (a bad confidence/leg). Construction is
/// fail-hard: a malformed cell is a build failure, never a silently-dropped node.
pub fn transpile_correspondences(
    dsl_view: &DslView,
    _onto_view: &DslView,
) -> Result<CorrespondenceProgram, String> {
    let mut correspondences: Vec<Correspondence> = Vec::new();

    // ── gmeow:TermEquivalence cells (the SSSOM 1:1 band) ───────────────────────────
    for cell in equivalence_cells(dsl_view) {
        // Relation + morphism class from the SAME band the SSSOM ledger gate uses.
        let (relation, morphism_class) = sssom_band(&cell.predicate);
        // The per-correspondence key folds (subject, predicate, object) — one subject may
        // align to several objects, so the triple (not just the subject) is the identity.
        let key = format!("{}|{}|{}", cell.subject, cell.predicate, cell.obj);
        let iri = correspondence_iri("term-equivalence", &key);
        let evidence_strength = evidence_strength_of_justification(cell.justification.as_deref());
        let corr = Correspondence::new(
            iri,
            relation,
            morphism_class,
            // The 1:1 SSSOM band is a satisfaction-preserving lens, never a bridge.
            MorphismKind::InstitutionMorphism,
            false,
            None,
            // The SSSOM 1:1 band carries only (subject, predicate, object) + confidence +
            // justification; it drops the get/put leg-program structure (that is what makes
            // its ledger row a `SoundUnder` drop), so the typed node leaves the legs unset.
            None,
            None,
            Vec::new(),
            cell.confidence,
            evidence_strength,
            None,
            None,
            // Unindexed cells are scoped to the unspecified standpoint (unspecified, not
            // universal): `accordingTo` stays unset.
            None,
        )?;
        correspondences.push(corr);
    }

    // ── gmeow:ProjectionMapping per-profile bindings (the EDOAL/SPARQL get leg) ─────
    for cell in projections(dsl_view)? {
        for binding in &cell.bindings {
            correspondences.push(correspondence_for_binding(&cell.iri, binding)?);
        }
    }

    // The frontend's preservation polarity for the lane: the alignment lowerings are a
    // sound under-approximation (they refuse the forced-equality reading), never exact.
    Ok(CorrespondenceProgram::new(
        correspondences,
        Vec::new(),
        PreservationKind::SoundUnder,
    ))
}

/// Materialize the typed [`Correspondence`] for one `gmeow:ProjectionMapping` profile
/// binding, reusing [`ProfileBinding::lattice`] for the relation/class/kind triple.
fn correspondence_for_binding(
    cell_iri: &str,
    binding: &ProfileBinding,
) -> Result<Correspondence, String> {
    let (relation, morphism_class, morphism_kind) = binding.lattice();
    // The per-profile target IRI the binding projects onto (predicate, class, or EDOAL
    // target — the first one named). It is the put leg's apex.
    let target = binding
        .to_predicate
        .as_deref()
        .or(binding.to_class.as_deref())
        .or(binding.edoal_target.as_deref())
        .unwrap_or("");
    // The per-correspondence key folds (cell IRI, profile, target): one mapping cell has
    // one binding per profile, each its own correspondence.
    let key = format!("{cell_iri}|{}|{target}", binding.profile);
    let iri = correspondence_iri("projection-mapping", &key);
    // The get leg references the pattern-bearing mapping cell (an IRI node, the acquired
    // source pattern); the put leg is the per-profile target IRI it projects onto, when
    // the binding names one. Both are absolute IRIs (the pattern's SPARQL-variable anchor
    // is NOT an IRI, so it is never used as a leg).
    let get_leg = Some(cell_iri.to_owned());
    let put_leg = (!target.trim().is_empty()).then(|| target.to_owned());
    Correspondence::new(
        iri,
        relation,
        morphism_class,
        morphism_kind,
        false,
        None,
        get_leg,
        put_leg,
        Vec::new(),
        binding.confidence,
        None,
        None,
        None,
        None,
    )
}

#[cfg(test)]
mod tests;
