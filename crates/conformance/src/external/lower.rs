// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lowering an external problem's declared outcome into the runner verdict shape.
//!
//! [`runner_verdict_json`] gives, for an external problem's declared
//! [`ExternalOutcome`], the runner's `verdicts.json` value (the same world-indexed
//! shape the engine emits) — "the runner ingests a manifest / SZS problem and
//! produces a runner verdict".
//!
//! Lowering an external *source* into the on-disk case anatomy (`input.nq`) is a
//! separate concern. For a TPTP FOF/CNF problem it is fully mechanical — the
//! [`crate::external::tptp`] pipeline parses the body, applies the FOL-negation
//! reduction, and lowers the EL/DL-expressible fragment to a world-scoped EDB. For
//! the W3C entailment seeds the negated conclusion is pre-baked in the authored
//! `input.nq`. [`premise_ds_to_world_nquads`] world-scopes the W3C-manifest
//! ingest. The TPTP lowerer builds its native world-scoped dataset directly.

use std::collections::BTreeMap;

use gmeow_errors::Diag;

use crate::error::NquadsLowering;
use crate::external::status::ExternalOutcome;

/// Convert a parsed premise dataset (default graph only) into sorted, deduped
/// N-Quads under the given world IRI, returning the N-Quads text (sorted,
/// trailing newline) and the quad count.
///
/// The native N-Triples serializer does all the term encoding (IRI angle
/// brackets, literal escaping, datatype IRIs, lang tags, blank-node labels), so
/// the world-scoping never re-implements it. This is the shared lowering waist:
/// W3C-manifest sources produce a default-graph dataset and world-scope it here,
/// so their external N-Quads encoding has one owner.
pub fn premise_ds_to_world_nquads(
    ds: &purrdf::RdfDataset,
    world_iri: &str,
) -> gmeow_errors::Result<(String, usize)> {
    let nt_bytes = purrdf::serialize_dataset(
        ds,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| {
        Diag::of_kind(NquadsLowering {
            detail: format!("N-Triples serialize failed: {e}"),
        })
    })?;
    let nt_text = String::from_utf8(nt_bytes).map_err(|_| {
        Diag::of_kind(NquadsLowering {
            detail: "N-Triples output was not valid UTF-8".to_string(),
        })
    })?;

    // Convert each N-Triple line (`S P O .`) to N-Quads (`S P O <graph> .`).
    // Trim trailing whitespace FIRST so the mandatory '.' is last, then strip it,
    // then trim again — the reverse order would leave a `. ` line with two
    // terminators (`S P O . <graph> .`).
    let mut nq_lines: Vec<String> = nt_text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| {
            let trimmed = line.trim_end();
            let without_dot = trimmed.strip_suffix('.').ok_or_else(|| {
                Diag::of_kind(NquadsLowering {
                    detail: format!("malformed N-Triples line (no trailing '.'): {line}"),
                })
            })?;
            let body = without_dot.trim_end();
            Ok(format!("{body} <{world_iri}> ."))
        })
        .collect::<gmeow_errors::Result<Vec<String>>>()?;
    nq_lines.sort();
    nq_lines.dedup();

    let count = nq_lines.len();
    let text = if nq_lines.is_empty() {
        String::new()
    } else {
        let mut s = nq_lines.join("\n");
        s.push('\n');
        s
    };
    Ok((text, count))
}

/// Build the runner's `verdicts.json` value for a single-world external problem.
///
/// `{ world_iri: { quads, status } }` — the same shape the engine emits, with the
/// status taken from the external declaration. `quads` is the EDB quad count in the
/// world (so the value matches the engine's blessed output for a decided case).
pub fn runner_verdict_json(
    world_iri: &str,
    quads: u64,
    outcome: ExternalOutcome,
) -> serde_json::Value {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    counts.insert(world_iri.to_string(), quads);
    crate::serialize::build_verdicts(&counts, |_| outcome.verdict_status())
}

#[path = "lower.tests.rs"]
#[cfg(test)]
mod tests;
