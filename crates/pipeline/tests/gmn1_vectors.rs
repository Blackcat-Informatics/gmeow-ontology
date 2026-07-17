// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The frozen GMN-1 conformance-vector corpus discharge harness.
//!
//! The corpus lives at `slices/grounding/lang/tests/gmn1-vectors/`. Its POSITIVE `.gmn`
//! outputs are DERIVED, never transcribed: each `<name>.in.ttl` is a small default-graph-only
//! GMN-0 model, and its frozen `<name>.gmn` is exactly what the production [`gmn1_write`]
//! codec emits over it with the REAL `gmeow:gmnDictV3` dictionary. This mirrors the
//! `check_gmn1_shipped_projections` recompute-and-byte-compare idiom
//! (`crates/pipeline/src/stages/gmn1_gate.rs`): reconstruct the document from source, assert
//! the produced text equals the frozen bytes byte-for-byte (a stale artifact is a HARD FAIL),
//! then read the reconstructed document back (its out-of-band reference table travels with it)
//! and prove the per-claim inversion + idempotence witnesses.
//!
//! To (re)freeze the outputs after a deliberate codec/corpus change, run this test with
//! `GMN1_VECTORS_BLESS=1` — it rewrites every `<name>.gmn` and `vector-manifest.ttl` from the current
//! codec, then the ordinary (unblessed) run byte-compares against them.
//!
//! # Tiers
//!
//! * POSITIVE vectors — every semantic construct the corpus must exercise, frozen + witnessed.
//! * CODEC-tier negatives (`negative-codec/`) — malformed `.gmn` read inputs (and one non-NFC
//!   `.in.ttl` write input) each raising EXACTLY its recorded [`Gmn1Error`] class through the
//!   production codec. Expectations live in `negative-codec/expected.ttl`.
//! * GRAPH-tier negatives (`negative-graph/`) — envelope TTL fixtures with a wrong/missing
//!   `gmeow:gmnCodebookDigest`, for the SHACL/native gate in a LATER task. Here they are only
//!   shape-asserted (no SHACL run is forced).
//!
//! The [`completeness_invariant_covers_every_required_construct`] test enumerates the required
//! coverage set (all thirteen sigils, triple term, reifier, by-reference annotation, and each
//! degenerate case) and HARD-FAILS if any is not exercised by ≥1 positive vector's ACTUAL codec
//! output — a falsifiable, no-existence-only invariant.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_lang_bridge::{
    Gmn0Model, Gmn1Document, GmnDictionary, codebook_digest, gmn0_canonically_equal, gmn1_read,
    gmn1_write, idempotence_check, per_claim_round_trip_check, resolve_current_codebook,
};
use purrdf::{RdfDataset, RdfTerm, parse_dataset};

// ── Well-known IRIs the discharge harness reads structurally ───────────────────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const ENFORCES_FAILURE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";
const GMN_CODEBOOK_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/gmnCodebookDigest";
const GMN_ENVELOPE: &str = "https://blackcatinformatics.ca/gmeow/GmnEnvelope";

/// The nineteen POSITIVE vectors, by `<name>` stem — the authoritative on-disk set the
/// discharge test cross-checks against the committed `<name>.in.ttl` files (a stray or missing
/// vector reds the corpus-shape check). Kept sorted for a stable manifest listing.
const POSITIVE_VECTORS: &[&str] = &[
    "all-slots",
    "by-ref-annotation",
    "by-ref-confidence",
    "claim-basic",
    "defeater",
    "err-repair",
    "evidence-span",
    "header-only",
    "lang-ast",
    "logic-record",
    "math-record",
    "modal-force",
    "patch-on-triple-term",
    "patch-repair",
    "proof",
    "reifier-folded",
    "retract-repair",
    "standpoint",
    "triple-term-object",
];

/// The thirteen GMN-1 sigils (ten semantic + three repair), the surface tokens the
/// completeness invariant proves are each produced by ≥1 positive vector's codec output.
const REQUIRED_SIGILS: &[&str] = &[
    "@c", "@e", "@s", "@p", "@π", "@d", "@m", "@μ", "@λ", "@ℒ", "@err", "@patch", "@retract",
];

// ── Paths + shared loaders ─────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root canonicalizes")
}

fn vectors_dir() -> PathBuf {
    repo_root().join("slices/grounding/lang/tests/gmn1-vectors")
}

fn lang_module_dataset() -> Arc<RdfDataset> {
    parse_ttl(&repo_root().join("slices/grounding/lang/module.ttl"))
}

fn parse_ttl(path: &Path) -> Arc<RdfDataset> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Lift a parsed RDF 1.2 dataset into the codec's GMN-0 quad model through the PRODUCTION
/// constructor — the exact call the GMN-1 gate uses. [`Gmn0Model::from_dataset`] itself
/// materializes purrdf's RDF 1.2 reifier and annotation side-tables (which live OUTSIDE
/// `owned_quads`, so a Turtle-authored `x rdf:reifies <<( s p o )>>` reaches the codec here
/// rather than being silently dropped). This wrapper exists only to name that
/// production-path intent at each call site; it holds no test-only materialization.
fn model_from_ttl(ds: &RdfDataset) -> Gmn0Model {
    Gmn0Model::from_dataset(ds)
}

/// The REAL shipped dictionary — `gmeow:gmnDictV3` loaded from the lang slice's `module.ttl`,
/// the same one the production gate and shipped-projection lint decode against.
fn dict() -> GmnDictionary {
    GmnDictionary::from_dataset(&lang_module_dataset()).expect("gmeow:gmnDictV3 loads")
}

/// Recompute the codebook's content-addressed Merkle digest from the live carrier — the value
/// the frozen corpus is pinned against in `vector-manifest.ttl`.
fn recomputed_codebook_digest() -> String {
    let ds = lang_module_dataset();
    let codebook = resolve_current_codebook(&ds).expect("current codebook resolves");
    codebook_digest(&codebook, &dict())
}

// ── (A) POSITIVE vectors: byte-frozen + witnessed ──────────────────────────────────────

/// Every positive `<name>.in.ttl` writes to a `Gmn1Document` whose text EQUALS the frozen
/// `<name>.gmn` bytes, then reads back canonically equal, and passes the per-claim inversion +
/// idempotence witnesses. A byte mismatch is a hard failure naming the re-freeze escape hatch.
#[test]
fn positive_vectors_freeze_byte_exact_and_round_trip() {
    let d = dict();
    let dir = vectors_dir();

    if bless_enabled() {
        bless_positive_outputs(&d, &dir);
    }

    // The on-disk `<name>.in.ttl` set is EXACTLY the declared positive vectors — a stray or
    // missing input reds here rather than silently skipping coverage.
    assert_eq!(
        on_disk_input_stems(&dir),
        POSITIVE_VECTORS.iter().map(|s| (*s).to_owned()).collect(),
        "the committed *.in.ttl inputs must equal the declared POSITIVE_VECTORS set"
    );

    for name in POSITIVE_VECTORS {
        let model = model_from_ttl(&parse_ttl(&dir.join(format!("{name}.in.ttl"))));
        let doc = gmn1_write(&model, &d)
            .unwrap_or_else(|e| panic!("vector {name}: gmn1_write over its input failed: {e}"));

        let frozen_path = dir.join(format!("{name}.gmn"));
        let frozen = std::fs::read(&frozen_path).unwrap_or_else(|e| {
            panic!(
                "vector {name}: frozen {} unreadable: {e}",
                frozen_path.display()
            )
        });
        assert_eq!(
            doc.text.as_bytes(),
            frozen.as_slice(),
            "vector {name}: the codec's current output differs from the frozen {name}.gmn \
             bytes (a stale/mismatched vector). Re-freeze deliberately with \
             GMN1_VECTORS_BLESS=1 after confirming the codec change is intended.\n\
             --- produced ---\n{}\n--- frozen ---\n{}",
            doc.text,
            String::from_utf8_lossy(&frozen),
        );

        // Read the RECONSTRUCTED document (carrying its out-of-band reference table) back and
        // prove the two mnemomorphic witnesses — not the frozen text alone, which holds no refs.
        let back = gmn1_read(&doc, &d)
            .unwrap_or_else(|e| panic!("vector {name}: gmn1_read of its own output failed: {e}"));
        assert!(
            gmn0_canonically_equal(&model, &back),
            "vector {name}: reconstructed model is not canonically equal to the source"
        );
        per_claim_round_trip_check(&model, &d)
            .unwrap_or_else(|e| panic!("vector {name}: per-claim inversion witness failed: {e}"));
        idempotence_check(&doc, &d)
            .unwrap_or_else(|e| panic!("vector {name}: idempotence witness failed: {e}"));
    }
}

/// The frozen corpus is pinned to the codebook it was derived against: `vector-manifest.ttl`'s
/// `gmeow:gmnCodebookDigest` must equal the digest recomputed from the live carrier. A codebook
/// change without a re-freeze reds here, catching a silently-stale corpus.
#[test]
fn manifest_pins_the_frozen_codebook_digest() {
    let dir = vectors_dir();
    if bless_enabled() {
        std::fs::write(
            dir.join("vector-manifest.ttl"),
            manifest_text(&recomputed_codebook_digest()),
        )
        .expect("write vector-manifest.ttl");
    }
    let manifest = parse_ttl(&dir.join("vector-manifest.ttl"));
    let pinned: Vec<String> = manifest
        .owned_quads()
        .filter(|q| q.predicate == GMN_CODEBOOK_DIGEST)
        .filter_map(|q| literal_lexical(&q.object))
        .collect();
    assert_eq!(
        pinned.len(),
        1,
        "vector-manifest.ttl must pin exactly one gmeow:gmnCodebookDigest, found {}",
        pinned.len()
    );
    assert_eq!(
        pinned[0],
        recomputed_codebook_digest(),
        "vector-manifest.ttl's pinned codebook digest is stale vs the live carrier — re-freeze the \
         corpus with GMN1_VECTORS_BLESS=1"
    );
}

// ── (B) CODEC-tier negatives: each raises exactly its recorded class ────────────────────

/// Each `negative-codec/` vector raises EXACTLY its recorded `lang:` failure class through the
/// production codec. A `.gmn` vector is a read-negative (`gmn1_read`); an `.in.ttl` vector is a
/// write-negative (`gmn1_write`) — the direction is read off the file extension. Expectations
/// come from `negative-codec/expected.ttl`, cross-checked against the on-disk files so neither
/// can drift.
#[test]
fn codec_tier_negatives_raise_their_recorded_class() {
    let d = dict();
    let neg_dir = vectors_dir().join("negative-codec");
    let expected = expected_negative_classes(&neg_dir);

    // The recorded expectation set is EXACTLY the on-disk negative vector files.
    assert_eq!(
        expected.keys().cloned().collect::<BTreeSet<_>>(),
        on_disk_negative_files(&neg_dir),
        "negative-codec/expected.ttl must name exactly the on-disk .gmn / .in.ttl negatives"
    );

    for (file, want_class) in &expected {
        let path = neg_dir.join(file);
        let err = if file.ends_with(".gmn") {
            let text = String::from_utf8(std::fs::read(&path).expect("read .gmn"))
                .expect("negative .gmn is UTF-8");
            gmn1_read(&Gmn1Document::from_text(text), &d)
                .expect_err(&format!("read-negative {file} must FAIL to decode"))
        } else {
            let model = model_from_ttl(&parse_ttl(&path));
            gmn1_write(&model, &d).expect_err(&format!("write-negative {file} must FAIL to encode"))
        };
        assert_eq!(
            local_name(err.failure_class()),
            *want_class,
            "negative {file} must raise lang:{want_class}, got {}",
            err.failure_class()
        );
    }
}

// ── (C) GRAPH-tier negatives: shape-asserted only (SHACL/native discharge is a later task) ──

/// The `negative-graph/` envelope fixtures are shape-asserted here (NOT driven through SHACL):
/// the mismatch fixture declares a `gmeow:gmnCodebookDigest` that differs from the recomputed
/// codebook digest, and the missing fixture declares a `gmeow:GmnEnvelope` with no digest field
/// at all. Their `lang:GmnCodebookDigestMismatch` / `lang:GmnMissingEnvelopeField` discharge is
/// the native/SHACL gate's job in a later task.
#[test]
fn graph_tier_negatives_have_the_declared_envelope_shape() {
    let dir = vectors_dir().join("negative-graph");
    let recomputed = recomputed_codebook_digest();

    let mismatch = envelope_digests(&parse_ttl(&dir.join("envelope-digest-mismatch.ttl")));
    assert_eq!(
        mismatch.len(),
        1,
        "the mismatch fixture must declare one GmnEnvelope digest, found {}",
        mismatch.len()
    );
    assert_ne!(
        mismatch[0], recomputed,
        "the digest-mismatch fixture must declare a codebook digest DIFFERENT from the real one \
         (else it is not a mismatch negative)"
    );

    let missing = envelope_digests(&parse_ttl(&dir.join("envelope-missing-digest.ttl")));
    assert!(
        missing.is_empty(),
        "the missing-digest fixture must declare a GmnEnvelope with NO gmnCodebookDigest, found {missing:?}"
    );
}

// ── (D) The runnable completeness invariant ────────────────────────────────────────────

/// Enumerate the required coverage set — all thirteen sigils, a triple term, a reifier, a
/// by-reference annotation, and each degenerate case — and HARD-FAIL if any is not exercised by
/// ≥1 positive vector's ACTUAL codec output. Features are derived from the produced `.gmn` text,
/// never from a vector's filename, so the invariant is falsifiable: drop `@e` from every vector
/// and the `@e` requirement reds.
#[test]
fn completeness_invariant_covers_every_required_construct() {
    let d = dict();
    let dir = vectors_dir();

    let required: BTreeSet<String> = required_coverage();
    let mut exercised: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in POSITIVE_VECTORS {
        let model = model_from_ttl(&parse_ttl(&dir.join(format!("{name}.in.ttl"))));
        let doc = gmn1_write(&model, &d).unwrap_or_else(|e| panic!("vector {name}: write: {e}"));
        for feature in features_of(&doc.text) {
            exercised
                .entry(feature)
                .or_default()
                .push((*name).to_owned());
        }
    }

    // Every required construct is exercised by at least one vector (no existence-only rows).
    let missing: Vec<&String> = required
        .iter()
        .filter(|feat| !exercised.contains_key(*feat))
        .collect();
    assert!(
        missing.is_empty(),
        "the frozen corpus leaves required GMN-1 constructs unexercised by any positive vector: \
         {missing:?}\nexercised map: {exercised:#?}"
    );

    // Falsifiability floor: the required set is non-trivial (13 sigils + 4 structural + 4
    // degenerate), so a corpus that quietly stopped covering a construct cannot pass vacuously.
    assert_eq!(
        required.len(),
        REQUIRED_SIGILS.len() + 3 + 4,
        "the required coverage set must be the thirteen sigils plus triple-term/reifier/\
         by-ref-annotation plus the four degenerate cases"
    );
}

// ── Feature detection over produced GMN-1 text ─────────────────────────────────────────

/// The required coverage set: `sigil:<glyph>` for each of the thirteen sigils, the three
/// structural constructs, and the four degenerate cases.
fn required_coverage() -> BTreeSet<String> {
    let mut set: BTreeSet<String> = REQUIRED_SIGILS
        .iter()
        .map(|s| format!("sigil:{s}"))
        .collect();
    set.insert("triple-term".to_owned());
    set.insert("reifier".to_owned());
    set.insert("by-ref-annotation".to_owned());
    set.insert("degenerate:header-only".to_owned());
    set.insert("degenerate:all-optional-slots".to_owned());
    set.insert("degenerate:by-ref-literal".to_owned());
    set.insert("degenerate:patch-on-triple-term".to_owned());
    set
}

/// The coverage features one produced GMN-1 document exercises, derived purely from its text.
fn features_of(text: &str) -> BTreeSet<String> {
    let record_lines: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with('@') && !l.starts_with("@gmn{"))
        .collect();

    let mut out = BTreeSet::new();

    // Sigils: the leading `@<sigil>{` token of each record line.
    for line in &record_lines {
        if let Some(sigil) = line.split('{').next() {
            out.insert(format!("sigil:{sigil}"));
        }
    }

    // Structural constructs.
    if text.contains('(') {
        // A `(` only ever appears in the compact `( s p o )` triple-term object surface — no
        // ordinary GMN-1 identifier/number token may contain a parenthesis.
        out.insert("triple-term".to_owned());
    }
    if text.contains("rdf__reifies") {
        out.insert("reifier".to_owned());
    }
    if text.contains("v: r_") {
        out.insert("by-ref-annotation".to_owned());
    }

    // Degenerate cases.
    if record_lines.is_empty() {
        out.insert("degenerate:header-only".to_owned());
    }
    if record_lines.iter().any(|l| {
        ["q: ", "st: ", "ev: ", "m: ", "ek: ", "bd: ", "it: "]
            .iter()
            .all(|slot| l.contains(slot))
    }) {
        out.insert("degenerate:all-optional-slots".to_owned());
    }
    if text.contains("q: r_") {
        out.insert("degenerate:by-ref-literal".to_owned());
    }
    if record_lines.iter().any(|l| l.starts_with("@patch{")) && text.contains('(') {
        out.insert("degenerate:patch-on-triple-term".to_owned());
    }

    out
}

// ── Small structural helpers ───────────────────────────────────────────────────────────

fn bless_enabled() -> bool {
    std::env::var_os("GMN1_VECTORS_BLESS").is_some()
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned()
}

fn literal_lexical(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
        _ => None,
    }
}

/// The `<stem>` of every `*.in.ttl` in the corpus root (the positive inputs).
fn on_disk_input_stems(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(dir).expect("read vectors dir") {
        let name = entry.expect("dir entry").file_name();
        let name = name.to_str().expect("utf-8 filename");
        if let Some(stem) = name.strip_suffix(".in.ttl") {
            out.insert(stem.to_owned());
        }
    }
    out
}

/// Every codec-tier negative vector file (`*.gmn` read-negatives and `*.in.ttl` write-negatives)
/// in `negative-codec/`, excluding the `expected.ttl` sidecar.
fn on_disk_negative_files(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(dir).expect("read negative-codec dir") {
        let name = entry.expect("dir entry").file_name();
        let name = name.to_str().expect("utf-8 filename").to_owned();
        if name.ends_with(".gmn") || name.ends_with(".in.ttl") {
            out.insert(name);
        }
    }
    out
}

/// Parse `negative-codec/expected.ttl` into a `filename -> failure-class-local-name` map.
fn expected_negative_classes(neg_dir: &Path) -> BTreeMap<String, String> {
    let ds = parse_ttl(&neg_dir.join("expected.ttl"));
    let mut label: BTreeMap<String, String> = BTreeMap::new();
    let mut class: BTreeMap<String, String> = BTreeMap::new();
    for q in ds.owned_quads() {
        let RdfTerm::BlankNode(subj) = &q.subject else {
            continue;
        };
        match q.predicate.as_str() {
            RDFS_LABEL => {
                if let Some(lex) = literal_lexical(&q.object) {
                    label.insert(subj.clone(), lex);
                }
            }
            ENFORCES_FAILURE_CLASS => {
                if let RdfTerm::Iri(iri) = &q.object {
                    class.insert(subj.clone(), local_name(iri));
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeMap::new();
    for (subj, file) in label {
        let cls = class
            .get(&subj)
            .unwrap_or_else(|| panic!("expected.ttl entry {file} has no enforcesFailureClass"));
        out.insert(file, cls.clone());
    }
    out
}

/// The `gmeow:gmnCodebookDigest` lexical values declared on any `gmeow:GmnEnvelope` subject.
fn envelope_digests(ds: &RdfDataset) -> Vec<String> {
    let envelopes: BTreeSet<String> = ds
        .owned_quads()
        .filter(|q| q.predicate == RDF_TYPE)
        .filter_map(|q| match (&q.subject, &q.object) {
            (RdfTerm::Iri(s), RdfTerm::Iri(o)) if o == GMN_ENVELOPE => Some(s.clone()),
            _ => None,
        })
        .collect();
    ds.owned_quads()
        .filter(|q| q.predicate == GMN_CODEBOOK_DIGEST)
        .filter(|q| matches!(&q.subject, RdfTerm::Iri(s) if envelopes.contains(s)))
        .filter_map(|q| literal_lexical(&q.object))
        .collect()
}

// ── The freeze (bless) path ────────────────────────────────────────────────────────────

/// Rewrite every `<name>.gmn` from the current codec output over its `<name>.in.ttl` input.
/// Only runs under `GMN1_VECTORS_BLESS`.
fn bless_positive_outputs(d: &GmnDictionary, dir: &Path) {
    for name in POSITIVE_VECTORS {
        let model = model_from_ttl(&parse_ttl(&dir.join(format!("{name}.in.ttl"))));
        let doc = gmn1_write(&model, d)
            .unwrap_or_else(|e| panic!("bless vector {name}: gmn1_write failed: {e}"));
        std::fs::write(dir.join(format!("{name}.gmn")), doc.text.as_bytes())
            .unwrap_or_else(|e| panic!("bless vector {name}: write .gmn failed: {e}"));
    }
}

/// The generated `vector-manifest.ttl`: pins the codebook digest the corpus was frozen against and
/// lists every positive vector. Regenerated under `GMN1_VECTORS_BLESS`.
fn manifest_text(digest: &str) -> String {
    let mut s = String::new();
    s.push_str(
        "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>\n\
         # SPDX-License-Identifier: CC-BY-4.0\n\
         # GENERATED by crates/pipeline/tests/gmn1_vectors.rs (GMN1_VECTORS_BLESS=1); do not hand-edit.\n\
         # The frozen GMN-1 conformance-vector corpus: its <name>.gmn outputs are DERIVED from the\n\
         # <name>.in.ttl inputs by the production codec against the codebook whose recomputed\n\
         # Merkle root is pinned below as gmeow:gmnCodebookDigest.\n\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .\n\
         @prefix vec:   <https://blackcatinformatics.ca/gmeow/tests/gmn1-vectors/> .\n\n",
    );
    s.push_str("vec:corpus a gmeow:GmnConformancePack ;\n");
    s.push_str("    rdfs:label \"Frozen GMN-1 conformance-vector corpus\"@x-gmeow-english ;\n");
    s.push_str("    gmeow:references gmeow:gmnCodebookCurrent ;\n");
    s.push_str(&format!(
        "    gmeow:gmnCodebookDigest \"{digest}\"^^xsd:string ;\n"
    ));
    let refs = POSITIVE_VECTORS
        .iter()
        .map(|name| format!("vec:{name}"))
        .collect::<Vec<_>>()
        .join(" ,\n        ");
    s.push_str(&format!("    rdfs:seeAlso {refs} .\n\n"));
    for name in POSITIVE_VECTORS {
        s.push_str(&format!(
            "vec:{name} rdfs:label \"{name}.in.ttl -> {name}.gmn\"^^xsd:string .\n"
        ));
    }
    s
}
