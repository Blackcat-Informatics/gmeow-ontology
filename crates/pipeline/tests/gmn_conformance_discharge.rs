// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMN conformance execution-discharge harness + runnable completeness invariant.
//!
//! Every GMN row of the `lang:` CONFORMANCE matrix ([`LANG-CONFORMANCE.md`], the "GMN dialect
//! rules" table) is discharged HERE by EXECUTION, not by fixture existence:
//!
//! * **Validator tier** (the six [`gmeow_lang_bridge::Gmn1Error`] classes) — the four labeled
//!   `INVALID — lang:Gmn…` blocks are EXTRACTED from the normative [`LANG-GMN.md`] charter and
//!   driven through the production [`gmeow_lang_bridge::gmn1_read`] codec; each raises EXACTLY
//!   its labeled class. The two residual classes carry no INVALID block and are driven from
//!   synthetic inputs: `GmnNonDecodableGrammar` from a non-decodable read input, and
//!   `GmnGraphOutOfDomain` from a named-graph model pushed through [`gmeow_lang_bridge::gmn1_write`]
//!   (the honest default-graph domain boundary). The VALID header form is extracted and reads
//!   back `Ok`.
//! * **SHACL tier** (thirteen `lang:Gmn*Shape` gates) — each counter-example is pushed through
//!   [`support::flagship_discharge::triggered_slice_failures`] (native structural lint ∪ native
//!   SHACL, filtered to `lang:`, merged with `module.ttl`) and its trip set is asserted EXACTLY
//!   (never mere membership); its worked example raises nothing.
//! * **Native tier** — two `Rust` gates that see graph-level state no codec byte-parse or SHACL
//!   shape can: (1) `lang:SilentDisambiguation`, driven through the native
//!   `structural_lint_dataset` (`crates/validate/src/lint.rs check_silent_disambiguation`); and
//!   (2) `lang:GmnCodebookDigestMismatch`, driven through the native codebook-digest gate
//!   ([`gmeow_pipeline::stages::gmn1_gate::check_gmn1_codebook_digest`]) which recomputes the
//!   codebook Merkle root and refuses an envelope whose declared `gmeow:gmnCodebookDigest`
//!   disagrees.
//! * **Build assert** (the `@λ` column ruling) — the `GMN_LANG_AST_COLUMNS` constant is
//!   re-exercised against the PRODUCTION CoNLL-U serializer's emitted column order, the same
//!   invariant `crates/lang-bridge/src/gmn_symbology.rs` pins at build time.
//!
//! The [`completeness_invariant_leaves_no_fixture_existence_only_row`] test is the EXECUTABLE
//! form of "no fixture-existence-only rows remain": it enumerates every `gmn-*.ttl`
//! counter-example on disk AND every GMN row parsed from the matrix, HARD-FAILS on any that has
//! no asserted discharge, and reconciles per tier (validator/SHACL/native/build) so drift at
//! any tier — a new codec class, a new shape, a matrix edit — reds the gate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_lang_bridge::{
    ConlluDoc, ConlluSentence, ConlluToken, GMN_LANG_AST_COLUMNS, Gmn0Model, Gmn1Document,
    Gmn1Error, GmnDictionary, TokenId, gmn1_read, gmn1_write, round_trip_check, serialize_conllu,
};
use gmeow_pipeline::stages::gmn1_gate;
use gmeow_validate::lint::structural_lint_dataset;
use gmeow_validate::store::dataset_from_paths;
use purrdf::{RdfQuad, RdfTerm};
mod support;
use support::flagship_discharge::{
    SliceSpec, load_scoped_shapes, local_name, minimal_lint_config, native_failure_classes,
    repo_root, shape_class_map, shared_shapes_path, triggered_slice_failures,
};

// ─────────────────────────────────────────────────────────────────────────────────────────
// Slice identity + fixture paths.
// ─────────────────────────────────────────────────────────────────────────────────────────

const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

fn lang_spec() -> SliceSpec {
    SliceSpec {
        slice_ns: LANG_NS,
        slice_prefix: "lang",
        slice_root: lang_root(),
        manifest_rel: "examples/flagship-acceptance.ttl",
    }
}

fn lang_root() -> PathBuf {
    repo_root().join("slices").join("grounding").join("lang")
}

fn counter_dir() -> PathBuf {
    lang_root().join("tests").join("counter-examples")
}

fn worked_dir() -> PathBuf {
    lang_root().join("tests").join("conformance-fixtures")
}

/// The set of `lang:` failure-class local names for the SIX typed codec classes, resolved
/// through the production [`Gmn1Error`] IRI constants. Enumerated WITHOUT a wildcard so a new
/// `Gmn1Error` variant forces an edit here (mirroring the codec's own exhaustive
/// `failure_class` witness) — the validator-tier drift surface. `GmnNonCanonicalCodepoint`
/// (the non-NFC literal class) reuses one of these IRIs and is not a distinct tier row.
fn codec_classes() -> BTreeSet<String> {
    [
        Gmn1Error::CLASS_UNCOVERED_TERM,
        Gmn1Error::CLASS_GRAPH_OUT_OF_DOMAIN,
        Gmn1Error::CLASS_NON_CANONICAL_ORDER,
        Gmn1Error::CLASS_MALFORMED_NUMBER,
        Gmn1Error::CLASS_UNDECLARED_DIALECT_VERSION,
        Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR,
    ]
    .into_iter()
    .map(local_name)
    .collect()
}

/// The four validator classes that carry a labeled `INVALID` block in `LANG-GMN.md`
/// (`GmnNonDecodableGrammar` is the residual class — driven from a synthetic non-decodable
/// input, never a normative block).
fn doc_block_classes() -> BTreeSet<String> {
    [
        "GmnUncoveredTerm",
        "GmnNonCanonicalOrder",
        "GmnMalformedNumber",
        "GmnUndeclaredDialectVersion",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// (A) Validator-tier rows — doc-extracted fixtures driven through the production codec.
// ─────────────────────────────────────────────────────────────────────────────────────────

/// The extracted `text` fenced blocks of the `LANG-GMN.md` charter, keyed by the local name of
/// the `lang:Gmn…` class named on the preceding `INVALID —` line. ROBUST: tolerates the
/// parenthetical after the label and the ```` ```text ```` fence.
fn extract_invalid_blocks(md: &str) -> BTreeMap<String, String> {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("INVALID") {
            let class = extract_lang_local(lines[i])
                .unwrap_or_else(|| panic!("INVALID line names no lang: class: {}", lines[i]));
            // The next fenced block is the machine fixture.
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            assert!(
                j < lines.len(),
                "INVALID block for lang:{class} has no opening ``` fence"
            );
            let mut body = String::new();
            let mut k = j + 1;
            while k < lines.len() && !lines[k].trim_start().starts_with("```") {
                body.push_str(lines[k]);
                body.push('\n');
                k += 1;
            }
            assert!(
                k < lines.len(),
                "INVALID block for lang:{class} has no closing ``` fence"
            );
            let prior = out.insert(class.clone(), body);
            assert!(
                prior.is_none(),
                "duplicate INVALID block for lang:{class} in LANG-GMN.md"
            );
            i = k + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The bodies of every ```` ```text ```` (or bare ```` ``` ````) fenced block in the charter.
fn all_fenced_blocks(md: &str) -> Vec<String> {
    let lines: Vec<&str> = md.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("```") {
            let mut body = String::new();
            let mut k = i + 1;
            while k < lines.len() && !lines[k].trim_start().starts_with("```") {
                body.push_str(lines[k]);
                body.push('\n');
                k += 1;
            }
            blocks.push(body);
            i = k + 1;
        } else {
            i += 1;
        }
    }
    blocks
}

/// Extract the FIRST `lang:<Local>` token's local name from a line (the label position).
fn extract_lang_local(line: &str) -> Option<String> {
    let idx = line.find("lang:")? + "lang:".len();
    let local: String = line[idx..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    (!local.is_empty()).then_some(local)
}

#[test]
fn validator_tier_rows_discharge_via_production_codec() {
    let md = std::fs::read_to_string(lang_root().join("design").join("LANG-GMN.md"))
        .expect("LANG-GMN.md is readable");
    let blocks = extract_invalid_blocks(&md);

    // HARD-FAIL on label drift: the extracted labeled-block set is EXACTLY the four
    // INVALID-block validator classes — a missing OR extra block reds this gate.
    let extracted: BTreeSet<String> = blocks.keys().cloned().collect();
    assert_eq!(
        extracted,
        doc_block_classes(),
        "the LANG-GMN.md INVALID blocks must name EXACTLY the four validator classes"
    );

    let dict = GmnDictionary::default();
    for (class, body) in &blocks {
        match gmn1_read(&Gmn1Document::from_text(body.clone()), &dict) {
            Ok(_) => panic!("INVALID block for lang:{class} must FAIL to read, but it decoded"),
            Err(err) => assert_eq!(
                local_name(err.failure_class()),
                *class,
                "the INVALID block labeled lang:{class} must raise EXACTLY that class, got {}",
                err.failure_class()
            ),
        }
    }

    // GmnNonDecodableGrammar: the residual class, no normative block — a genuinely undecodable
    // input (an unknown sigil the parse table has no production for).
    let non_decodable = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@x{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1}\n";
    let err = gmn1_read(&Gmn1Document::from_text(non_decodable), &dict)
        .expect_err("an unknown sigil is non-decodable grammar");
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);

    // GmnGraphOutOfDomain: the second residual class (also no normative INVALID block). It is a
    // WRITE-side domain boundary — a quad carrying a named graph is outside the default-graph
    // GMN-0 normal form — so it is driven through the production `gmn1_write`, not `gmn1_read`.
    let named_graph_model = Gmn0Model {
        quads: vec![RdfQuad {
            subject: RdfTerm::Iri("https://blackcatinformatics.ca/gmeow/gate1".to_owned()),
            predicate: "https://blackcatinformatics.ca/gmeow/hasState".to_owned(),
            object: RdfTerm::Iri("https://blackcatinformatics.ca/gmeow/doorGate1".to_owned()),
            graph_name: Some(RdfTerm::Iri(
                "https://blackcatinformatics.ca/gmeow/namedGraph1".to_owned(),
            )),
            location: None,
        }],
    };
    let err = gmn1_write(&named_graph_model, &dict)
        .expect_err("a named-graph quad is out of the default-graph GMN-0 domain");
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_GRAPH_OUT_OF_DOMAIN);

    // VALID header form: the first lone `@gmn{…}` fenced block of the charter reads back Ok.
    let header_block = all_fenced_blocks(&md)
        .into_iter()
        .find(|b| {
            let mut non_empty = b.lines().filter(|l| !l.trim().is_empty());
            match (non_empty.next(), non_empty.next()) {
                (Some(first), None) => first.trim_start().starts_with("@gmn{"),
                _ => false,
            }
        })
        .expect("LANG-GMN.md carries a lone @gmn{…} VALID header block");
    gmn1_read(&Gmn1Document::from_text(header_block), &dict)
        .expect("the VALID header form reads back Ok");

    // A canonical record + header reads Ok and round-trips byte-stably. The charter's
    // illustrative record (LANG-GMN.md, "A valid record") uses placeholder terms (gate1,
    // hasState) the shipped dictionary does not mint, so the decodable canonical form uses
    // `gmeow__`-direct IRIs — the codec's own reference-position encoding.
    let canonical = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1, q: 0.95}\n";
    let model =
        gmn1_read(&Gmn1Document::from_text(canonical), &dict).expect("a canonical record reads Ok");
    round_trip_check(&model, &dict).expect("a canonical record round-trips byte-stably");
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// (B) SHACL-tier rows — minimized exact-set assertion.
// ─────────────────────────────────────────────────────────────────────────────────────────

/// One SHACL-tier discharge: a counter-example fixture, its named failure class, the EXACT set
/// of `lang:` classes it trips (a singleton for every row except the structurally-coupled
/// export-ring row), and a clean worked example.
struct ShaclRow {
    counter: &'static str,
    class: &'static str,
    trips: &'static [&'static str],
    worked: &'static str,
}

/// The thirteen SHACL-tier GMN conformance rows (fourteen counter-examples — the envelope
/// contract carries two). Each `trips` set is asserted EXACTLY. The export-ring row's trip set
/// is the irreducible two-class pair (see the module.ttl / LANG-CONFORMANCE.md note): a
/// ring-less export envelope's firing condition is a strict subset of the nine-field contract's
/// ring `sh:minCount 1`, so `GmnMissingEnvelopeField` co-fires by construction. Asserting the
/// exact two-class set is set-equality, NOT membership — no assertion is weakened.
fn shacl_rows() -> Vec<ShaclRow> {
    vec![
        ShaclRow {
            counter: "gmn-noncanonical-codepoint.ttl",
            class: "GmnNonCanonicalCodepoint",
            trips: &["GmnNonCanonicalCodepoint"],
            worked: "gmn-script-glyph-canonical.ttl",
        },
        ShaclRow {
            counter: "gmn-confusable-glyph.ttl",
            class: "GmnConfusableGlyph",
            trips: &["GmnConfusableGlyph"],
            worked: "gmn-script-glyph-canonical.ttl",
        },
        ShaclRow {
            counter: "gmn-glyph-collision.ttl",
            class: "GmnGlyphCollision",
            trips: &["GmnGlyphCollision"],
            worked: "gmn-script-glyph-canonical.ttl",
        },
        ShaclRow {
            counter: "gmn-envelope-missing-field.ttl",
            class: "GmnMissingEnvelopeField",
            trips: &["GmnMissingEnvelopeField"],
            worked: "gmn-envelope-complete.ttl",
        },
        ShaclRow {
            counter: "gmn-envelope-dictionary-version-plural.ttl",
            class: "GmnMissingEnvelopeField",
            trips: &["GmnMissingEnvelopeField"],
            worked: "gmn-envelope-complete.ttl",
        },
        ShaclRow {
            counter: "gmn-dictionary-alias-collision.ttl",
            class: "GmnDictionaryAliasCollision",
            trips: &["GmnDictionaryAliasCollision"],
            worked: "gmn-dictionary-alias-unique.ttl",
        },
        ShaclRow {
            counter: "gmn-ring-lattice-malformed.ttl",
            class: "GmnRingLatticeMalformed",
            trips: &["GmnRingLatticeMalformed"],
            worked: "gmn-envelope-complete.ttl",
        },
        ShaclRow {
            counter: "gmn-version-overclaim.ttl",
            class: "GmnVersionOverclaim",
            trips: &["GmnVersionOverclaim"],
            worked: "gmn-migration-additive.ttl",
        },
        ShaclRow {
            counter: "gmn-compaction-without-provenance.ttl",
            class: "GmnCompactionWithoutProvenance",
            trips: &["GmnCompactionWithoutProvenance"],
            worked: "gmn-compaction-honest.ttl",
        },
        ShaclRow {
            counter: "gmn-compaction-overclaim.ttl",
            class: "GmnCompactionOverclaim",
            trips: &["GmnCompactionOverclaim"],
            worked: "gmn-compaction-honest.ttl",
        },
        ShaclRow {
            counter: "gmn-undispositioned-feature-value.ttl",
            class: "GmnUndispositionedTerm",
            trips: &["GmnUndispositionedTerm"],
            worked: "gmn-feature-value-dispositioned.ttl",
        },
        ShaclRow {
            counter: "gmn-plane-missing-version.ttl",
            class: "GmnUnattributedPlane",
            trips: &["GmnUnattributedPlane"],
            worked: "gmn-imported-plane-attributed.ttl",
        },
        ShaclRow {
            counter: "gmn-uncosted-script-glyph.ttl",
            class: "GmnUncostedScriptGlyph",
            trips: &["GmnUncostedScriptGlyph"],
            worked: "gmn-costed-script-glyph.ttl",
        },
        ShaclRow {
            counter: "gmn-export-crossing-no-ring.ttl",
            class: "GmnUnringedExportCrossing",
            trips: &["GmnMissingEnvelopeField", "GmnUnringedExportCrossing"],
            worked: "gmn-export-ring-bound.ttl",
        },
    ]
}

/// The native-lint row (`lang:SilentDisambiguation`) — no SHACL shape, driven through the
/// native structural lint.
const NATIVE_COUNTER: &str = "gmn-compaction-silent-disambiguation.ttl";
const NATIVE_WORKED: &str = "gmn-compaction-honest.ttl";
const NATIVE_CLASS: &str = "SilentDisambiguation";

/// The native GMN codebook-digest gate row (`lang:GmnCodebookDigestMismatch`) — NOT a SHACL
/// shape and NOT the per-record codec validator: the native gate
/// ([`gmn1_gate::check_gmn1_codebook_digest`]) recomputes the codebook Merkle root and refuses
/// an envelope whose declared digest disagrees. Its counter-example is the graph-tier
/// `negative-graph/envelope-digest-mismatch.ttl` fixture (a wrong declared digest); its worked
/// example the nine-field `gmn-envelope-complete.ttl` (which declares the real recomputed digest).
const NATIVE_GMN_GATE_CLASS: &str = "GmnCodebookDigestMismatch";
const NATIVE_GMN_GATE_COUNTER: &str = "envelope-digest-mismatch.ttl";
const NATIVE_GMN_GATE_WORKED: &str = "gmn-envelope-complete.ttl";

/// The graph-tier negative fixtures the native codebook-digest gate is driven over.
fn negative_graph_dir() -> PathBuf {
    lang_root()
        .join("tests")
        .join("gmn1-vectors")
        .join("negative-graph")
}

/// The build-assert row carries no failure class; its matrix cell is the em-dash marker.
const BUILD_MARKER: &str = "—";

/// Parse the canonical generated shapes plus residual slice shapes and build the shape→class
/// map, mirroring the flagship runner's compositional migration contract.
fn load_shapes() -> (purrdf::shapes::shapes::Shapes, BTreeMap<String, String>) {
    let spec = lang_spec();
    let (shapes, paths) = load_scoped_shapes(&spec);
    let mut class_paths = paths;
    class_paths.push(shared_shapes_path());
    let map = shape_class_map(&class_paths);
    (shapes, map.into_iter().collect())
}

fn set_of(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn shacl_tier_rows_discharge_by_execution() {
    let spec = lang_spec();
    let (shapes, shape_class) = load_shapes();
    let shape_class: std::collections::HashMap<String, String> = shape_class.into_iter().collect();
    let rows = shacl_rows();
    // Each row validates an independent counter/worked pair against the same
    // frozen grounding kernel and scoped lang: shapes. Bound internal
    // parallelism at six so the 14-row matrix retains CI headroom under the per-test wall
    // budget without multiplying the large immutable datasets without limit.
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(rows.len().max(1))
        .min(6);
    let mut validations: Vec<(usize, BTreeSet<String>, BTreeSet<String>)> =
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for worker in 0..workers {
                let rows = &rows;
                let shapes = &shapes;
                let shape_class = &shape_class;
                let spec = &spec;
                handles.push(scope.spawn(move || {
                    let mut out = Vec::new();
                    for (index, row) in rows.iter().enumerate() {
                        if index % workers != worker {
                            continue;
                        }
                        let counter = counter_dir().join(row.counter);
                        let triggered =
                            triggered_slice_failures(spec, &counter, shapes, shape_class)
                                .into_iter()
                                .collect();
                        let worked = worked_dir().join(row.worked);
                        let clean = triggered_slice_failures(spec, &worked, shapes, shape_class)
                            .into_iter()
                            .collect();
                        out.push((index, triggered, clean));
                    }
                    out
                }));
            }
            handles
                .into_iter()
                .flat_map(|handle| handle.join().expect("GMN SHACL worker joins"))
                .collect()
        });
    validations.sort_by_key(|(index, _, _)| *index);

    for (row, (_, triggered, clean)) in rows.iter().zip(validations) {
        assert_eq!(
            triggered,
            set_of(row.trips),
            "counter-example {} must trip EXACTLY {:?}, but tripped {triggered:?}",
            row.counter,
            row.trips
        );

        assert!(
            clean.is_empty(),
            "worked example {} must raise nothing, but raised {clean:?}",
            row.worked
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// (C) Native-lint + build-assert rows.
// ─────────────────────────────────────────────────────────────────────────────────────────

/// The `lang:` classes the NATIVE structural lint raises over a fixture merged with module.ttl.
fn native_lint_failures(fixture: &Path) -> BTreeSet<String> {
    let module = lang_root().join("module.ttl");
    let ds = dataset_from_paths(&[module, fixture.to_path_buf()])
        .unwrap_or_else(|e| panic!("parse {} + module: {e}", fixture.display()));
    let report = structural_lint_dataset(&ds, &minimal_lint_config());
    native_failure_classes(&report.errors(), "lang")
        .into_iter()
        .collect()
}

#[test]
fn native_silent_disambiguation_row_discharges() {
    let counter = native_lint_failures(&counter_dir().join(NATIVE_COUNTER));
    assert_eq!(
        counter,
        set_of(&[NATIVE_CLASS]),
        "the silent-disambiguation counter must raise EXACTLY lang:{NATIVE_CLASS} via the \
         native lint (check_silent_disambiguation), got {counter:?}"
    );
    let worked = native_lint_failures(&worked_dir().join(NATIVE_WORKED));
    assert!(
        worked.is_empty(),
        "the honest-compaction worked example must raise nothing, got {worked:?}"
    );
}

#[test]
fn native_codebook_digest_row_discharges() {
    let root = repo_root();

    // The counter-example: an envelope declaring a codebook digest the real codebook does not
    // have. Driven through the SAME native gate `run.rs` wires on-gate — a mismatch trips
    // EXACTLY lang:GmnCodebookDigestMismatch, naming the offending envelope.
    let counter = negative_graph_dir().join(NATIVE_GMN_GATE_COUNTER);
    let report = gmn1_gate::check_gmn1_codebook_digest(&root, std::slice::from_ref(&counter))
        .expect("codebook-digest gate runs without a hard I/O error");
    assert!(
        !report.is_clean(),
        "the digest-mismatch counter must trip the native codebook-digest gate, not pass vacuously"
    );
    assert_eq!(
        report.checked, 1,
        "the counter declares exactly one envelope codebook digest to check, got {}",
        report.checked
    );
    let classes: BTreeSet<String> = report
        .mismatches
        .iter()
        .map(|m| local_name(m.failure_class()))
        .collect();
    assert_eq!(
        classes,
        set_of(&[NATIVE_GMN_GATE_CLASS]),
        "the counter must raise EXACTLY lang:{NATIVE_GMN_GATE_CLASS}, got {classes:?}"
    );

    // The worked example: the nine-field envelope declaring the REAL recomputed digest — the
    // gate checks it (checked == 1) and raises nothing.
    let worked = worked_dir().join(NATIVE_GMN_GATE_WORKED);
    let clean = gmn1_gate::check_gmn1_codebook_digest(&root, std::slice::from_ref(&worked))
        .expect("codebook-digest gate runs over the worked example");
    assert!(
        clean.is_clean(),
        "the complete-envelope worked example declares the real digest and must raise nothing, \
         got {:?}",
        clean.mismatches
    );
    assert_eq!(
        clean.checked, 1,
        "the worked example declares exactly one envelope codebook digest, got {}",
        clean.checked
    );
}

/// A falsifiable doctrine lint: the GMN-1 charter must NEVER frame its default-graph boundary as
/// a "lossy lens" / silent drop, and every remaining `lossy` occurrence must be scoped to GMN-2
/// (the honest lossy-compaction variety), never to GMN-1. A regression here (recasting the honest
/// `lang:GmnGraphOutOfDomain` boundary as tolerated loss) hard-fails the gate.
#[test]
fn lang_gmn_charter_carries_no_lossy_lens_framing() {
    let md = std::fs::read_to_string(lang_root().join("design").join("LANG-GMN.md"))
        .expect("LANG-GMN.md readable");
    let lower = md.to_lowercase();

    const BANNED: &[&str] = &[
        "lossy lens",
        "lossy-lens",
        "lossy narrow",
        "silently drop",
        "section-retraction with narrow",
    ];
    for phrase in BANNED {
        assert!(
            !lower.contains(phrase),
            "LANG-GMN.md must not frame the GMN-1 boundary as {phrase:?}: the default-graph \
             refusal is an honest typed boundary (lang:GmnGraphOutOfDomain), never a lossy lens \
             or a silent drop"
        );
    }

    for (i, line) in md.lines().enumerate() {
        if line.to_lowercase().contains("lossy") {
            assert!(
                line.contains("GMN-2"),
                "LANG-GMN.md line {} uses 'lossy' outside a GMN-2 scope — a GMN-1 line must never \
                 call itself lossy: {line:?}",
                i + 1
            );
        }
    }
}

#[test]
fn build_assert_row_lang_ast_columns_match_conllu_serializer() {
    // Exercise the SAME production surfaces the crate's `gmn_symbology.rs` build assert pins:
    // the public `GMN_LANG_AST_COLUMNS` constant and the production CoNLL-U serializer. A
    // one-token document whose string columns spell their own names makes the serializer's
    // tab-joined output enumerate the column order it emits.
    let token = ConlluToken {
        id: TokenId::Simple(1),
        form: "FORM".to_owned(),
        lemma: "LEMMA".to_owned(),
        upos: "UPOS".to_owned(),
        xpos: "XPOS".to_owned(),
        feats: "FEATS".to_owned(),
        head: "HEAD".to_owned(),
        deprel: "DEPREL".to_owned(),
        deps: "DEPS".to_owned(),
        misc: "MISC".to_owned(),
    };
    let doc = ConlluDoc {
        sentences: vec![ConlluSentence {
            comments: vec![],
            tokens: vec![token],
        }],
    };
    let text = String::from_utf8(serialize_conllu(&doc)).expect("CoNLL-U is UTF-8");
    let emitted: Vec<&str> = text
        .lines()
        .next()
        .expect("one token line")
        .split('\t')
        .collect();
    assert_eq!(emitted.len(), 10, "CoNLL-U emits exactly ten columns");
    assert_eq!(GMN_LANG_AST_COLUMNS[0], "ID", "column 0 is the ID slot");
    assert_eq!(
        emitted[0], "1",
        "the serializer emits the numeric ID in column 0"
    );
    assert_eq!(
        emitted[1..],
        GMN_LANG_AST_COLUMNS[1..],
        "the @λ column ruling must reuse the CoNLL-U column order verbatim (drift = build failure)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// (D) Runnable completeness invariant + per-tier reconciliation.
// ─────────────────────────────────────────────────────────────────────────────────────────

/// Every GMN row of the LANG-CONFORMANCE.md matrix: its failure-class local name, or the
/// [`BUILD_MARKER`] for the class-less build-assert row.
fn matrix_gmn_rows(md: &str) -> Vec<String> {
    let lines: Vec<&str> = md.lines().collect();
    // Locate the "### GMN dialect rules" section and read its table rows until the next
    // section heading.
    let start = lines
        .iter()
        .position(|l| l.trim_start().starts_with("### GMN dialect rules"))
        .expect("LANG-CONFORMANCE.md has a '### GMN dialect rules' section");
    let mut out = Vec::new();
    for line in &lines[start + 1..] {
        let t = line.trim_start();
        if t.starts_with("## ") {
            break; // next top-level section — end of the GMN subsection.
        }
        if !t.starts_with('|') {
            continue; // prose / blank line between heading and table.
        }
        // A markdown table row. Skip the header and the `|---|` separator.
        let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
        let last = cells.last().copied().unwrap_or_default();
        if last == "Failure class" || last.chars().all(|c| c == '-' || c.is_whitespace()) {
            continue;
        }
        if last == "`slice-quality.gmn-glyph-optimality.unaudited-executable-target` advisory" {
            // This row is the slice-quality ratchet over executable glyph coverage, not a
            // validator/SHACL/native/build failure class. Its discharge lives in the quality-axis
            // tests and therefore does not join the four conformance tiers partitioned here.
            continue;
        }
        if let Some(local) = extract_lang_local(last) {
            out.push(local);
        } else if last.contains(BUILD_MARKER) {
            out.push(BUILD_MARKER.to_owned());
        } else {
            panic!("GMN matrix row has an unrecognized failure-class cell: {last:?}");
        }
    }
    out
}

/// The `lang:Gmn*` failure classes the slice shapes enforce (via `gmeow:enforcesFailureClass`)
/// — the SHACL-tier ontology surface.
fn shape_gmn_classes() -> BTreeSet<String> {
    let (_shapes, shape_class) = load_shapes();
    shape_class
        .into_values()
        .map(|iri| local_name(&iri))
        .filter(|local| local.starts_with("Gmn"))
        .collect()
}

/// Enumerate the `gmn-*.ttl` counter-example fixtures on disk.
fn on_disk_counter_fixtures() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(counter_dir()).expect("counter-examples dir readable") {
        let name = entry.expect("dir entry").file_name();
        let name = name.to_str().expect("utf-8 filename");
        if name.starts_with("gmn-") && name.ends_with(".ttl") {
            out.insert(name.to_owned());
        }
    }
    out
}

#[test]
fn completeness_invariant_leaves_no_fixture_existence_only_row() {
    let md_conf = std::fs::read_to_string(lang_root().join("design").join("LANG-CONFORMANCE.md"))
        .expect("LANG-CONFORMANCE.md readable");
    let md_gmn = std::fs::read_to_string(lang_root().join("design").join("LANG-GMN.md"))
        .expect("LANG-GMN.md readable");

    let shacl = shacl_rows();

    // ── The DISCHARGED sets, built from what (A)/(B)/(C) actually assert. ────────────────
    let mut discharged_fixtures: BTreeSet<String> = BTreeSet::new();
    let mut discharged_classes: BTreeSet<String> = BTreeSet::new();

    // (A) validator tier.
    discharged_classes.extend(codec_classes());
    // (B) SHACL tier.
    let shacl_classes: BTreeSet<String> = shacl.iter().map(|r| r.class.to_owned()).collect();
    for row in &shacl {
        discharged_fixtures.insert(row.counter.to_owned());
        discharged_classes.insert(row.class.to_owned());
    }
    // (C) native tier (the silent-disambiguation lint AND the codebook-digest gate) + build
    // assert. The codebook-digest gate's counter-example lives under tests/gmn1-vectors/
    // negative-graph/, NOT the tests/counter-examples/ corpus part (1) reconciles, so only its
    // CLASS joins the discharged set here — it is discharged by execution in
    // `native_codebook_digest_row_discharges`, not by a counter-examples fixture.
    discharged_fixtures.insert(NATIVE_COUNTER.to_owned());
    discharged_classes.insert(NATIVE_CLASS.to_owned());
    discharged_classes.insert(NATIVE_GMN_GATE_CLASS.to_owned());
    discharged_classes.insert(BUILD_MARKER.to_owned());

    // ── (1) Every counter-example fixture on disk has an asserted discharge. ─────────────
    assert_eq!(
        discharged_fixtures,
        on_disk_counter_fixtures(),
        "every gmn-*.ttl counter-example must have an asserted discharge (no fixture-\
         existence-only rows), and every discharge must name a real fixture"
    );

    // ── The matrix rows, and every matrix class discharged. ──────────────────────────────
    let matrix_rows = matrix_gmn_rows(&md_conf);
    let matrix_classes: BTreeSet<String> = matrix_rows.iter().cloned().collect();
    assert_eq!(
        matrix_classes, discharged_classes,
        "every GMN matrix row must have an asserted discharge, and every discharge must name a \
         real matrix row"
    );

    // ── (2a) VALIDATOR tier: codec classes == doc INVALID labels ∪ the two residuals. ─────
    let codec = codec_classes();
    let mut doc_plus_residual = extract_invalid_blocks(&md_gmn)
        .keys()
        .cloned()
        .collect::<BTreeSet<String>>();
    assert_eq!(
        doc_plus_residual,
        doc_block_classes(),
        "the LANG-GMN.md INVALID blocks must name exactly the four validator classes"
    );
    // The two residual validator classes carry no normative INVALID block (they are driven from
    // synthetic inputs in `validator_tier_rows_discharge_via_production_codec`): the non-decodable
    // grammar residual and the honest default-graph domain boundary.
    doc_plus_residual.insert("GmnNonDecodableGrammar".to_owned());
    doc_plus_residual.insert("GmnGraphOutOfDomain".to_owned());
    assert_eq!(
        codec, doc_plus_residual,
        "the codec's six failure classes must set-equal the four doc INVALID labels plus the \
         two residuals GmnNonDecodableGrammar and GmnGraphOutOfDomain"
    );

    // ── (2b) SHACL tier: ontology enforcesFailureClass (Gmn*) == matrix SHACL rows. ───────
    let ontology_shacl = shape_gmn_classes();
    assert_eq!(
        ontology_shacl, shacl_classes,
        "the slice's Gmn* enforcesFailureClass set must set-equal the discharged SHACL rows"
    );

    // Partition the matrix rows by tier (by class). Two rows land in the native tier by class:
    // the SilentDisambiguation lint (whose gate cell also says 'Rust validator') and the
    // GmnCodebookDigestMismatch codebook-digest gate — neither a codec byte-parse class nor a
    // SHACL shape, so both are checked out of the codec/SHACL branches explicitly.
    let mut matrix_validator = BTreeSet::new();
    let mut matrix_shacl = BTreeSet::new();
    let mut matrix_native = BTreeSet::new();
    let mut matrix_build = BTreeSet::new();
    for class in &matrix_rows {
        if class == BUILD_MARKER {
            matrix_build.insert(class.clone());
        } else if class == NATIVE_CLASS || class == NATIVE_GMN_GATE_CLASS {
            matrix_native.insert(class.clone());
        } else if codec.contains(class) {
            matrix_validator.insert(class.clone());
        } else {
            matrix_shacl.insert(class.clone());
        }
    }
    assert_eq!(
        matrix_validator, codec,
        "the matrix validator-tier rows must set-equal the codec classes"
    );
    assert_eq!(
        matrix_shacl, ontology_shacl,
        "the matrix SHACL-tier rows must set-equal the ontology Gmn* enforcesFailureClass set"
    );
    assert_eq!(
        matrix_native,
        set_of(&[NATIVE_CLASS, NATIVE_GMN_GATE_CLASS]),
        "the matrix native tier is exactly lang:SilentDisambiguation (native lint) plus \
         lang:GmnCodebookDigestMismatch (native codebook-digest gate)"
    );
    assert_eq!(
        matrix_build,
        set_of(&[BUILD_MARKER]),
        "the matrix build-assert tier is exactly the class-less column-order row"
    );

    // ── (2c) WHOLE matrix: rows == disjoint union of the four tiers == discharged set. ────
    let tiers = [
        &matrix_validator,
        &matrix_shacl,
        &matrix_native,
        &matrix_build,
    ];
    let mut union = BTreeSet::new();
    for tier in tiers {
        for c in tier {
            assert!(
                union.insert(c.clone()),
                "the four GMN tiers must be DISJOINT, but {c} appears in two"
            );
        }
    }
    assert_eq!(
        union, matrix_classes,
        "the four tiers must partition the whole GMN matrix"
    );
    assert_eq!(
        union, discharged_classes,
        "the disjoint tier union must set-equal the discharged-class set"
    );
}
