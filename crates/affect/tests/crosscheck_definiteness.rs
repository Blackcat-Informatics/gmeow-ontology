// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shipped-bundle definiteness gate: the AUTHORED `math:definiteness` of
//! `gmeow:coreAffectGram` in the committed `generated/dist/gmeow.gts` must be
//! certified by the COMPUTED exact-rational LDLᵀ positive-definiteness witness.
//!
//! Nothing else cross-checks the authored declaration against the derived
//! factorization; this closes that loop over the shippable deliverable.

use std::path::{Path, PathBuf};

use gmeow_affect::crosscheck_authored_definiteness;

const CORE_AFFECT_GRAM: &str = "https://blackcatinformatics.ca/gmeow/coreAffectGram";

/// Repository root used to authenticate the exact producer-selected bundle.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

// The shipped core-affect Gram (diagonal 1, valence–arousal coupling 1/4) is
// authored `math:positiveDefinite`; the LDLᵀ witness must AGREE, certifying it
// with pivots [1, 15/16, 1, 1] — all strictly positive (Sylvester's criterion).
#[test]
fn shipped_core_affect_gram_authored_pd_is_certified() {
    let bytes = gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("load the exact authenticated producer bundle without rebuilding it");
    let pivots = crosscheck_authored_definiteness(&bytes, CORE_AFFECT_GRAM)
        .expect("authored PD is certified by the LDLᵀ witness");
    assert_eq!(
        pivots,
        vec![
            "1".to_string(),
            "15/16".to_string(),
            "1".to_string(),
            "1".to_string(),
        ],
    );
}
