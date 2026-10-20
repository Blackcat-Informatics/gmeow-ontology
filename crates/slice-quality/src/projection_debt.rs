// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Live authored-source debt measured against a separately supplied governance authority.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;

use crate::model::{CountKind, GovernanceFloors};

/// One measured cell. A deficit is report data, not permission to change a ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionDebtRow {
    /// Identity declared by the source manifest.
    pub slice: String,
    /// Prefix declared by the trusted vocabulary registry.
    pub vocabulary: String,
    /// Live ungrounded authoring constructs from the shared counter.
    pub measured: u64,
    /// Trusted committed ceiling, or the registry's explicit default.
    pub ceiling: u64,
}

impl ProjectionDebtRow {
    /// Exact signed difference for the entire u64 input domain.
    #[must_use]
    pub fn headroom(&self) -> i128 {
        i128::from(self.ceiling) - i128::from(self.measured)
    }
}

fn invalid(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Report {
        detail: detail.into(),
    })
}

/// Measure an external slice without a checkout, generation, or a corpus fixture.
/// Policy comes only from `governance`; source-local commitments cannot raise it.
/// Documentary non-RDF registry entries are not presented as measured RDF cells.
///
/// # Errors
/// Rejects missing/ambiguous manifests, missing modules, malformed or unreadable
/// authoring sources, and missing/ambiguous registry or ceiling definitions.
pub fn measure_slice_debt(
    slice_dir: &Path,
    governance: &GovernanceFloors,
) -> gmeow_errors::Result<Vec<ProjectionDebtRow>> {
    if !governance
        .vocabularies
        .iter()
        .any(|vocabulary| vocabulary.count_kind != CountKind::NonRdfSurface)
    {
        return Err(invalid(
            "projection debt requires a guarded vocabulary registry",
        ));
    }
    let slice = crate::slice_iri_of_dir(slice_dir)?;
    let paths = crate::ratchet_surface_paths(slice_dir)?;
    let measured =
        crate::measure_surface_residue_constructs(&paths, &slice, &governance.vocabularies)?;
    let mut seen = BTreeSet::new();
    let mut rows = Vec::with_capacity(governance.vocabularies.len());
    for vocabulary in &governance.vocabularies {
        if !seen.insert(vocabulary.prefix.as_str()) {
            return Err(invalid(format!(
                "duplicate guarded prefix {}",
                vocabulary.prefix
            )));
        }
        if vocabulary.count_kind == CountKind::NonRdfSurface {
            continue;
        }
        let commitments: Vec<_> = governance
            .ceilings
            .iter()
            .filter(|ceiling| ceiling.slice == slice && ceiling.vocab_prefix == vocabulary.prefix)
            .collect();
        if commitments.len() > 1 {
            return Err(invalid(format!(
                "duplicate ceiling for {slice} / {}",
                vocabulary.prefix
            )));
        }
        let count = measured.get(&vocabulary.prefix).map_or(0, Vec::len);
        rows.push(ProjectionDebtRow {
            slice: slice.clone(),
            vocabulary: vocabulary.prefix.clone(),
            measured: u64::try_from(count).map_err(|_| invalid("residue count exceeds u64"))?,
            ceiling: commitments
                .first()
                .map_or(vocabulary.default_ceiling, |ceiling| ceiling.count),
        });
    }
    if let Some(unknown) = governance
        .ceilings
        .iter()
        .find(|ceiling| ceiling.slice == slice && !seen.contains(ceiling.vocab_prefix.as_str()))
    {
        return Err(invalid(format!(
            "ceiling names unknown guarded prefix {}",
            unknown.vocab_prefix
        )));
    }
    rows.sort_by(|left, right| {
        (&left.slice, &left.vocabulary).cmp(&(&right.slice, &right.vocabulary))
    });
    Ok(rows)
}

/// Render exact rows and totals. No signed-64-bit saturation or wrapping totals.
///
/// # Errors
/// Refuses unknown formats and aggregate counts exceeding u128.
pub fn render_projection_debt(
    rows: &[ProjectionDebtRow],
    format: &str,
) -> gmeow_errors::Result<String> {
    if !matches!(format, "tsv" | "human") {
        return Err(invalid(format!(
            "unknown projection debt format {format:?}: expected human or tsv"
        )));
    }
    let mut out = String::new();
    if format == "tsv" {
        out.push_str("slice\tvocab\tmeasured\tceiling\theadroom\n");
    } else {
        out.push_str("Slice / vocabulary: measured, ceiling, headroom\n");
    }
    let mut measured = 0u128;
    let mut ceiling = 0u128;
    let mut at_ceiling = 0usize;
    for row in rows {
        measured = measured
            .checked_add(u128::from(row.measured))
            .ok_or_else(|| invalid("aggregate measured residue exceeds u128"))?;
        ceiling = ceiling
            .checked_add(u128::from(row.ceiling))
            .ok_or_else(|| invalid("aggregate ceiling exceeds u128"))?;
        at_ceiling += usize::from(row.measured == row.ceiling);
        if format == "tsv" {
            writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}",
                row.slice,
                row.vocabulary,
                row.measured,
                row.ceiling,
                row.headroom()
            )
            .expect("writing a String cannot fail");
        } else {
            writeln!(
                out,
                "{} / {}: {}, {}, {}",
                row.slice,
                row.vocabulary,
                row.measured,
                row.ceiling,
                row.headroom()
            )
            .expect("writing a String cannot fail");
        }
    }
    let headroom = if ceiling >= measured {
        (ceiling - measured).to_string()
    } else {
        format!("-{}", measured - ceiling)
    };
    writeln!(
        out,
        "# total measured={measured} total headroom={headroom} at-ceiling={at_ceiling} cells={}",
        rows.len()
    )
    .expect("writing a String cannot fail");
    Ok(out)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "projection_debt.tests.rs"]
mod tests;
