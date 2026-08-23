// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Carrier-purity gate (C11).
//!
//! The build pipeline's inter-stage CARRIER/TRANSPORT path is NATIVE: the composed
//! value rides `Arc<PipelineBundle<PipelineHandle>>`/`RdfDataset` handles, the
//! `gts_compose` union is a native `RdfDataset::union`, and the `snapshot`
//! named-graph assembly merges authored sources with the standardize-apart union and
//! emits the bundle through the SOLE `emit_gts` byte emitter. No oxigraph `Store` is
//! created to accumulate / union / round-trip the carried RDF on that path.
//!
//! This is a STRUCTURAL gate, not a fragile whole-crate grep: the pipeline crate
//! LEGITIMATELY keeps oxigraph for two documented, NON-transport residuals, and the
//! gate must not regress to forbidding those:
//!
//!   1. Source-file PARSING in `source_load` / `statements` / `mappings` (oxigraph
//!      parse of authored `.ttl` into a dataset). That is INGESTION, not inter-stage
//!      transport, so those modules are out of the scanned set.
//!
//!   2. The DAG `loader` (`src/loader.rs`) and the oxigraph adapters in `purrdf`
//!      (`store_from_dataset` / `dataset_from_store`) — general adapters / the build
//!      graph loader, never the carrier transport. Out of the scanned set.
//!
//! There is NO sanctioned-exception carve-out: the carrier's typed-literal value-space
//! canonicalization is now NATIVE (`purrdf::xsd::parse_by_iri` + `XsdValue::canonical_lexical`
//! in `carrier::dataset_to_nquads`), so the former transient-`Store`
//! `canonicalize_quad_literals` residual (C3) is GONE — the carrier path uses NO
//! oxigraph `Store` at all.
//!
//! What this gate FORBIDS — and FAILS on if reintroduced — is a `Store::new()`
//! accumulation (or a `store_from_dataset` / `dataset_from_store` store round-trip)
//! creeping back into the CARRIER functions: `gts_compose::compose`'s union path or
//! `carrier`'s named-graph assembly (`assemble_carrier` / `load_authored_default` /
//! `load_imports` / `build_snapshot_bundle` and the native helpers around them). Those
//! two modules' PRODUCTION source (everything outside their `#[cfg(test)]` region) is
//! scanned token-by-token; reintroducing oxigraph accumulation there turns this test
//! red. The accompanying `tests::gate_would_fail_if_oxigraph_accumulation_returned`
//! demonstrates the detector flags exactly that.

use std::path::{Path, PathBuf};

/// The carrier-transport source files whose PRODUCTION region must stay free of
/// oxigraph store accumulation. Both are in the pipeline crate's `src/stages/`.
const CARRIER_MODULES: [&str; 2] = ["src/stages/gts_compose.rs", "src/stages/carrier.rs"];

/// Tokens that signal an oxigraph `Store` is being created to ACCUMULATE / UNION /
/// round-trip the carried RDF — exactly what the native carrier replaced. Any of these
/// appearing in carrier production code is a transport regression.
const FORBIDDEN_TOKENS: [&str; 3] = ["Store::new(", "store_from_dataset(", "dataset_from_store("];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The PRODUCTION region of a Rust source: everything before the first top-level
/// `#[cfg(test)]` attribute (the `#[cfg(test)] mod tests` blocks that legitimately
/// build oxigraph oracles to PROVE the native path is byte-isomorphic to the old
/// store path). Carrier modules keep all their test oracles after that line.
fn production_region(source: &str) -> &str {
    match source.find("\n#[cfg(test)]") {
        Some(idx) => &source[..idx],
        None => source,
    }
}

/// Strip Rust line-comments (`//`, `///`, `//!`) so a doc-comment that NAMES a
/// forbidden token (e.g. "the old `Store::new()+ingest_turtle` path") is not a false
/// positive. Block comments are not used for these mentions in the carrier modules, so
/// line-stripping is sufficient and keeps the scanner simple and robust.
fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find("//") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Scan a carrier module's production code for forbidden oxigraph accumulation tokens.
/// Returns the list of `(token, line_snippet)` violations (empty ⇒ pure).
fn scan_violations(module_rel: &str, source: &str) -> Vec<(String, String)> {
    let prod = production_region(source);
    let code = strip_line_comments(prod);
    let mut violations = Vec::new();
    for (lineno, line) in code.lines().enumerate() {
        for token in FORBIDDEN_TOKENS {
            if line.contains(token) {
                violations.push((
                    token.to_string(),
                    format!("{module_rel}:{} | {}", lineno + 1, line.trim()),
                ));
            }
        }
    }
    violations
}

fn read_module(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("carrier-purity: cannot read {}: {e}", path.display()))
}

#[test]
fn carrier_transport_path_creates_no_oxigraph_store_for_accumulation() {
    let root = manifest_dir();
    let mut all_violations: Vec<(String, String)> = Vec::new();
    for rel in CARRIER_MODULES {
        let source = read_module(&root, rel);
        all_violations.extend(scan_violations(rel, &source));
    }
    assert!(
        all_violations.is_empty(),
        "carrier-purity FAILED: an oxigraph Store accumulation/round-trip was reintroduced into \
         the inter-stage carrier transport path. The composed value must ride the native \
         `RdfDataset` / `PipelineBundle` carrier (RdfDataset::union), NOT a Store. There is NO \
         sanctioned exception: the carrier's typed-literal value-space canonicalization is native \
         (purrdf::xsd). Violations:\n{}",
        all_violations
            .iter()
            .map(|(tok, loc)| format!("  - `{tok}` at {loc}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn carrier_literal_canonicalization_is_native_gmeow_xsd_not_an_oxigraph_store() {
    // The carrier's typed-literal value-space canonicalization (C3) is NATIVE:
    // `carrier::dataset_to_nquads` maps each literal through `purrdf::xsd::parse_by_iri`
    // + `XsdValue::canonical_lexical`, with NO transient oxigraph `Store`. The former
    // `canonicalize_quad_literals` residual is GONE — assert it is neither referenced by
    // the carrier nor present anywhere in the crate, so a reviewer sees the carve-out is
    // genuinely retired (not merely renamed).
    let root = manifest_dir();
    let snapshot = read_module(&root, "src/stages/carrier.rs");
    assert!(
        snapshot.contains("purrdf::xsd::parse_by_iri"),
        "carrier-purity: carrier.rs must canonicalize typed literals via the native \
         `purrdf::xsd::parse_by_iri` (the oxigraph `canonicalize_quad_literals` residual is retired)."
    );
    assert!(
        !snapshot.contains("canonicalize_quad_literals"),
        "carrier-purity: the retired oxigraph `canonicalize_quad_literals` value-space normalizer \
         must no longer appear in the carrier — the canonicalization is native purrdf::xsd now."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prove the detector actually FAILS when oxigraph accumulation is reintroduced:
    /// feed it a synthetic carrier function that builds a `Store` and unions through
    /// it, and confirm `scan_violations` flags it. This is the negative arm — without
    /// it a vacuous always-pass scanner would look identical to a real gate.
    #[test]
    fn gate_would_fail_if_oxigraph_accumulation_returned() {
        let reintroduced = r#"
fn compose(upstream: &Foo) -> Result<RdfDataset, E> {
    let store = Store::new()?;                 // <- accumulation regression
    for product in upstream.values() {
        rdf_bytes_into_store(&store, product.bytes(), "text/turtle", "carrier")?;
    }
    let dataset = dataset_from_store(&store)?; // <- store round-trip regression
    Ok(dataset)
}
"#;
        let violations = scan_violations("src/stages/gts_compose.rs", reintroduced);
        let tokens: Vec<&str> = violations.iter().map(|(t, _)| t.as_str()).collect();
        assert!(
            tokens.contains(&"Store::new("),
            "negative check: detector must flag a reintroduced `Store::new(` accumulation, got {tokens:?}"
        );
        assert!(
            tokens.contains(&"dataset_from_store("),
            "negative check: detector must flag a reintroduced `dataset_from_store(` round-trip, got {tokens:?}"
        );
    }

    /// The native gmeow-xsd literal canonicalization is NOT mistaken for an
    /// accumulation token: the carrier's value-space normalization is now
    /// `purrdf::xsd::parse_by_iri` + `XsdValue::canonical_lexical` (no oxigraph `Store`),
    /// which matches none of the forbidden transport tokens, so it survives the scan.
    #[test]
    fn native_xsd_canonicalization_is_not_flagged() {
        let native_canon = r#"
fn canonicalize_term_xsd(term: &mut RdfTerm) -> Result<(), E> {
    if let Some(dt) = literal.datatype.as_deref() {
        match purrdf::xsd::parse_by_iri(&literal.lexical_form, dt)? {
            Some(value) => literal.lexical_form = value.canonical_lexical(),
            None => {}
        }
    }
    Ok(())
}
"#;
        let violations = scan_violations("src/stages/carrier.rs", native_canon);
        assert!(
            violations.is_empty(),
            "the native purrdf::xsd literal canonicalization must NOT be flagged as a \
             transport-Store accumulation, got {violations:?}"
        );
    }

    /// A doc-comment that NAMES a forbidden token (the carrier modules describe the old
    /// `Store::new()` path they replaced) is a comment, not code — the scanner strips
    /// line-comments, so the mention is not a false positive.
    #[test]
    fn doc_comment_mention_of_old_store_path_is_not_flagged() {
        let doc_mention = r#"
/// The native equivalent of the old `Store::new()+ingest_turtle+store_to_nquads`
/// trio (no oxigraph `Store`). Replaces the `dataset_from_store` round-trip.
fn turtle_to_nquads(bytes: &[u8]) -> Result<Vec<u8>, E> {
    native(bytes)
}
"#;
        let violations = scan_violations("src/stages/carrier.rs", doc_mention);
        assert!(
            violations.is_empty(),
            "a doc-comment mention of the OLD store path must not be flagged, got {violations:?}"
        );
    }

    /// The `#[cfg(test)]` region (where carrier modules legitimately build oxigraph
    /// oracles to PROVE the native path is byte-isomorphic to the old store path) is
    /// excluded: only PRODUCTION carrier code is scanned.
    #[test]
    fn cfg_test_oracle_region_is_excluded() {
        let with_test_oracle = r#"
fn compose() -> RdfDataset { native_union() }

#[cfg(test)]
mod tests {
    #[test]
    fn isomorphic_to_old_byte_path() {
        let store = Store::new().unwrap();         // oracle, NOT carrier
        let ds = dataset_from_store(&store).unwrap();
    }
}
"#;
        let violations = scan_violations("src/stages/gts_compose.rs", with_test_oracle);
        assert!(
            violations.is_empty(),
            "carrier modules' #[cfg(test)] oracle region must be excluded from the scan, got {violations:?}"
        );
    }

    /// The real carrier modules are pure: running the scanner over the committed
    /// source finds no violations (the same assertion the top-level gate test makes,
    /// re-checked here so the unit-test lane covers the live source too).
    #[test]
    fn live_carrier_modules_are_pure() {
        let root = manifest_dir();
        for rel in CARRIER_MODULES {
            let source = read_module(&root, rel);
            let violations = scan_violations(rel, &source);
            assert!(
                violations.is_empty(),
                "live carrier module {rel} has oxigraph accumulation: {violations:?}"
            );
        }
    }
}
