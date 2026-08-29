// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gmeow gmn` conformance surface — the shipped, checkout-free twin of the
//! GMN-1 production gates and authenticated frozen-corpus consumers.
//!
//! The digest / codec / witness / pack layer built in the codec crate is otherwise
//! reachable only from `crates/pipeline`'s own gates and tests. This module exposes
//! it on the shipped `gmeow` binary so an INDEPENDENT GMN-1 implementation can
//! conform AGAINST the CLI: `gmn digest` prints the codebook Merkle root and an
//! input's content digest, `gmn encode`/`gmn decode` are the two codec legs, and
//! `gmn verify` is the conformance driver over a frozen vector corpus. Every
//! subcommand HARD-FAILS (non-zero exit + a typed diagnostic) on any mismatch —
//! never a soft warning (no-optionality).
//!
//! The GMN codebook + dictionary + grammar leaf + pack identity are resolved from the
//! EMBEDDED `gmeow.gts` bundle ([`crate::BUNDLE_GTS`]) — the same authored
//! `gmeow:gmnCodebookCurrent` / `gmeow:gmnDictV3` the production gate loads, folded
//! into the shipped snapshot — so `digest` / `encode` / `decode` run from the installed
//! binary with NO repository checkout (`crates/gmeow-cli` coding guideline). The
//! `--lang-module` / `--grammar` / `--pack` flags override the embedded defaults with
//! on-disk files. `verify` additionally needs the frozen vector corpus, which is a TEST
//! artifact (not shipped in the bundle): it defaults to the in-repo corpus when present
//! and otherwise REQUIRES `--vectors` pointing at a conformer's own corpus — an explicit
//! input, never a silent skip.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use gmeow_cli_core::Reporter;
use gmeow_lang_bridge::{
    CurrentCodebook, EcosystemLeaves, Gmn0Model, Gmn1Document, GmnDictionary, GmnMigration,
    RingLattice, codebook_digest, consume_project, content_digest, derive_target_inventory,
    extract_operators, gmn0_canonically_equal, gmn1_read, gmn1_write, grammar_leaf,
    header_schema_major, idempotence_check, pack_root_from_grammar_leaf,
    per_claim_round_trip_check, reemit_migrated_document, resolve_current_codebook,
    source_operator_table,
};
use purrdf::{RdfDataset, RdfTerm};

use crate::commands::{emit_error, fail};

/// The frozen GMN-1 conformance-vector corpus root, as committed IN THE REPO. Used as the
/// `verify` default when a source checkout is present; an installed binary outside a checkout
/// finds no such directory and REQUIRES `--vectors` (the corpus is a test artifact, never shipped
/// in the bundle).
const DEFAULT_VECTORS: &str = "slices/grounding/lang/tests/gmn1-vectors";

const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const GMN_PACK_CURRENT: &str = "https://blackcatinformatics.ca/gmeow/gmnPackCurrent";
const GMN_PACK_ROOT: &str = "https://blackcatinformatics.ca/gmeow/gmnPackRoot";
const GMN_CODEBOOK_CURRENT: &str = "https://blackcatinformatics.ca/gmeow/gmnCodebookCurrent";
const GMN_CODEBOOK_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/gmnCodebookDigest";
const GMN_GRAMMAR: &str = "https://blackcatinformatics.ca/gmeow/gmnGrammar";
const GMN_GRAMMAR_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/gmnGrammarDigest";
const ENFORCES_FAILURE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";
// The four ecosystem-view Merkle-leaf `<subject, predicate>` pairs the pack pins into the bundle,
// so `gmn verify` recomputes the whole-ecosystem pack root from the bundle alone.
const GMN_GBNF: &str = "https://blackcatinformatics.ca/gmeow/gmnGbnf";
const GMN_GBNF_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/gmnGbnfDigest";
const GMN_LARK: &str = "https://blackcatinformatics.ca/gmeow/gmnLark";
const GMN_LARK_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/gmnLarkDigest";
const GMN_TOKEN_METRICS: &str = "https://blackcatinformatics.ca/gmeow/gmnTokenMetricsCurrent";
const GMN_TOKEN_METRICS_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/gmnTokenMetricsDigest";
const GMN_VERBALIZATIONS: &str = "https://blackcatinformatics.ca/gmeow/gmnVerbalizationsCurrent";
const GMN_VERBALIZATIONS_DIGEST: &str =
    "https://blackcatinformatics.ca/gmeow/gmnVerbalizationsDigest";

/// The `gmn digest` output serialization.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum DigestFormat {
    /// Two labeled, greppable lines (the default).
    #[default]
    Text,
    /// A single JSON object `{ "codebook_digest": …, "content_digest": … }`.
    Json,
}

/// The `gmn decode` output serialization for the reconstructed GMN-0 model.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum DecodeFormat {
    /// RDFC-1.0 canonical N-Quads (the default — the same form the content digest
    /// is taken over).
    #[default]
    Nquads,
    /// Turtle (default graph).
    Turtle,
}

// ── shared resolvers ───────────────────────────────────────────────────────────

/// Fold the embedded `gmeow.gts` snapshot into its RDF dataset — the checkout-free source of the
/// authored `gmeow:gmnCodebookCurrent` / `gmeow:gmnDictV3` / `gmeow:gmnGrammarDigest` /
/// `gmeow:gmnPackRoot`. A load failure is a HARD fail (exit 1), never a degraded default.
fn bundle_dataset(reporter: &dyn Reporter) -> Result<Arc<RdfDataset>, i32> {
    gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(crate::BUNDLE_GTS)
        .and_then(|bundle| bundle.dataset())
        .map_err(|e| {
            fail(
                reporter,
                "gmeow-cli.gmn.bundle",
                format!("cannot fold the embedded gmeow.gts snapshot: {e}"),
            )
        })
}

/// Parse a `--lang-module` override TTL file into its dataset. A read / parse failure is a HARD
/// fail (exit 1).
fn file_dataset(reporter: &dyn Reporter, path: &Path) -> Result<Arc<RdfDataset>, i32> {
    let bytes = std::fs::read(path).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.lang-module-read",
            format!("cannot read {}: {e}", path.display()),
        )
    })?;
    purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.lang-module-parse",
            format!("cannot parse {} as Turtle: {e}", path.display()),
        )
    })
}

/// Resolve the current codebook + the `gmeow:gmnDictV3` dictionary — the exact pair the GMN-1 gate
/// loads — from the EMBEDDED bundle by default, or from an explicit `--lang-module` override file.
/// A resolution failure is a HARD fail (exit 1), never a degraded default.
fn load_codebook_and_dict(
    reporter: &dyn Reporter,
    lang_module: Option<&Path>,
) -> Result<(CurrentCodebook, GmnDictionary), i32> {
    let ds = match lang_module {
        Some(path) => file_dataset(reporter, path)?,
        None => bundle_dataset(reporter)?,
    };
    let codebook = resolve_current_codebook(&ds).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.codebook",
            format!("cannot resolve gmeow:gmnCodebookCurrent: {}", e.0),
        )
    })?;
    let dict = GmnDictionary::from_dataset(&ds).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.dictionary",
            format!("cannot load gmeow:gmnDictV3: {}", e.0),
        )
    })?;
    Ok((codebook, dict))
}

/// Parse a user-supplied RDF file as Turtle into the codec's GMN-0 quad model
/// (`Gmn0Model::from_dataset` materializes purrdf's RDF-1.2 reifier/annotation side
/// tables, so a triple-term or by-reference annotation reaches the codec).
fn model_from_ttl(reporter: &dyn Reporter, input: &Path) -> Result<Gmn0Model, i32> {
    let bytes = std::fs::read(input).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.input-read",
            format!("cannot read {}: {e}", input.display()),
        )
    })?;
    let ds = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.input-parse",
            format!("cannot parse {} as Turtle: {e}", input.display()),
        )
    })?;
    Ok(Gmn0Model::from_dataset(&ds))
}

// ── gmn digest ─────────────────────────────────────────────────────────────────

/// `gmeow gmn digest <input.ttl>` — print BOTH the codebook Merkle root
/// (`codebook_digest`, a `blake3:…` value) and the input's content digest
/// (`content_digest`, `blake3:…` over its RDFC-1.0 canonical N-Quads).
pub fn digest(
    reporter: &dyn Reporter,
    input: &Path,
    lang_module: Option<&Path>,
    format: DigestFormat,
) -> i32 {
    let (codebook, dict) = match load_codebook_and_dict(reporter, lang_module) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let model = match model_from_ttl(reporter, input) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let cb = codebook_digest(&codebook, &dict);
    let cd = content_digest(&model);
    match format {
        DigestFormat::Text => {
            println!("codebook_digest {cb}");
            println!("content_digest {cd}");
        }
        DigestFormat::Json => {
            println!(
                "{}",
                serde_json::json!({ "codebook_digest": cb, "content_digest": cd })
            );
        }
    }
    0
}

// ── gmn encode ─────────────────────────────────────────────────────────────────

/// `gmeow gmn encode <input.ttl>` — parse → `Gmn0Model` → `gmn1_write` against the
/// codebook dictionary → print the GMN-1 document text to stdout. An uncovered /
/// out-of-domain construct HARD-FAILS with the typed `Gmn1Error` (exit 1).
pub fn encode(reporter: &dyn Reporter, input: &Path, lang_module: Option<&Path>) -> i32 {
    let (_codebook, dict) = match load_codebook_and_dict(reporter, lang_module) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let model = match model_from_ttl(reporter, input) {
        Ok(m) => m,
        Err(code) => return code,
    };
    match gmn1_write(&model, &dict) {
        Ok(doc) => {
            print!("{}", doc.text);
            0
        }
        Err(e) => fail(
            reporter,
            "gmeow-cli.gmn.encode",
            format!("cannot encode {} to GMN-1: {e}", input.display()),
        ),
    }
}

// ── gmn decode ─────────────────────────────────────────────────────────────────

/// `gmeow gmn decode <input.gmn>` — read GMN-1 text → `gmn1_read` → serialize the
/// reconstructed `Gmn0Model` back to canonical N-Quads (or Turtle) on stdout.
///
/// The reader is presented raw text via [`Gmn1Document::from_text`] (an EMPTY
/// out-of-band reference table): a document that names an unresolvable
/// `r_<hash>` by-reference token HARD-FAILS as `lang:GmnUncoveredTerm` — never a
/// silent drop.
pub fn decode(
    reporter: &dyn Reporter,
    input: &Path,
    lang_module: Option<&Path>,
    format: DecodeFormat,
) -> i32 {
    let (_codebook, dict) = match load_codebook_and_dict(reporter, lang_module) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let text = match std::fs::read_to_string(input) {
        Ok(t) => t,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.gmn.input-read",
                format!("cannot read {}: {e}", input.display()),
            );
        }
    };
    let doc = Gmn1Document::from_text(text);
    let model = match gmn1_read(&doc, &dict) {
        Ok(m) => m,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.gmn.decode",
                format!("cannot decode {} from GMN-1: {e}", input.display()),
            );
        }
    };
    match format {
        DecodeFormat::Nquads => {
            print!("{}", model.canonical_nquads());
            0
        }
        DecodeFormat::Turtle => {
            let dataset = model.to_dataset();
            // GMN-0's domain is default-graph-only (a named-graph quad hard-fails
            // `lang:GmnGraphOutOfDomain` at encode), so a decoded model never carries a named
            // graph. Assert that invariant rather than let `SerializeGraph::DefaultGraph` silently
            // drop one if it ever did — a Turtle rendering must not lose a quad.
            if dataset.owned_quads().any(|q| q.graph_name.is_some()) {
                return fail(
                    reporter,
                    "gmeow-cli.gmn.decode-serialize",
                    "decoded model carries a named-graph quad, which GMN-0's default-graph domain \
                     forbids; refusing to serialize a Turtle rendering that would drop it"
                        .to_owned(),
                );
            }
            match purrdf::serialize_dataset(
                &dataset,
                "text/turtle",
                purrdf::SerializeGraph::DefaultGraph,
            ) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => {
                        print!("{s}");
                        0
                    }
                    Err(e) => fail(
                        reporter,
                        "gmeow-cli.gmn.decode-serialize",
                        format!("decoded Turtle is not UTF-8: {e}"),
                    ),
                },
                Err(e) => fail(
                    reporter,
                    "gmeow-cli.gmn.decode-serialize",
                    format!("cannot serialize decoded model to Turtle: {e}"),
                ),
            }
        }
    }
}

// ── gmn project ──────────────────────────────────────────────────────────────────

/// The `gmeow:` namespace base — a bare `--ring` token is resolved under it.
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// Resolve a `--ring` argument to a full ring IRI: a `http(s)://…` value is used verbatim,
/// otherwise the token is treated as a `gmeow:` local name (e.g. `gmnRingCore`).
fn resolve_ring_arg(ring: &str) -> String {
    if ring.starts_with("http://") || ring.starts_with("https://") {
        ring.to_owned()
    } else {
        format!("{GMEOW_NS}{ring}")
    }
}

/// `gmeow gmn project --ring <ring> [--budget N] <input.ttl>` — the consume-path
/// security-ring filter on the shipped surface: canonicalize → SECURITY-RING FILTER → GMN-1 →
/// token-budget fit. The ring-tagged input dataset (each content subject carrying a
/// `gmeow:gmnSecurityRing`) is filtered to exactly the content ADMISSIBLE into `<ring>` under
/// the DERIVED `gmeow:gmnRingWithin` product-order (computed checkout-free from the bundle's
/// authored ring coordinates), projected to GMN-1 on stdout, and fitted to `--budget` tokens
/// with the elided remainder DISCLOSED on stderr — never a silent truncation.
///
/// HARD-FAILS (non-zero) on a leakage violation (`lang:GmnRingLeak`): unclassified content, an
/// admitted claim referencing out-of-ring content, or entangled shared structure — the
/// serializer refuses to emit a projection that would leak the boundary. An unresolvable target
/// ring hard-fails `lang:GmnRingLatticeMalformed`.
pub fn project(
    reporter: &dyn Reporter,
    input: &Path,
    ring: &str,
    budget: Option<u64>,
    lang_module: Option<&Path>,
) -> i32 {
    let (_codebook, dict) = match load_codebook_and_dict(reporter, lang_module) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    // The ring lattice coordinates are resolved from the SAME source as the codebook/dictionary:
    // the embedded bundle by default, or the `--lang-module` override.
    let lattice_ds = match lang_module {
        Some(path) => match file_dataset(reporter, path) {
            Ok(ds) => ds,
            Err(code) => return code,
        },
        None => match bundle_dataset(reporter) {
            Ok(ds) => ds,
            Err(code) => return code,
        },
    };
    let lattice = RingLattice::from_dataset(&lattice_ds);
    let target = resolve_ring_arg(ring);
    if !lattice.contains(&target) {
        return fail(
            reporter,
            "gmeow-cli.gmn.project.unknown-ring",
            format!(
                "target ring <{target}> is not a resolvable gmeow:GmnSecurityRing in the lattice \
                 (the shipped rings are gmeow:gmnRingCore / gmnRingTrusted / gmnRingRestricted / \
                 gmnRingNato); pass a full IRI or a gmeow: local name via --ring"
            ),
        );
    }

    let model = match model_from_ttl(reporter, input) {
        Ok(m) => m,
        Err(code) => return code,
    };

    match consume_project(&model, &lattice, &target, budget, &dict) {
        Ok(projection) => {
            // stdout carries ONLY the ring-filtered, budget-fit GMN-1 payload (pipeable to
            // `gmn decode`); the admission summary and any elision disclosure go to stderr.
            print!("{}", projection.text);
            eprintln!(
                "gmn project: target <{}> admitted {}/{} claims, excluded {}, emitted {} \
                 ({} tokens)",
                projection.target,
                projection.admitted_claims,
                projection.input_claims,
                projection.excluded_claims,
                projection.emitted_claims,
                projection.tokens,
            );
            if let Some(disclosure) = &projection.disclosure {
                eprintln!("gmn project: {disclosure}");
            }
            0
        }
        Err(e) => fail(
            reporter,
            "gmeow-cli.gmn.project.leak",
            format!(
                "cannot project {} into ring <{target}>: {e}",
                input.display()
            ),
        ),
    }
}

// ── gmn verify ─────────────────────────────────────────────────────────────────

/// `gmeow gmn verify` — the conformance driver an external GMN-1 implementation runs against the
/// shipped CLI. Over a frozen vector corpus it proves, for every POSITIVE vector, that `gmn1_write`
/// reproduces the frozen `.gmn` byte-for-byte and that the reconstructed document reads back
/// canonically equal AND passes the per-claim inversion + idempotence witnesses; for every
/// CODEC-tier negative, that the recorded `lang:` failure class is raised; and that the recomputed
/// codebook digest + whole-ecosystem `pack_root` (from the embedded bundle's codebook, dictionary,
/// grammar leaf, AND the four pinned ecosystem-view leaves — gbnf / lark / token-metrics /
/// verbalizations) equal what the corpus manifest and the shipped bundle declare — so a perturbation
/// of ANY ecosystem surface reds verify. Any failure prints a diagnostic and EXITS NON-ZERO.
///
/// The codebook / dictionary / grammar leaf / pack root come from the EMBEDDED bundle (checkout-free),
/// overridable by `--lang-module` / `--grammar` / `--pack`. The corpus is a test artifact (never in
/// the bundle): `--vectors` defaults to the in-repo corpus and is REQUIRED when that path is absent
/// (an installed binary outside a checkout) — an explicit input, never a silent skip. The corpus
/// must be COMPLETE: its `vector-manifest.ttl` and `negative-codec/expected.ttl` are mandatory, and
/// the discovered positive set must equal the manifest inventory exactly, so a truncated corpus
/// cannot pass.
pub fn verify(
    reporter: &dyn Reporter,
    vectors: Option<&Path>,
    lang_module: Option<&Path>,
    grammar: Option<&Path>,
    pack: Option<&Path>,
) -> i32 {
    // The corpus is a required input: use --vectors, else the in-repo default IF it exists.
    let default_vectors = Path::new(DEFAULT_VECTORS);
    let vectors = match vectors {
        Some(v) => v,
        None if default_vectors.is_dir() => default_vectors,
        None => {
            return fail(
                reporter,
                "gmeow-cli.gmn.verify.no-corpus",
                format!(
                    "no vector corpus: pass --vectors <dir> (the frozen corpus is a test artifact, \
                     not shipped in the bundle; the in-repo default {DEFAULT_VECTORS} is absent \
                     outside a source checkout)"
                ),
            );
        }
    };

    let (codebook, dict) = match load_codebook_and_dict(reporter, lang_module) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    let mut failures: Vec<String> = Vec::new();
    let mut positives = 0usize;
    let mut negatives = 0usize;

    // ── (A) positive vectors: byte-frozen + witnessed, and COMPLETE vs the manifest ──
    let manifest = vectors.join("vector-manifest.ttl");
    let manifest_stems = match manifest_inventory(reporter, &manifest) {
        Ok(stems) => stems,
        Err(code) => return code,
    };
    let disk_stems = match positive_stems(reporter, vectors) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let disk_set: BTreeSet<String> = disk_stems.iter().cloned().collect();
    if disk_set != manifest_stems {
        let missing_on_disk: Vec<&String> = manifest_stems.difference(&disk_set).collect();
        let missing_in_manifest: Vec<&String> = disk_set.difference(&manifest_stems).collect();
        return fail(
            reporter,
            "gmeow-cli.gmn.verify.incomplete-corpus",
            format!(
                "corpus is not exactly the manifest inventory: declared-but-absent {missing_on_disk:?}, \
                 present-but-unlisted {missing_in_manifest:?} — a conformance run must exercise the \
                 whole frozen corpus, never a subset"
            ),
        );
    }
    for stem in &disk_stems {
        let in_ttl = vectors.join(format!("{stem}.in.ttl"));
        let model = match model_from_ttl(reporter, &in_ttl) {
            Ok(m) => m,
            Err(code) => return code,
        };
        let doc = match gmn1_write(&model, &dict) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("positive {stem}: gmn1_write failed: {e}"));
                continue;
            }
        };
        let frozen_path = vectors.join(format!("{stem}.gmn"));
        let frozen = match std::fs::read(&frozen_path) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!(
                    "positive {stem}: frozen {} unreadable: {e}",
                    frozen_path.display()
                ));
                continue;
            }
        };
        if doc.text.as_bytes() != frozen.as_slice() {
            failures.push(format!(
                "positive {stem}: gmn1_write output differs from frozen {stem}.gmn (byte mismatch)"
            ));
            continue;
        }
        let back = match gmn1_read(&doc, &dict) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!(
                    "positive {stem}: gmn1_read of its own output failed: {e}"
                ));
                continue;
            }
        };
        if !gmn0_canonically_equal(&model, &back) {
            failures.push(format!(
                "positive {stem}: reconstructed model is not canonically equal to the source"
            ));
            continue;
        }
        if let Err(e) = per_claim_round_trip_check(&model, &dict) {
            failures.push(format!(
                "positive {stem}: per-claim inversion witness failed: {e}"
            ));
            continue;
        }
        if let Err(e) = idempotence_check(&doc, &dict) {
            failures.push(format!("positive {stem}: idempotence witness failed: {e}"));
            continue;
        }
        positives += 1;
    }

    // ── (B) codec-tier negatives: REQUIRED, each raises exactly its recorded class ──
    let neg_dir = vectors.join("negative-codec");
    let expected = match expected_negative_classes(reporter, &neg_dir) {
        Ok(e) => e,
        Err(code) => return code,
    };
    for (file, want_class) in &expected {
        let path = neg_dir.join(file);
        let got = if file.ends_with(".gmn") {
            match std::fs::read_to_string(&path) {
                Ok(text) => gmn1_read(&Gmn1Document::from_text(text), &dict).err(),
                Err(e) => {
                    failures.push(format!("negative {file}: unreadable: {e}"));
                    continue;
                }
            }
        } else {
            match model_from_ttl(reporter, &path) {
                Ok(model) => gmn1_write(&model, &dict).err(),
                Err(code) => return code,
            }
        };
        match got {
            None => failures.push(format!(
                "negative {file}: expected lang:{want_class} but the codec ACCEPTED it"
            )),
            Some(err) => {
                let got_class = local_name(err.failure_class());
                if &got_class == want_class {
                    negatives += 1;
                } else {
                    failures.push(format!(
                        "negative {file}: expected lang:{want_class}, got lang:{got_class}"
                    ));
                }
            }
        }
    }

    // ── (C) codebook digest: recompute, assert the corpus manifest AND the bundle agree ──
    let recomputed_digest = codebook_digest(&codebook, &dict);
    match pinned_literal(reporter, &manifest, GMN_CODEBOOK_DIGEST) {
        Ok(Some(pinned)) if pinned == recomputed_digest => {}
        Ok(Some(pinned)) => failures.push(format!(
            "vector-manifest.ttl pins codebook digest {pinned} but the live carrier recomputes to {recomputed_digest}"
        )),
        Ok(None) => {
            failures.push("vector-manifest.ttl declares no gmeow:gmnCodebookDigest".to_owned())
        }
        Err(code) => return code,
    }

    // ── (D) grammar leaf + pack root: recompute from the bundle (or overrides), assert declared ──
    let bundle = match bundle_dataset(reporter) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let grammar_leaf_hex = match grammar {
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => grammar_leaf(&bytes),
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.gmn.verify.grammar",
                    format!("cannot read GMN grammar {}: {e}", path.display()),
                );
            }
        },
        None => match pinned_literal_of(reporter, &bundle, GMN_GRAMMAR, GMN_GRAMMAR_DIGEST) {
            Ok(leaf) => leaf,
            Err(code) => return code,
        },
    };
    // Cross-check the bundle's declared codebook digest against the recomputation too.
    match pinned_literal_of(reporter, &bundle, GMN_CODEBOOK_CURRENT, GMN_CODEBOOK_DIGEST) {
        Ok(declared) if declared == recomputed_digest => {}
        Ok(declared) => failures.push(format!(
            "bundle declares codebook digest {declared} but the live carrier recomputes to {recomputed_digest}"
        )),
        Err(code) => return code,
    }
    // The four ecosystem-view Merkle leaves come from the bundle's pinned digests (gbnf / lark /
    // token-metrics / verbalizations), exactly as the grammar leaf does — so the recomputed pack
    // root certifies the WHOLE GMN ecosystem from the bundle alone. A missing pinned leaf is a
    // CONFORMANCE failure (accumulated, non-zero exit — never a silent pass), and a perturbed leaf
    // makes the recomputed root disagree with the pinned root, so verify reds.
    let ecosystem = EcosystemLeaves {
        gbnf: match bundle_view_leaf(reporter, &bundle, GMN_GBNF, GMN_GBNF_DIGEST, &mut failures) {
            Ok(leaf) => leaf,
            Err(code) => return code,
        },
        lark: match bundle_view_leaf(reporter, &bundle, GMN_LARK, GMN_LARK_DIGEST, &mut failures) {
            Ok(leaf) => leaf,
            Err(code) => return code,
        },
        token_metrics: match bundle_view_leaf(
            reporter,
            &bundle,
            GMN_TOKEN_METRICS,
            GMN_TOKEN_METRICS_DIGEST,
            &mut failures,
        ) {
            Ok(leaf) => leaf,
            Err(code) => return code,
        },
        verbalizations: match bundle_view_leaf(
            reporter,
            &bundle,
            GMN_VERBALIZATIONS,
            GMN_VERBALIZATIONS_DIGEST,
            &mut failures,
        ) {
            Ok(leaf) => leaf,
            Err(code) => return code,
        },
    };
    let recomputed_root =
        pack_root_from_grammar_leaf(&recomputed_digest, &dict, &grammar_leaf_hex, &ecosystem);
    let declared_root = match pack {
        Some(path) => pinned_literal(reporter, path, GMN_PACK_ROOT),
        None => pinned_literal_of(reporter, &bundle, GMN_PACK_CURRENT, GMN_PACK_ROOT).map(Some),
    };
    let pack_status: String = match declared_root {
        Ok(Some(declared)) if declared == recomputed_root => {
            format!("pack-root {recomputed_root} (matches declaration)")
        }
        Ok(Some(declared)) => {
            failures.push(format!(
                "declared gmeow:gmnPackRoot {declared} but the parts recompute to {recomputed_root}"
            ));
            format!("pack-root {recomputed_root} (MISMATCH vs {declared})")
        }
        Ok(None) => {
            failures.push("no gmeow:gmnPackRoot declaration found".to_owned());
            format!("pack-root {recomputed_root} (no declaration)")
        }
        Err(code) => return code,
    };

    // ── summary + exit ──
    println!("codebook-digest {recomputed_digest}");
    println!("{pack_status}");
    println!("positives {positives}/{}", disk_stems.len());
    println!("negatives {negatives}");
    if failures.is_empty() {
        println!("gmn conformance PASS");
        0
    } else {
        for f in &failures {
            emit_error(reporter, "gmeow-cli.gmn.verify.failure", f.clone());
        }
        fail(
            reporter,
            "gmeow-cli.gmn.verify.failed",
            format!("gmn conformance FAILED with {} failure(s)", failures.len()),
        )
    }
}

// ── gmn migrate ────────────────────────────────────────────────────────────────

/// `gmeow gmn migrate --correspondence <IRI> --migrations <FILE> <input.gmn>` — the
/// production entry point for the GMN version-migration executor
/// ([`gmeow_lang_bridge::GmnMigration::migrate`]): re-emit a STORED GMN-1 document at the
/// target dialect major across an authored inter-version correspondence.
///
/// Every operator vocabulary is GRAPH-DERIVED, never hardcoded: the dictionary / registry
/// come from the EMBEDDED bundle (or a `--lang-module` override), and the migration leg — the
/// `logic:Correspondence`, its `gmeow:GmnMigrationRewrite`s, and the target major's
/// `gmeow:gmnVersionDefinesOperator` native inventory — from the `--migrations` TTL. The flow:
///
/// 1. project the stored document to its operator surface ([`source_operator_table`] +
///    [`extract_operators`]) — source glyphs mapped back to version-stable terms;
/// 2. derive the target major's native operator inventory ([`derive_target_inventory`]);
/// 3. run the executor; an unbridged glyph drop (or any migration error) is a HARD FAIL,
///    surfacing the named `lang:GmnUnbridgedGlyphDrop` class and a non-zero exit — never a
///    silent repair;
/// 4. re-emit the migrated document at the target major on stdout ([`reemit_migrated_document`]),
///    reporting the crossing's preservation JUDGMENT (never a boolean) on stderr.
///
/// A missing/unreadable input, a document with no `@gmn{v: …}` header, an out-of-window source
/// major, an unloadable dictionary, or a malformed migration leg are each a HARD FAIL (exit 1).
pub fn migrate(
    reporter: &dyn Reporter,
    input: &Path,
    correspondence: &str,
    migrations: &Path,
    lang_module: Option<&Path>,
) -> i32 {
    // 1. The dictionary / registry + operator precedences: the embedded bundle by default, or
    //    the `--lang-module` override — the SAME source the sibling legs resolve.
    let base_ds = match lang_module {
        Some(path) => match file_dataset(reporter, path) {
            Ok(ds) => ds,
            Err(code) => return code,
        },
        None => match bundle_dataset(reporter) {
            Ok(ds) => ds,
            Err(code) => return code,
        },
    };
    let dict = match GmnDictionary::from_dataset(&base_ds) {
        Ok(dict) => dict,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.gmn.dictionary",
                format!("cannot load gmeow:gmnDictV3: {}", e.0),
            );
        }
    };

    // 2. The authored migration leg: correspondence + rewrites + target inventory.
    let mig_ds = match file_dataset(reporter, migrations) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let migration = match GmnMigration::from_dataset(&mig_ds, correspondence) {
        Ok(migration) => migration,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.gmn.migrate.leg",
                format!(
                    "cannot load migration correspondence <{correspondence}> from {}: {e}",
                    migrations.display()
                ),
            );
        }
    };

    // 3. The stored source-major GMN-1 document.
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.gmn.input-read",
                format!("cannot read {}: {e}", input.display()),
            );
        }
    };

    // 4. Header presence + source-major-in-window: both HARD FAILS (no migrating on a guess).
    let header_major = match header_schema_major(&text) {
        Some(major) => major,
        None => {
            return fail(
                reporter,
                "gmeow-cli.gmn.migrate.malformed",
                format!(
                    "{} carries no @gmn{{v: …}} header; a migratable GMN-1 document must pin its \
                     source dialect major",
                    input.display()
                ),
            );
        }
    };
    if header_major != migration.from_version() {
        return fail(
            reporter,
            "gmeow-cli.gmn.migrate.version",
            format!(
                "{} pins dialect major {header_major:?} but migration <{correspondence}> migrates \
                 FROM major {:?}; the stored document is outside the crossing's source window",
                input.display(),
                migration.from_version()
            ),
        );
    }

    // 5. Project the document to its operator surface — every glyph graph-resolved to its term.
    let table = source_operator_table(&dict, &migration, &base_ds);
    let record_set = extract_operators(&text, &table);

    // 6. The target major's native operator inventory, graph-derived from the migration leg.
    let target_inventory = match derive_target_inventory(&mig_ds, correspondence) {
        Ok(inventory) => inventory,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.gmn.migrate.leg",
                format!(
                    "cannot derive the target-major operator inventory for <{correspondence}>: {e}"
                ),
            );
        }
    };

    // 7. Run the executor. An unbridged glyph drop (or any migration error) is a HARD FAIL that
    //    surfaces the named lang: conformance class — never a silent repair or degrade.
    let migrated = match migration.migrate(&record_set, &target_inventory) {
        Ok(migrated) => migrated,
        Err(e) => {
            let class = e.failure_class().unwrap_or("(none)");
            return fail(
                reporter,
                "gmeow-cli.gmn.migrate.unbridged",
                format!(
                    "migration <{correspondence}> cannot re-emit {} at major {}: {e} [{class}]",
                    input.display(),
                    migration.to_version()
                ),
            );
        }
    };

    // 8. Re-emit at the target major on stdout; report the preservation JUDGMENT on stderr.
    print!(
        "{}",
        reemit_migrated_document(&text, &record_set, &migrated, migration.to_version())
    );
    eprintln!(
        "gmn migrate: <{correspondence}> major {} -> {}, preservation logic:{}, {} operator(s) \
         migrated",
        migration.from_version(),
        migration.to_version(),
        migrated.preservation.as_str(),
        migrated.operators.len(),
    );
    0
}

// ── small structural helpers ───────────────────────────────────────────────────

fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned()
}

/// The sorted `<stem>` of every `*.in.ttl` at the corpus root (the positive inputs).
fn positive_stems(reporter: &dyn Reporter, dir: &Path) -> Result<Vec<String>, i32> {
    let mut stems = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.verify.vectors",
            format!("cannot read vectors dir {}: {e}", dir.display()),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            fail(
                reporter,
                "gmeow-cli.gmn.verify.vectors",
                format!("dir entry error in {}: {e}", dir.display()),
            )
        })?;
        if let Some(name) = entry.file_name().to_str()
            && let Some(stem) = name.strip_suffix(".in.ttl")
        {
            stems.push(stem.to_owned());
        }
    }
    stems.sort();
    Ok(stems)
}

/// The positive-vector inventory the corpus manifest declares, read off each
/// `<name>.in.ttl -> <name>.gmn` `rdfs:label`. The manifest is MANDATORY (a missing or
/// empty manifest is a HARD fail) so a truncated corpus cannot self-declare a reduced set.
fn manifest_inventory(reporter: &dyn Reporter, manifest: &Path) -> Result<BTreeSet<String>, i32> {
    let bytes = std::fs::read(manifest).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.verify.manifest",
            format!(
                "cannot read required vector-manifest.ttl {}: {e}",
                manifest.display()
            ),
        )
    })?;
    let ds = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.verify.manifest",
            format!("cannot parse {}: {e}", manifest.display()),
        )
    })?;
    let mut stems = BTreeSet::new();
    for q in ds.owned_quads() {
        if q.predicate == RDFS_LABEL
            && let RdfTerm::Literal(lit) = &q.object
            && let Some((stem, rest)) = lit.lexical_form.split_once(".in.ttl -> ")
            && rest == format!("{stem}.gmn")
        {
            stems.insert(stem.to_owned());
        }
    }
    if stems.is_empty() {
        return Err(fail(
            reporter,
            "gmeow-cli.gmn.verify.manifest",
            format!(
                "vector-manifest.ttl {} declares no `<name>.in.ttl -> <name>.gmn` inventory",
                manifest.display()
            ),
        ));
    }
    Ok(stems)
}

/// Parse `negative-codec/expected.ttl` into a `filename -> failure-class-local-name` map. The
/// negative tier is MANDATORY: a missing `expected.ttl` is a HARD fail, so a corpus cannot drop its
/// negative coverage and still certify.
fn expected_negative_classes(
    reporter: &dyn Reporter,
    neg_dir: &Path,
) -> Result<BTreeMap<String, String>, i32> {
    let path = neg_dir.join("expected.ttl");
    let bytes = std::fs::read(&path).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.verify.expected",
            format!(
                "cannot read required negative-codec/expected.ttl {}: {e}",
                path.display()
            ),
        )
    })?;
    let ds = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.verify.expected",
            format!("cannot parse {}: {e}", path.display()),
        )
    })?;
    let mut label: BTreeMap<String, String> = BTreeMap::new();
    let mut class: BTreeMap<String, String> = BTreeMap::new();
    for q in ds.owned_quads() {
        let RdfTerm::BlankNode(subj) = &q.subject else {
            continue;
        };
        match q.predicate.as_str() {
            RDFS_LABEL => {
                if let RdfTerm::Literal(lit) = &q.object {
                    label.insert(subj.clone(), lit.lexical_form.clone());
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
        if let Some(cls) = class.get(&subj) {
            out.insert(file, cls.clone());
        } else {
            return Err(fail(
                reporter,
                "gmeow-cli.gmn.verify.expected",
                format!("expected.ttl entry {file} has no gmeow:enforcesFailureClass"),
            ));
        }
    }
    if out.is_empty() {
        return Err(fail(
            reporter,
            "gmeow-cli.gmn.verify.expected",
            format!("{} declares no negative vectors", path.display()),
        ));
    }
    Ok(out)
}

/// The single literal lexical value declared for `predicate` anywhere in a parsed Turtle document
/// (the pack / manifest declare exactly one). HARD-FAILS when more than one DISTINCT literal is
/// present — no first-of-many selection (LOW/NO-OPTIONALITY). Returns `None` when the predicate is
/// absent.
fn pinned_literal(
    reporter: &dyn Reporter,
    path: &Path,
    predicate: &str,
) -> Result<Option<String>, i32> {
    let bytes = std::fs::read(path).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.verify.pinned",
            format!("cannot read {}: {e}", path.display()),
        )
    })?;
    // The shipped pack projection is N-Triples on disk; Turtle parses both.
    let ds = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.gmn.verify.pinned",
            format!("cannot parse {}: {e}", path.display()),
        )
    })?;
    let mut found: Vec<String> = ds
        .owned_quads()
        .filter(|q| q.predicate == predicate)
        .filter_map(|q| match &q.object {
            RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
            _ => None,
        })
        .collect();
    found.sort();
    found.dedup();
    if found.len() > 1 {
        return Err(fail(
            reporter,
            "gmeow-cli.gmn.verify.ambiguous",
            format!(
                "{} declares {} distinct {predicate} literals; exactly one is required",
                path.display(),
                found.len()
            ),
        ));
    }
    Ok(found.into_iter().next())
}

/// The single literal value declared on `<subject> <predicate>` in a dataset, if present — the
/// checkout-free bundle read. Filters by BOTH subject and predicate (a digest predicate like
/// `gmeow:gmnCodebookDigest` also appears on example envelopes, so a predicate-only scan would be
/// ambiguous). `Ok(None)` when the pair is absent; HARD-FAILS (exit 1) only when AMBIGUOUS (more
/// than one distinct literal).
fn pinned_literal_of_opt(
    reporter: &dyn Reporter,
    ds: &RdfDataset,
    subject: &str,
    predicate: &str,
) -> Result<Option<String>, i32> {
    let mut found: Vec<String> = ds
        .owned_quads()
        .filter(|q| q.predicate == predicate)
        .filter(|q| matches!(&q.subject, RdfTerm::Iri(s) if s == subject))
        .filter_map(|q| match &q.object {
            RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
            _ => None,
        })
        .collect();
    found.sort();
    found.dedup();
    match found.len() {
        0 => Ok(None),
        1 => Ok(Some(found.into_iter().next().expect("len==1"))),
        n => Err(fail(
            reporter,
            "gmeow-cli.gmn.verify.pinned",
            format!(
                "bundle declares {n} distinct <{subject}> <{predicate}> literals; exactly one is required"
            ),
        )),
    }
}

/// The single literal value declared on `<subject> <predicate>` — [`pinned_literal_of_opt`] with
/// an ABSENT pair promoted to a HARD fail (the codebook / grammar leaf are mandatory bundle pins).
fn pinned_literal_of(
    reporter: &dyn Reporter,
    ds: &RdfDataset,
    subject: &str,
    predicate: &str,
) -> Result<String, i32> {
    match pinned_literal_of_opt(reporter, ds, subject, predicate)? {
        Some(value) => Ok(value),
        None => Err(fail(
            reporter,
            "gmeow-cli.gmn.verify.pinned",
            format!(
                "bundle declares 0 distinct <{subject}> <{predicate}> literals; exactly one is required"
            ),
        )),
    }
}

/// Read ONE pinned ecosystem-view Merkle leaf (`<subject> <predicate>`) off the bundle for the
/// whole-ecosystem pack-root recomputation. A MISSING declaration is a conformance failure —
/// accumulated into `failures` (non-zero exit at the end, never a silent pass) — and returns an
/// empty leaf so the recomputed root still forms and DISAGREES with the pinned root, so verify reds.
/// An AMBIGUOUS declaration hard-fails (exit 1). This mirrors the grammar-leaf bundle read.
fn bundle_view_leaf(
    reporter: &dyn Reporter,
    ds: &RdfDataset,
    subject: &str,
    predicate: &str,
    failures: &mut Vec<String>,
) -> Result<String, i32> {
    match pinned_literal_of_opt(reporter, ds, subject, predicate)? {
        Some(leaf) => Ok(leaf),
        None => {
            failures.push(format!(
                "bundle declares no <{subject}> <{predicate}> ecosystem-view Merkle leaf; the pack \
                 root cannot certify that surface from the bundle alone"
            ));
            Ok(String::new())
        }
    }
}
