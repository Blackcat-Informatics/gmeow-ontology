// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The coat-side distinctiveness hard gate.
//!
//! A slice's per-term annotation coats must *distinguish* one term from another. This
//! gate rejects a near-duplicate: two distinct TBox terms carrying the same normalized
//! skeleton for a distinguishing coat. It is a hard boolean reject at N = 2 (any
//! collision), never a scored axis or a tuned floor — an axis score can only hard-fail
//! through a committed (calibrated) floor, which the no-calibration discipline forbids,
//! so this lives as a structural gate beside the rubric binding/completeness gates.
//!
//! The distinguishing coat predicates guarded are `gmeow:useWhen` / `avoidWhen` /
//! `howToUse` and `skos:definition`, all under one [`skeleton`] normalizer (lowercase +
//! whitespace-collapse, CURIEs kept — see [`gmeow_validate::distinctiveness`]): two terms
//! that differ only by a load-bearing CURIE (e.g. a usage coat naming its own distinct
//! range) are genuinely distinct and must NOT collide; only a byte-identical (modulo
//! case/space) coat across distinct terms is the near-duplicate.
//!
//! `skos:example` is deliberately out of scope: distinct terms legitimately cite the same
//! example individual, so a hard reject there would mis-fire. `gmeow:graphBoxRole` is a
//! controlled-vocabulary role, legitimately repeated. Only TBox terms are checked (an
//! A-Box individual is not a distinguishing coat).

use std::path::Path;

use gmeow_validate::distinctiveness::{Collision, collisions, skeleton};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef};

use crate::graph::{self, all_lits, g, id};
use crate::score::slice_terms;

/// The `skos:definition` IRI.
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
/// The `owl:` namespace — a TBox term is an `owl:Class` or an `owl:*Property`.
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
/// The usage-coat predicate local names guarded with the single no-strip skeleton
/// (lowercase + whitespace-collapse; CURIEs kept as content).
const USAGE_COATS: &[&str] = &["useWhen", "avoidWhen", "howToUse"];

/// Whether `subject` is a TBox term: typed `owl:Class` or some `owl:*Property`. Mirrors
/// the `is_tbox_term` discipline the information/prose axes use — usage coats and
/// distinguishing definitions are a TBox-term bar, so an A-Box value-vocabulary
/// individual is never checked here.
fn is_tbox(ds: &RdfDataset, subject: TermId) -> bool {
    let Some(type_p) = id(ds, graph::RDF_TYPE) else {
        return false;
    };
    ds.quads_for_pattern(Some(subject), Some(type_p), None, GraphMatch::Any)
        .any(|q| match ds.resolve(q.o) {
            TermRef::Iri(t) => {
                t == "http://www.w3.org/2002/07/owl#Class"
                    || (t.starts_with(OWL_NS) && t.ends_with("Property"))
            }
            _ => false,
        })
}

/// Format one collision into a gate `FAIL`-ready message naming the slice, the coat
/// predicate, the shared skeleton, and the colliding term IRIs.
fn message(slice_iri: &str, predicate: &str, c: &Collision) -> String {
    format!(
        "slice {slice_iri}: {} distinct terms share one {predicate} skeleton {:?} — a coat must distinguish its term (near-duplicate template): {}",
        c.members.len(),
        c.skeleton,
        c.members.join(", ")
    )
}

/// The near-duplicate coat collisions in the slice at `slice_dir` — non-empty ⇒ the gate
/// reds. Reads the slice's `module.ttl`, enumerates its own TBox terms, and detects, per
/// distinguishing coat predicate, any normalized skeleton shared by ≥2 distinct terms.
///
/// # Errors
/// A hard error if the slice `module.ttl` cannot be read/parsed or its ontology IRI
/// cannot be resolved — the gate never silently skips a slice it cannot read.
pub fn slice_coat_collisions(slice_dir: &Path) -> gmeow_errors::Result<Vec<String>> {
    let module = slice_dir.join("module.ttl");
    // A slice with no module.ttl (e.g. a profile/query slice) authors no TBox terms and
    // therefore no distinguishing coats — nothing to check, not a degradation.
    if !module.is_file() {
        return Ok(Vec::new());
    }
    let ds = crate::dataset_from_paths(&[&module])?;
    let slice_iri = crate::slice_iri_of_dir(slice_dir)?;
    let terms = slice_terms(&ds, &slice_iri);

    // The TBox subset of the slice's own terms, resolved once.
    let tbox: Vec<(String, TermId)> = terms
        .iter()
        .filter_map(|iri| id(&ds, iri).map(|sid| (iri.clone(), sid)))
        .filter(|(_, sid)| is_tbox(&ds, *sid))
        .collect();

    let mut out = Vec::new();

    // Usage coats, one predicate at a time (a shared avoidWhen is a distinct kind of
    // near-duplicate from a shared useWhen).
    for local in USAGE_COATS {
        let Some(pred) = id(&ds, &g(local)) else {
            continue; // predicate never used in this slice
        };
        let pairs: Vec<(String, String)> = tbox
            .iter()
            .flat_map(|(iri, sid)| {
                all_lits(&ds, *sid, pred)
                    .into_iter()
                    .map(move |v| (iri.clone(), skeleton(&v)))
            })
            .collect();
        for c in collisions(&pairs) {
            out.push(message(&slice_iri, local, &c));
        }
    }

    // Definitions: same exact-match skeleton (load-bearing CURIEs kept as content).
    if let Some(def_p) = id(&ds, SKOS_DEFINITION) {
        let pairs: Vec<(String, String)> = tbox
            .iter()
            .flat_map(|(iri, sid)| {
                all_lits(&ds, *sid, def_p)
                    .into_iter()
                    .map(move |v| (iri.clone(), skeleton(&v)))
            })
            .collect();
        for c in collisions(&pairs) {
            out.push(message(&slice_iri, "skos:definition", &c));
        }
    }

    Ok(out)
}
