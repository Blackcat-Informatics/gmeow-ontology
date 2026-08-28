// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The W2b bundle-explorer `describe` WITNESS (T1/F2) — now an EQUIVALENCE.
//!
//! The browser bundle explorer answers `describe <term>` over the object-level ontology.
//! It used to do that by parsing a 27 MB `gmeow-core.nq` re-serialization in a vendored
//! purrdf wasm engine; it now sends one `query_local` frame at the MCP engine already
//! booted over `gmeow.gts`. The vendored engine is retired, so an attestation that pinned
//! only the native purrdf path would no longer be attesting the shipped surface.
//!
//! This test therefore proves BOTH sides against ONE committed attestation
//! (`tests/witness/describe.nt`):
//!
//! 1. the NATIVE oracle — the authenticated bundle importer restores the bundle's
//!    default object-level graph, and a deterministic renderer describes a deterministic
//!    subject out of it. This is the independent definition of "the object-level
//!    description of a term", derived without the MCP surface;
//! 2. the SHIPPED route — the exact `query_local` frame the docs controller sends
//!    (`describeQuery` in `crates/docs/assets/docs-controller.mjs`), rendered through the
//!    SAME renderer.
//!
//! Both must equal the committed bytes. That is strictly stronger than what the retired
//! witness proved: it pins the describe AND pins the two paths to each other, so the
//! explorer cannot drift from the projection it claims to describe.
//!
//! # Why a bound-subject CONSTRUCT and not `DESCRIBE`
//!
//! SPARQL leaves `DESCRIBE`'s result implementation-defined, and this engine's gathers
//! across every named graph: `DESCRIBE <AboutnessMode>` returns 38 quads, picking up the
//! documentation graph's `addedInVersion` / `definitionDigest` / `inScheme` rows. A
//! bound-subject pattern reads the active (default) graph alone and returns the 11 the
//! object-level ontology asserts. The explorer means the second, so the query says the
//! second rather than depending on a DESCRIBE dialect.
//!
//! Refreshed with the bundle only by `make maint-refresh-describe-witness`.

#[path = "support/explorer_describe.rs"]
mod explorer_describe;

use explorer_describe::{attestation_path, repo_root, verified_describe};

#[test]
fn explorer_describe_is_the_same_on_both_routes_and_matches_the_attestation() {
    let root = repo_root();
    let snapshot = gmeow_bundle_import::load_authenticated_source_bytes(&root)
        .expect("authenticated bundle; tests never produce it");
    let core = gmeow_bundle_import::load_authenticated_repository_bundle(&root)
        .expect("authenticated bundle dataset; tests never produce it")
        .dataset;
    let witness =
        verified_describe(&snapshot, core.as_ref()).expect("verified explorer describe witness");
    assert!(
        witness
            .subject
            .starts_with("https://blackcatinformatics.ca/gmeow/"),
        "the witness subject must belong to the GMEOW namespace"
    );
    let path = attestation_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "describe witness attestation {} missing; run `make maint-refresh-describe-witness`: {e}",
            path.display()
        )
    });
    assert_eq!(
        witness.rendered, committed,
        "the object-level describe drifted from the committed witness attestation"
    );
}
