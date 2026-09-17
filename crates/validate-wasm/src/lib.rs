// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-validate-wasm — the repo-free Tier-1 validator, in the browser
//!
//! This crate compiles the wasm-clean [`gmeow-validate`](gmeow_validate) Tier-1 core to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript, so editor
//! plugins, browsers, and LLM clients can check authored GMEOW RDF against a
//! `gmeow.gts` bundle **client-side** — before submitting it — with no server, no
//! repository, and no Docker.
//!
//! ## Scope (by charter)
//!
//! - **Tier-1 only.** SHACL against the bundle's data-graph shape union plus the
//!   OntoUML disciplines — the checks that carry no reasoner. The Tier-2 `--deep`
//!   semantic pass reasons via the native DL engine, which does not compile to wasm;
//!   it is excluded here by contract, not degraded, so this surface exposes exactly
//!   the deep-less [`gmeow_validate::data_validate::validate_json`] core.
//! - **JSON boundary.** [`validate`] takes the RDF text, its format, the bundle
//!   bytes, the GMEOW namespace, and the data file's display path, and returns the
//!   canonical diagnostics `Report` serialized to JSON — the same shape the native
//!   CLI and the SARIF bridge project from.
//! - **GMN-1 conformance.** [`gmn_validate`] reads a GMN-1 document through the
//!   production codec ([`gmeow_lang_bridge::gmn1_read`]) against complete native tables
//!   prepared from the authored language source and embedded in the wasm image, pinned
//!   by [`GMN_CODEBOOK_DIGEST`], returning the typed `lang:LangConformanceFailure` verdict.
//!   Embedding the codebook is what makes this a real validator rather than a syntax
//!   check: an unbound glyph / uncovered term is rejected because it fails to RESOLVE.
//!   This path is reasoner-free — it links only the codec + graph-derived dictionary +
//!   the purrdf RDF core (the `make wasm` GMN purity gate proves no reasoner leak).
//!
//! ## Architecture
//!
//! The `#[wasm_bindgen]` surface is a thin shim: `gmeow-validate` already returns a
//! JSON string with no Python or filesystem coupling, so this crate only marshals the
//! JS strings/bytes across the boundary and maps the validator's `String` error onto a
//! JS exception. The validation logic lives in `gmeow-validate` so it unit-tests on
//! the native workspace gate; the wasm-bindgen wrapper is exercised as real wasm by
//! the Node round-trip lane.

use std::sync::OnceLock;

use gmeow_lang_bridge::{
    Gmn1Document, GmnDictionary,
    gmn_validation::validate_gmn_document,
    gmn1_codec::native::{self, NativeCodebook},
};
use wasm_bindgen::prelude::*;

// ── GMN-1 validator: the embedded codebook ──────────────────────────────────────────

/// Full native codebook projected from the producer's shared source dictionary.
/// No authored RDF is embedded or parsed by the browser or its Node tests.
const NATIVE_CODEBOOK: &[u8] =
    include_bytes!("../../../generated/projections/lang/gmn-codebook.cbor");

/// Pinned original-source BLAKE3, shared by both browser codebook consumers.
/// The native codec's pure source-byte test checks the pin without compiling RDF.
pub const GMN_CODEBOOK_DIGEST: &str = native::SOURCE_BLAKE3;

/// Complete native codebook, hydrated once from the producer's compact projection.
///
/// The packet is source-pinned and authenticated before this crate builds. Invalid
/// embedded bytes are a build-integrity violation and hard-fail as a panic/wasm trap.
fn embedded_codebook() -> &'static NativeCodebook {
    static CODEBOOK: OnceLock<NativeCodebook> = OnceLock::new();
    CODEBOOK.get_or_init(|| {
        native::decode(NATIVE_CODEBOOK, GMN_CODEBOOK_DIGEST)
            .expect("embedded native GMN codebook must match its exact producer-selected source")
    })
}

/// The BLAKE3 digest of the original GMN-1 source document (`module.ttl`), as
/// lowercase hex. The prepared native packet preserves this original-byte identity,
/// so a JS caller can pin the source their document was validated against.
#[wasm_bindgen]
pub fn gmn_codebook_digest() -> String {
    embedded_codebook().source_blake3().to_owned()
}

/// Validate a GMN-1 document against the EMBEDDED codebook, returning a canonical JSON
/// verdict.
///
/// The `bytes` are the raw GMN-1 surface text (the `@gmn{…}` header plus one record per
/// line). They are read through [`gmeow_lang_bridge::gmn1_read`] — the production codec's reader — against the
/// dictionary/glyph registry hydrated from the embedded native packet. Because the
/// codebook is embedded, glyphs, dictionary aliases, and prefixed terms are actually
/// RESOLVED: a document whose grammar is well-formed but which names a term the codebook
/// does not cover is rejected as `lang:GmnUncoveredTerm`, and every other codec-tier
/// violation resolves to its one typed `lang:LangConformanceFailure` class.
///
/// # Returns
///
/// A JSON object:
/// - conformant: `{ "conformant": true }` — the document read back cleanly.
/// - non-conformant: `{ "conformant": false, "failureClass":
///   "https://blackcatinformatics.ca/lang/Gmn…", "detail": "…" }` — `failureClass` is the
///   full `lang:` IRI from [`gmeow_lang_bridge::Gmn1Error::failure_class`] (the ONE
///   canonical classifier), `detail` its human-readable rendering.
///
/// # Errors
///
/// Throws a JS exception only if the document text is not valid UTF-8. A build-integrity
/// failure of the embedded native codebook is not a runtime condition — its complete
/// tables are a pinned build artifact (see [`embedded_codebook`])
/// — so it hard-fails as a panic / wasm trap, never a document defect and never a silent
/// degradation to a syntax-only check.
#[wasm_bindgen]
pub fn gmn_validate(bytes: &[u8]) -> Result<String, JsError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| JsError::new(&format!("GMN-1 document is not valid UTF-8: {e}")))?;
    Ok(gmn_verdict_json(text, embedded_codebook().dictionary()))
}

/// Marshal the same typed verdict the producer observes over its native dictionary.
fn gmn_verdict_json(text: &str, dictionary: &GmnDictionary) -> String {
    let verdict = validate_gmn_document(&Gmn1Document::from_text(text), dictionary);
    serde_json::to_string(&verdict).expect("GMN verdict contains only JSON scalars")
}

/// The validator version (the crate's SemVer), exposed to JS as `version()`.
///
/// A liveness probe for the wasm build + the npm package: importing the module and
/// calling `version()` proves it instantiated and the validator core linked.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Run Tier-1 conformance of `data` (RDF text in `format`) against the SHACL shapes
/// and OntoUML disciplines carried in the `gts` bundle bytes, returning the
/// diagnostics `Report` as a JSON string.
///
/// - `data` — the RDF document to validate (UTF-8 text).
/// - `format` — a media type or short id understood by the validator
///   (`turtle`/`ttl`, `trig`, `n-triples`/`nt`, `n-quads`/`nq`, `rdf+xml`, or the
///   JSON-LD ids `json-ld`/`jsonld`).
/// - `gts` — the `gmeow.gts` bundle bytes (carrying the `shapes-archive`).
/// - `namespace` — the GMEOW IRI prefix the discipline checks key on.
/// - `origin` — the data file's display path, recorded as each finding's location.
///
/// The returned JSON is the canonical `Report`: `{ "tool": "validate", "findings":
/// [ { "severity": "error"|"warning"|"note", "code": ..., ... } ] }`, with `findings`
/// omitted when the graph conforms.
///
/// # Errors
///
/// Throws a JS exception if the bundle carries no `shapes-archive`, the archive or
/// shapes are malformed, or the data graph fails to parse.
#[wasm_bindgen]
pub fn validate(
    data: &str,
    format: &str,
    gts: &[u8],
    namespace: &str,
    origin: &str,
) -> Result<String, JsError> {
    gmeow_validate::data_validate::validate_json(data.as_bytes(), format, gts, namespace, origin)
        .map_err(|e| JsError::new(e.message()))
}

/// Extract a `gmeow.gts` bundle's RDF as **graph-preserving N-Quads text**, so an
/// in-browser RDF engine (gmeow-query-wasm) can parse and query the SAME
/// bundle the pipeline shipped — the browser source of truth for the documentation
/// playground and bundle explorer, replacing any second curated data path.
///
/// - `gts` — the `gmeow.gts` bundle bytes (the single canonical browser-query
///   bundle; the container is read, not re-embedded).
///
/// Returns N-Quads (`application/n-quads`) covering every named graph in the bundle
/// (the graph component of each quad is retained — the query surface sees the
/// bundle's real graph structure, not a flattened union).
///
/// # Errors
///
/// Throws a JS exception if the container cannot be read, the statement layer cannot
/// be folded, or the dataset cannot be serialized.
#[wasm_bindgen]
pub fn bundle_dataset(gts: &[u8]) -> Result<String, JsError> {
    gmeow_validate::store::dataset_nquads_from_gts(gts).map_err(|e| JsError::new(e.message()))
}

// ── GMN-1 validator host tests ──────────────────────────────────────────────────────
//
// The shared target-independent verdict logic runs over the original source dictionary
// before tests start. Host contracts consume those selected observations: accept a frozen
// conformant vector and REJECT a grammar-valid document naming an unbound term.
// The wasm+JS boundary is separately exercised as real wasm by the Node round-trip lane.
#[cfg(test)]
mod gmn_tests {
    use super::*;

    /// A frozen positive conformance vector: a basic `@c{s p o}` claim over dictionary /
    /// prefix-covered terms.
    const FROZEN_POSITIVE: &str = "slices/grounding/lang/tests/gmn1-vectors/claim-basic.gmn";

    /// A frozen codec-tier negative: grammar-valid (`@c{s p o q}`, a known sigil, known
    /// keys, a well-formed number) but every term (`zx9`, `quuxes`, `gate1`) is UNCOVERED
    /// by the codebook. The recorded class is `lang:GmnUncoveredTerm`
    /// (`negative-codec/expected.ttl`).
    const FROZEN_UNKNOWN_GLYPH: &str =
        "slices/grounding/lang/tests/gmn1-vectors/negative-codec/neg-uncovered-term.gmn";

    fn verdict(path: &str) -> serde_json::Value {
        static OBSERVATIONS: OnceLock<serde_json::Value> = OnceLock::new();
        let observed = OBSERVATIONS.get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                "pipeline/wasm-gmn-verdicts.json",
            )
            .expect("browser GMN verdicts have an authenticated producer selection");
            serde_json::from_slice(&bytes).expect("produced GMN verdict JSON")
        });
        observed
            .get(path)
            .expect("selected frozen vector has a verdict")
            .clone()
    }

    #[test]
    fn gmn_wasm_accepts_a_frozen_vector() {
        let verdict = verdict(FROZEN_POSITIVE);
        assert_eq!(
            verdict["conformant"],
            serde_json::Value::Bool(true),
            "the frozen positive vector must validate against the embedded codebook, got {verdict}"
        );
    }

    #[test]
    fn gmn_wasm_rejects_unknown_glyph_vector() {
        // The load-bearing proof that the codebook is EMBEDDED and consulted: this document
        // is syntactically well-formed, so a syntax-only checker would accept it. It is
        // rejected ONLY because `gmn1_read` resolves its terms against the embedded
        // dictionary/glyph registry and finds them uncovered.
        let verdict = verdict(FROZEN_UNKNOWN_GLYPH);
        assert_eq!(
            verdict["conformant"],
            serde_json::Value::Bool(false),
            "a document naming an unbound/uncovered term must be REJECTED, got {verdict}"
        );
        assert_eq!(
            verdict["failureClass"],
            serde_json::Value::String(
                "https://blackcatinformatics.ca/lang/GmnUncoveredTerm".to_owned()
            ),
            "the rejection must be the codebook-resolution class GmnUncoveredTerm (not a \
             grammar/syntax class), proving the embedded codebook was actually consulted: {verdict}"
        );
    }

    /// Exercise the public JSON marshal with an explicit tiny dictionary and document.
    /// Real-codebook resolution remains the independently produced vector observation.
    #[test]
    fn gmn_verdict_json_retains_success_and_typed_failure_fields() {
        let dictionary = GmnDictionary::default();
        let document = gmeow_lang_bridge::gmn1_write(
            &gmeow_lang_bridge::Gmn0Model { quads: Vec::new() },
            &dictionary,
        )
        .expect("explicit empty model encodes");
        assert_eq!(
            gmn_verdict_json(&document.text, &dictionary),
            "{\"conformant\":true}"
        );
        let negative = gmn_verdict_json("@synthetic_unknown_sigil{}", &dictionary);
        let verdict: serde_json::Value = serde_json::from_str(&negative).expect("verdict JSON");
        assert_eq!(verdict["conformant"], false);
        assert_eq!(
            verdict["failureClass"],
            "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar"
        );
        assert!(
            !verdict["detail"]
                .as_str()
                .expect("failure detail")
                .is_empty()
        );
        assert_eq!(verdict.as_object().expect("verdict object").len(), 3);
    }

    #[test]
    fn gmn_codebook_digest_is_pinned() {
        // The hydrated packet must report its exact pinned original-source identity.
        // The codec's separate pure hash test checks that pin against the source bytes.
        assert_eq!(
            gmn_codebook_digest(),
            GMN_CODEBOOK_DIGEST,
            "the prepared codebook must report the pinned original module.ttl digest"
        );
    }
}
