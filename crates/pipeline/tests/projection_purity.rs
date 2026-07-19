// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection-purity leak guard for the exact-numeric value tower (issue 1428).
//!
//! The moded builtin evaluator carries [`gmeow_logic`]'s `Value` tower — exact
//! rationals, SI ℚ⁷ dimension vectors, and dimensioned quantities — between
//! builtins during reasoning as TRANSPORT-TAGGED literals, using the engine-internal
//! datatype IRIs `urn:gmeow:transport:{rational,dimension,quantity}`. Those literals
//! are a reasoning-runtime carrier ONLY; they are never authored in a slice and must
//! never be projected into the shipped `generated/dist/gmeow.gts` bundle as asserted
//! ontology content. If one leaks, a downstream consumer would treat an engine-private
//! transport encoding as canonical data.
//!
//! This gate reads the materialized bundle and asserts the three transport datatype
//! IRIs appear NOWHERE in its term table. It is the mechanized complement to the
//! superset/fold gate: the new `logic:Constraint`s the numeric-builtin work authored
//! (`logic:builtinBilinearSquaredDistance`, the `math:` gap classes) are legitimate
//! DECLARED terms and are expected in the bundle — what must not appear is the value
//! TRANSPORT surface.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// The engine-internal value-transport datatype IRIs (see
/// `crates/logic/src/physical/builtin_eval.rs`). None may appear in the shipped bundle.
const TRANSPORT_DATATYPES: &[&str] = &[
    "urn:gmeow:transport:rational",
    "urn:gmeow:transport:dimension",
    "urn:gmeow:transport:quantity",
];

#[test]
fn bundle_carries_no_value_transport_surface() {
    let bundle = repo_root().join("generated/dist/gmeow.gts");
    let bytes = std::fs::read(&bundle).unwrap_or_else(|e| {
        panic!(
            "materialized bundle {} is required for the projection-purity gate — run \
             `make sync` first: {e}",
            bundle.display()
        )
    });

    // Read every interned term of the CBOR bundle. A transport-tagged literal would
    // intern its datatype IRI as a term, so scanning the term table catches a leak in
    // any position (subject, predicate, object, graph name, or datatype reference).
    let graph = purrdf::gts::read_graph(&bytes, true).expect("read gmeow.gts");
    let mut leaks: Vec<String> = Vec::new();
    // Non-vacuity guard: the numeric-builtin work DID author a first-class declared
    // term. If the bundle does not carry it, the projection is empty/wrong and the
    // leak-absence below would be meaningless.
    const BILINEAR_BUILTIN_IRI: &str =
        "https://blackcatinformatics.ca/logic/builtinBilinearSquaredDistance";
    let mut saw_declared_builtin = false;
    for term in &graph.terms {
        let Some(value) = term.value.as_deref() else {
            continue;
        };
        if value == BILINEAR_BUILTIN_IRI {
            saw_declared_builtin = true;
        }
        for dt in TRANSPORT_DATATYPES {
            if value == *dt || value.contains(dt) {
                leaks.push(value.to_owned());
            }
        }
    }
    leaks.sort();
    leaks.dedup();
    assert!(
        saw_declared_builtin,
        "the bundle does not carry {BILINEAR_BUILTIN_IRI} — the numeric-builtin ontology \
         content did not project, so the transport-surface leak check would be vacuous"
    );
    assert!(
        leaks.is_empty(),
        "engine-internal value-transport surface leaked into generated/dist/gmeow.gts \
         — these are reasoning-runtime carriers, never shipped ontology content: {leaks:?}"
    );
}
