// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared helpers for the `*_decides_w3c_divergence` acceptance suites
//! (`casesplit_decides_w3c_divergence`, `counting_decides_w3c_divergence`,
//! `datatype_value_space_decides_w3c_divergence`).
//!
//! Each suite re-runs a handful of committed W3C OWL 2 Full divergence slugs
//! through the exact same `dl_consistency` path the grader/runner uses, and
//! resolves each slug's `input.nq` by checking the relocated
//! `w3c-owl2-full-decided` corpus first, falling back to the sibling
//! `w3c-owl2-full-divergence` corpus. This module is the single source for
//! that path-resolution + verdict-mapping machinery.
//!
//! Unlike the sibling `gmeow-conformance` helpers, these are NOT
//! timeout-guarded — none of the named slugs these suites exercise are the
//! known memory/compute-heavy chase cases, and no per-case timeout existed in
//! any of the three duplicated originals, so none is added here.

#![allow(dead_code)] // not every binary uses every helper

use std::path::{Path, PathBuf};

use purrdf::{NativeRdfFormat, dataset_from_bytes};

/// Resolve a slug's `input.nq`, looking in the `w3c-owl2-full-decided` corpus
/// first (the cases the kernel now DECIDES were relocated there) and falling
/// back to the sibling `w3c-owl2-full-divergence` corpus (the still-withheld
/// cases). The two corpora partition the original W3C-full set, so exactly one
/// holds the slug.
pub fn case_input(slug: &str) -> PathBuf {
    let external =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/logic/cases/external");
    let decided = external
        .join("w3c-owl2-full-decided")
        .join(slug)
        .join("input.nq");
    if decided.is_file() {
        return decided;
    }
    external
        .join("w3c-owl2-full-divergence")
        .join(slug)
        .join("input.nq")
}

/// The native verdict token for one slug (`consistent` / `inconsistent` /
/// `incomplete`), computed exactly as the grader/runner does: a non-empty
/// `gaps` is `incomplete` (an honest "cannot decide"); otherwise the
/// consistency boolean.
pub fn native_token(slug: &str) -> String {
    let path = case_input(slug);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
        .unwrap_or_else(|e| panic!("dl_consistency on {slug}: {e}"));
    if !verdict.gaps.is_empty() {
        "incomplete".to_owned()
    } else if verdict.consistent {
        "consistent".to_owned()
    } else {
        "inconsistent".to_owned()
    }
}
