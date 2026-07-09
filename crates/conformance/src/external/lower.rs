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
//! `input.nq`. [`premise_ds_to_world_nquads`] is the shared world-scoping waist both
//! paths (and the W3C-manifest ingest) funnel their default-graph triples through.

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
/// the W3C-manifest ingest and the TPTP FOL lowerer both produce a default-graph
/// dataset and world-scope it here, so a single code path owns the encoding.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_verdict_reflects_the_mapped_status() {
        let v = runner_verdict_json("https://w/x", 4, ExternalOutcome::Inconsistent);
        assert_eq!(v["https://w/x"]["status"], "inconsistent");
        assert_eq!(v["https://w/x"]["quads"], 4);

        let v = runner_verdict_json("https://w/x", 2, ExternalOutcome::Consistent);
        assert_eq!(v["https://w/x"]["status"], "consistent");

        let v = runner_verdict_json("https://w/x", 0, ExternalOutcome::Incomplete);
        assert_eq!(v["https://w/x"]["status"], "incomplete");
    }
}
