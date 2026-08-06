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
//!   production codec ([`gmeow_lang_bridge::gmn1_read`]) against a codebook EMBEDDED in
//!   the wasm image (the authored `slices/grounding/lang/module.ttl`, pinned by
//!   [`GMN_CODEBOOK_DIGEST`]), returning the typed `lang:LangConformanceFailure` verdict.
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

use gmeow_lang_bridge::{Gmn1Document, GmnDictionary, gmn1_read};
use wasm_bindgen::prelude::*;

// ── GMN-1 validator: the embedded codebook ──────────────────────────────────────────

/// The GMN-1 codebook carrier, embedded into the wasm image at build time.
///
/// This is the authored `slices/grounding/lang/module.ttl` verbatim — the SAME carrier the
/// `gmeow gmn` CLI resolves `gmeow:gmnCodebookCurrent` / `gmeow:gmnDictV3` from when given a
/// `--lang-module` override, and the SAME one every `gmeow-lang-bridge` codec test loads
/// (`GmnDictionary::from_dataset(&lang_module_dataset())`). Embedding it — rather than
/// requiring the caller to pass a bundle, or (worse) shipping a hand-trimmed subset that
/// would be a second, drift-prone source of truth — is what makes [`gmn_validate`] a REAL
/// validator: `gmn1_read` resolves every glyph / dictionary alias / prefix against this
/// codebook, so a grammar-valid document naming an unbound term is REJECTED
/// (`lang:GmnUncoveredTerm`), not waved through as a mere syntax check.
///
/// The full authored module (not a minimal extract) is embedded deliberately: it is the
/// canonical source with zero drift risk, and `resolve_current_codebook` /
/// `GmnDictionary::from_dataset` simply ignore the quads outside the codebook selection.
const GMN_CODEBOOK_TTL: &[u8] = include_bytes!("../../../slices/grounding/lang/module.ttl");

/// The pinned blake3 content digest of [`GMN_CODEBOOK_TTL`] (`b3sum module.ttl`). Recording
/// it here documents EXACTLY which codebook this wasm image validates against; the
/// `gmn_codebook_digest_is_pinned` host test recomputes it over the embedded bytes and
/// hard-fails if the two drift, so this constant can never silently fall out of date.
///
/// Re-blessed for the `logic:`-only validation migration (retiring hand-authored shapes):
/// `module.ttl`'s TBox restriction blocks for `lang:inSignSystem`, `lang:analysisLevel`,
/// `lang:featureKey`, `lang:denotationKind`, `lang:renderingKind`, `lang:translationCorrespondence`,
/// `lang:paraphraseSamenessKind`, `gmeow:gmnSecurityRing`, and `gmeow:gmnRingLevel` drop their
/// redundant hand-paired `maxQualifiedCardinality 1` (or, for `gmnSecurityRing`, both min/max)
/// restriction, replaced by an `owl:FunctionalProperty` typing (or, for `gmeow:gmnSecurityRing`
/// / `gmeow:gmnEnvelopeCorrespondence` / `gmeow:gmnRingLevel`, a new `logic:ClosureEntry` closed-world
/// record) that reasons the same exactly-one/exactly-one-typed obligation instead of restating it
/// as a shape-shaped restriction pair. None of this touches the GMN dictionary/glyph predicates
/// [`GmnDictionary::from_dataset`] and [`gmn1_read`] actually resolve (`gmeow:gmnDictV3` and the
/// glyph/prefix tables are untouched), so the codebook this wasm image validates against is
/// unchanged in every way [`gmn_validate`] observes — only the raw carrier bytes moved.
///
/// Re-blessed again when the eight "exactly one" upper bounds that migration had left
/// unenforced were restored. The `owl:FunctionalProperty` typing it introduced on
/// `lang:inSignSystem`, `lang:analysisLevel`, `lang:featureKey`, `lang:denotationKind`,
/// `lang:renderingKind`, `lang:translationCorrespondence` and `lang:paraphraseSamenessKind`
/// is an object-property characteristic OUTSIDE the EL profile the slice's own
/// `ex:saNoForbiddenCharacteristics` structural assertion protects, and the pipeline projects
/// no `sh:maxCount` from it — so the marker silently dropped the upper half of the obligation.
/// The seven markers are removed and, together with `gmeow:gmnSecurityRing`, each property now
/// carries a class-scoped `logic:Restriction` with `logic:maxQualifiedCardinality 1` and
/// `logic:onClass owl:Thing` (the `owl:Thing` qualifier is what degrades the qualified bound to
/// the BARE `sh:maxCount 1` the retired shapes had). The whole diff is those seven `a
/// owl:ObjectProperty , owl:FunctionalProperty` lines, eight added restriction bodies, and the
/// note that explains them: 31 lines added, 10 removed, no other subject touched. Once more it
/// leaves `gmeow:gmnDictV3`, `gmeow:gmnCodebookCurrent`, the `gmeow:references` inventory and
/// the grapheme/prefix tables — everything [`GmnDictionary::from_dataset`] and [`gmn1_read`]
/// resolve — untouched, so only the carrier bytes moved.
pub const GMN_CODEBOOK_DIGEST: &str =
    "7f261effd735fd847d78a3631daf26199ec34602c70c614422438532bb89e797";

/// The graph-derived dictionary, built ONCE from the embedded codebook and memoized.
///
/// The embedded codebook ([`GMN_CODEBOOK_TTL`]) is a build-time constant — the real authored
/// `module.ttl`, its exact bytes pinned by [`GMN_CODEBOOK_DIGEST`] and guarded by the
/// `gmn_codebook_digest_is_pinned` host test — so a parse/resolve failure here is a
/// build-integrity invariant violation, never a runtime condition. It therefore hard-fails
/// (a panic / wasm trap), never silently degrading to a syntax-only check; the return type is
/// infallible because the embedded carrier is known-good by construction.
fn embedded_dictionary() -> &'static GmnDictionary {
    static DICT: OnceLock<GmnDictionary> = OnceLock::new();
    DICT.get_or_init(|| {
        let dataset = purrdf::parse_dataset(GMN_CODEBOOK_TTL, "text/turtle", None)
            .expect("embedded GMN codebook module.ttl must parse (build-integrity invariant)");
        GmnDictionary::from_dataset(&dataset).expect(
            "embedded GMN codebook must resolve gmeow:gmnDictV3 (build-integrity invariant)",
        )
    })
}

/// The blake3 content digest of the embedded GMN-1 codebook (`module.ttl`), as lowercase
/// hex. Lets a JS caller pin the EXACT codebook their document was validated against — the
/// same content address the codec's codebook-digest layer and the `gmeow gmn digest` CLI
/// report over the carrier bytes.
#[wasm_bindgen]
pub fn gmn_codebook_digest() -> String {
    blake3::hash(GMN_CODEBOOK_TTL).to_hex().to_string()
}

/// Validate a GMN-1 document against the EMBEDDED codebook, returning a canonical JSON
/// verdict.
///
/// The `bytes` are the raw GMN-1 surface text (the `@gmn{…}` header plus one record per
/// line). They are read through [`gmn1_read`] — the production codec's reader — against the
/// dictionary/glyph registry resolved from the embedded [`GMN_CODEBOOK_TTL`]. Because the
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
/// failure of the EMBEDDED codebook (it fails to parse or to resolve `gmeow:gmnDictV3`) is not
/// a runtime condition — the codebook is a pinned build constant (see [`embedded_dictionary`])
/// — so it hard-fails as a panic / wasm trap, never a document defect and never a silent
/// degradation to a syntax-only check.
#[wasm_bindgen]
pub fn gmn_validate(bytes: &[u8]) -> Result<String, JsError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| JsError::new(&format!("GMN-1 document is not valid UTF-8: {e}")))?;
    let dict = embedded_dictionary();
    let verdict = match gmn1_read(&Gmn1Document::from_text(text), dict) {
        Ok(_model) => serde_json::json!({ "conformant": true }),
        Err(error) => serde_json::json!({
            "conformant": false,
            "failureClass": error.failure_class(),
            "detail": error.to_string(),
        }),
    };
    Ok(verdict.to_string())
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
// The validation logic is target-independent (the `#[wasm_bindgen]` fns compile natively
// too), so the load-bearing behaviour — accept a conformant vector, and REJECT a
// grammar-valid document that names an unbound term — is exercised on the host gate here.
// The wasm+JS boundary is separately exercised as real wasm by the Node round-trip lane.
#[cfg(test)]
mod gmn_tests {
    use super::*;

    /// A frozen positive conformance vector: a basic `@c{s p o}` claim over dictionary /
    /// prefix-covered terms.
    const FROZEN_POSITIVE: &[u8] =
        include_bytes!("../../../slices/grounding/lang/tests/gmn1-vectors/claim-basic.gmn");

    /// A frozen codec-tier negative: grammar-valid (`@c{s p o q}`, a known sigil, known
    /// keys, a well-formed number) but every term (`zx9`, `quuxes`, `gate1`) is UNCOVERED
    /// by the codebook. The recorded class is `lang:GmnUncoveredTerm`
    /// (`negative-codec/expected.ttl`).
    const FROZEN_UNKNOWN_GLYPH: &[u8] = include_bytes!(
        "../../../slices/grounding/lang/tests/gmn1-vectors/negative-codec/neg-uncovered-term.gmn"
    );

    fn parse(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("gmn_validate returns well-formed JSON")
    }

    #[test]
    fn gmn_wasm_accepts_a_frozen_vector() {
        let verdict =
            parse(&gmn_validate(FROZEN_POSITIVE).expect("valid UTF-8 + embedded codebook"));
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
        let verdict =
            parse(&gmn_validate(FROZEN_UNKNOWN_GLYPH).expect("valid UTF-8 + embedded codebook"));
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

    #[test]
    fn gmn_codebook_digest_is_pinned() {
        // The recorded `GMN_CODEBOOK_DIGEST` must equal the live blake3 of the embedded
        // carrier — a drift guard so the documented codebook identity can never go stale.
        assert_eq!(
            gmn_codebook_digest(),
            GMN_CODEBOOK_DIGEST,
            "the embedded module.ttl digest drifted from the pinned GMN_CODEBOOK_DIGEST — \
             update the constant (and any docs quoting it) to the new blake3"
        );
    }
}
