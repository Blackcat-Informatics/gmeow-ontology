// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Structural gates over the `gts` slice's MEDIUM axis.
//!
//! The medium axis models a medium as a lawful `(encode, decode)` pair, splits the
//! AUTHORED dictionary definition from its GENERATED realization, and registers one
//! `gmeow:PayloadSchema` per blob representation the carrier can emit. These tests
//! hold the invariants that make that model total rather than aspirational:
//!
//! * every shipped dictionary resolves to exactly one corpus, and that corpus
//!   declares at least one selector (an unselected corpus leaves the training set
//!   undefined, so the bundle-internal guarantee could not be checked);
//! * the medium axis and the GMN DIALECT axis share no vocabulary;
//! * the slice ships no hand-authored `shapes.ttl` (validation is authored in
//!   `logic:` and DERIVED — a shapes file would be a second source of truth);
//! * every class the medium axis mints carries exactly one `logic:` UFO meta-type
//!   and a `gmeow:docsConcern`;
//! * every `REP_*` representation constant the carrier defines has a registered
//!   `gmeow:PayloadSchema`, so adding an archive without registering its schema
//!   reds here instead of shipping a payload with an undefined medium assignment.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";

/// The seven dictionaries the bundle ships, by `gmeow:dictionaryId`.
///
/// SEVEN, not eight. The inventory was first drafted from SLICE NAMES, and the rule
/// that decides it is that a dictionary is justified by the FRAME SET it primes and
/// must pay for its own in-band bytes on that set. Two of the drafts named frame sets
/// the bundle did not yet have, and the answer was to build them: the `lang:`
/// terminology surfaces and the statement layer's byte projections were already
/// opaque bytes on the general archive, so they moved onto their families' own reps
/// and their dictionaries are measured over the populations their names claim.
///
/// The eighth, `gmeow-math-v1`, is absent as a THEOREM rather than a measurement: a
/// dictionary primes a frame, `gmeow:payloadSchemaDictionary` is
/// `maxQualifiedCardinality 1`, and every `math:` named graph is unioned into the ONE
/// snapshot frame, which already binds `gmeow-core-v1`. No mathematical BYTE family
/// exists to give one instead, and manufacturing one by de-folding a named graph
/// would trade queryable structure for compression. The mathematical content is
/// primed in full by `gmeow-core-v1`, so nothing is lost.
const SHIPPED_DICTIONARY_IDS: [&str; 6] = [
    "gmeow-core-v1",
    "gmeow-lang-ast-v1",
    "gmeow-logic-v1",
    "gmeow-memory-compact-v1",
    "gmeow-memory-hot-v1",
    "gmeow-prooftrace-v1",
];

/// The classes the medium axis mints. Each must carry exactly one `logic:` UFO
/// meta-type and a `gmeow:docsConcern`.
const MEDIUM_AXIS_CLASSES: [&str; 19] = [
    "CompressionDictionary",
    "CompressionDictionaryRealization",
    "CorpusTrainingSplit",
    "DictionaryCorpus",
    "DictionaryStrategy",
    "DigestStratum",
    "GtsConformanceFailure",
    "Medium",
    "MediumCorpusDrift",
    "MediumDictionaryRegression",
    "MediumDigestMismatch",
    "MediumEnvelope",
    "MediumOpaqueFrame",
    "MediumSourceKind",
    "MediumUndeclaredDictionary",
    "MediumUnknownDictionary",
    "MediumUnknownSchema",
    "PayloadSchema",
    "ZstdDictMedium",
];

/// The `logic:` UFO meta-types a GMEOW class may be punned with (the same closed set
/// `gmeow_validate::gufo`'s stereotype discipline recognizes).
const UFO_META_TYPES: [&str; 11] = [
    "AbstractIndividualType",
    "Category",
    "Event",
    "Kind",
    "Mixin",
    "Phase",
    "PhaseMixin",
    "Role",
    "RoleMixin",
    "Situation",
    "SubKind",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn gts_module_path() -> PathBuf {
    repo_root().join("slices/core/gts/module.ttl")
}

fn gts_module_text() -> String {
    std::fs::read_to_string(gts_module_path()).expect("gts module.ttl readable")
}

fn gts_module() -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(gts_module_text().as_bytes(), "text/turtle", Some(GMEOW))
        .expect("gts module.ttl parses as Turtle")
}

fn id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Every object of `subject predicate ?o`, as a term-id list.
fn objects(ds: &RdfDataset, subject: TermId, predicate: &str) -> Vec<TermId> {
    let Some(p) = id(ds, predicate) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(subject), Some(p), None, GraphMatch::Any)
        .map(|q| q.o)
        .collect()
}

/// Every subject typed `class_iri`.
fn instances(ds: &RdfDataset, class_iri: &str) -> Vec<TermId> {
    let (Some(t), Some(c)) = (id(ds, RDF_TYPE), id(ds, class_iri)) else {
        return Vec::new();
    };
    ds.quads_for_pattern(None, Some(t), Some(c), GraphMatch::Any)
        .map(|q| q.s)
        .collect()
}

fn iri_of(ds: &RdfDataset, term: TermId) -> String {
    match ds.resolve(term) {
        TermRef::Iri(iri) => iri.to_owned(),
        other => panic!("expected an IRI term, got {other:?}"),
    }
}

fn literal_of(ds: &RdfDataset, term: TermId) -> String {
    match ds.resolve(term) {
        TermRef::Literal { lexical, .. } => lexical.to_string(),
        other => panic!("expected a literal term, got {other:?}"),
    }
}

// --------------------------------------------------------------------------- //
// (a) Every shipped dictionary resolves to exactly one corpus with a selector.
// --------------------------------------------------------------------------- //

#[test]
fn every_shipped_dictionary_resolves_to_one_corpus_with_at_least_one_selector() {
    let ds = gts_module();
    let selectors = [
        gm("corpusSelectsBlobRep"),
        gm("corpusSelectsGraph"),
        gm("corpusSelectsPathPrefix"),
        gm("corpusSelectsStageProduct"),
    ];

    // dictionaryId -> dictionary IRI, over every authored gmeow:CompressionDictionary.
    let mut by_id: BTreeMap<String, TermId> = BTreeMap::new();
    for dict in instances(&ds, &gm("CompressionDictionary")) {
        let ids = objects(&ds, dict, &gm("dictionaryId"));
        assert_eq!(
            ids.len(),
            1,
            "{} must carry exactly one gmeow:dictionaryId",
            iri_of(&ds, dict)
        );
        let previous = by_id.insert(literal_of(&ds, ids[0]), dict);
        assert!(previous.is_none(), "duplicate gmeow:dictionaryId");
    }

    let found: BTreeSet<&str> = by_id.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = SHIPPED_DICTIONARY_IDS.into_iter().collect();
    assert_eq!(
        found.len(),
        SHIPPED_DICTIONARY_IDS.len(),
        "the gts slice must declare exactly {} dictionaries; got {found:?}",
        SHIPPED_DICTIONARY_IDS.len()
    );
    assert_eq!(
        found, expected,
        "the gts slice must declare exactly the shipped dictionary inventory"
    );

    for (dictionary_id, dict) in &by_id {
        let corpora = objects(&ds, *dict, &gm("trainsOverCorpus"));
        assert_eq!(
            corpora.len(),
            1,
            "{dictionary_id} must gmeow:trainsOverCorpus exactly one gmeow:DictionaryCorpus"
        );
        let corpus = corpora[0];
        let corpus_iri = iri_of(&ds, corpus);

        // The corpus is a declared gmeow:DictionaryCorpus, not an untyped stand-in.
        assert!(
            instances(&ds, &gm("DictionaryCorpus")).contains(&corpus),
            "{dictionary_id}'s corpus {corpus_iri} must be a declared gmeow:DictionaryCorpus"
        );

        let selector_count: usize = selectors
            .iter()
            .map(|p| objects(&ds, corpus, p).len())
            .sum();
        assert!(
            selector_count >= 1,
            "{dictionary_id}'s corpus {corpus_iri} declares no selector — its training set \
             would be undefined and the bundle-internal guarantee uncheckable"
        );
    }
}

// --------------------------------------------------------------------------- //
// (a2) EXACTLY ONE held-out split governs EVERY archive-backed corpus.
// --------------------------------------------------------------------------- //

/// The split is declared ONCE, is a proper partition, and carries no per-corpus
/// override.
///
/// One individual rather than one per corpus is the whole point: a per-corpus knob
/// would be a per-dictionary carve-out with extra steps, and the dictionary whose
/// evaluation most needs an unseen member is exactly the one whose author would be
/// tempted to widen its own training set. So this pins BOTH halves — that a split
/// exists at all, and that no corpus carries a second one.
#[test]
fn exactly_one_held_out_split_governs_every_archive_backed_corpus() {
    let ds = gts_module();

    let splits = instances(&ds, &gm("CorpusTrainingSplit"));
    assert_eq!(
        splits.len(),
        1,
        "the gts slice must declare EXACTLY ONE gmeow:CorpusTrainingSplit — zero would leave \
         every archive-backed dictionary trained on the bytes it is scored over, and two would \
         leave 'which members did this dictionary never see' with two answers"
    );
    let split = splits[0];

    let stride: u64 = {
        let values = objects(&ds, split, &gm("splitHeldOutStride"));
        assert_eq!(values.len(), 1, "exactly one gmeow:splitHeldOutStride");
        literal_of(&ds, values[0]).parse().expect("an integer")
    };
    let offset: u64 = {
        let values = objects(&ds, split, &gm("splitHeldOutOffset"));
        assert_eq!(values.len(), 1, "exactly one gmeow:splitHeldOutOffset");
        literal_of(&ds, values[0]).parse().expect("an integer")
    };
    assert!(
        stride >= 2,
        "a stride below 2 holds out every member, leaving no training set"
    );
    assert!(
        offset < stride,
        "an offset at or above the stride can never be hit, so the split would hold nothing out \
         while still claiming to"
    );

    // No corpus carries its own split coordinates: the rule is uniform by
    // construction, not by convention.
    for corpus in instances(&ds, &gm("DictionaryCorpus")) {
        for predicate in [gm("splitHeldOutStride"), gm("splitHeldOutOffset")] {
            assert!(
                objects(&ds, corpus, &predicate).is_empty(),
                "{} carries its own split coordinate — the split is ONE rule over EVERY \
                 archive-backed corpus, and a per-corpus value is a per-dictionary carve-out",
                iri_of(&ds, corpus)
            );
        }
    }

    // …and at least one shipped corpus is archive-backed, so the split governs
    // something rather than being decoration.
    let archive_backed = instances(&ds, &gm("DictionaryCorpus"))
        .into_iter()
        .filter(|corpus| !objects(&ds, *corpus, &gm("corpusSelectsBlobRep")).is_empty())
        .count();
    assert!(
        archive_backed > 0,
        "no shipped corpus selects a blob rep — the held-out split would govern nothing"
    );
}

// --------------------------------------------------------------------------- //
// (b) The medium axis and the GMN dialect axis share no vocabulary.
// --------------------------------------------------------------------------- //

#[test]
fn gts_module_declares_no_gmn_terms() {
    let ds = gts_module();
    let mut offenders: BTreeSet<String> = BTreeSet::new();
    for quad in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        for term in [quad.s, quad.p, quad.o] {
            if let TermRef::Iri(iri) = ds.resolve(term)
                && let Some(local) = iri.strip_prefix(GMEOW)
                && (local.starts_with("gmn") || local.starts_with("Gmn"))
            {
                offenders.insert(iri.to_owned());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the gts slice must declare zero gmeow:gmn* terms — the medium axis and the GMN \
         dialect axis share no vocabulary; found {offenders:?}"
    );
}

// --------------------------------------------------------------------------- //
// (c) The slice ships no hand-authored shapes.ttl.
// --------------------------------------------------------------------------- //

#[test]
fn gts_slice_ships_no_hand_authored_shapes_file() {
    let shapes = repo_root().join("slices/core/gts/shapes.ttl");
    assert!(
        !shapes.exists(),
        "slices/core/gts/shapes.ttl must not exist — declarative obligations are EL-safe \
         logic: restrictions in module.ttl and procedural ones are logic:Constraint + \
         logic:Formula; a shapes file would be a forbidden second source of truth"
    );
    // Belt and braces: not one SHACL term appears in the PARSED module. A text scan
    // would trip over prose mentioning the derived surface, so the check is over the
    // triples themselves — exactly what the projection-vocabulary ratchet counts.
    const SHACL: &str = "http://www.w3.org/ns/shacl#";
    let ds = gts_module();
    let mut shacl_terms: BTreeSet<String> = BTreeSet::new();
    for quad in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        for term in [quad.s, quad.p, quad.o] {
            if let TermRef::Iri(iri) = ds.resolve(term)
                && iri.starts_with(SHACL)
            {
                shacl_terms.insert(iri.to_owned());
            }
        }
    }
    assert!(
        shacl_terms.is_empty(),
        "slices/core/gts/module.ttl must hand-author no SHACL vocabulary — the shape \
         surface is DERIVED from the logic: restrictions and logic:Constraints; found \
         {shacl_terms:?}"
    );
}

// --------------------------------------------------------------------------- //
// (d) Every medium-axis class carries its logic: meta-type and docsConcern.
// --------------------------------------------------------------------------- //

#[test]
fn every_medium_axis_class_carries_a_ufo_meta_type_and_a_docs_concern() {
    let ds = gts_module();
    let meta_types: BTreeSet<String> = UFO_META_TYPES
        .into_iter()
        .map(|local| format!("{LOGIC}{local}"))
        .collect();

    for local in MEDIUM_AXIS_CLASSES {
        let iri = gm(local);
        let subject = id(&ds, &iri).unwrap_or_else(|| panic!("gmeow:{local} is not declared"));

        let types: BTreeSet<String> = objects(&ds, subject, RDF_TYPE)
            .into_iter()
            .map(|t| iri_of(&ds, t))
            .collect();
        assert!(
            types.contains(OWL_CLASS),
            "gmeow:{local} must be declared an owl:Class"
        );

        let stereotypes: Vec<&String> = types.intersection(&meta_types).collect();
        assert_eq!(
            stereotypes.len(),
            1,
            "gmeow:{local} must carry EXACTLY ONE logic: UFO meta-type (OntoUML stereotype \
             discipline); found {stereotypes:?}"
        );

        let concerns = objects(&ds, subject, &gm("docsConcern"));
        assert!(
            !concerns.is_empty(),
            "gmeow:{local} must carry a gmeow:docsConcern"
        );
    }
}

// --------------------------------------------------------------------------- //
// (e) Every carrier REP_* constant has a registered gmeow:PayloadSchema.
// --------------------------------------------------------------------------- //

/// Every `REP_*: &str = "…"` representation constant defined anywhere under the
/// carrier crate (`crates/pipeline/src`), keyed by constant name.
///
/// The carrier is the single producer of the bundle's blob channel, so its `REP_*`
/// constants ARE the closed set of representations a `gmeow.gts` can carry. Reading
/// them from the source keeps this test free of a `gmeow-validate` → `gmeow-pipeline`
/// crate edge (the pipeline depends on validate, so the reverse edge would be a cycle)
/// while still failing the moment a new representation is added without a schema.
fn carrier_rep_constants() -> BTreeMap<String, String> {
    let src = repo_root().join("crates/pipeline/src");
    let mut out = BTreeMap::new();
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("carrier source directory readable") {
            let entry = entry.expect("readable directory entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("carrier source file readable");
            for line in text.lines() {
                let line = line.trim();
                let Some(rest) = line.strip_prefix("pub const REP_") else {
                    let Some(rest) = line
                        .strip_prefix("pub(crate) const REP_")
                        .or_else(|| line.strip_prefix("const REP_"))
                    else {
                        continue;
                    };
                    record_rep_constant(&mut out, rest);
                    continue;
                };
                record_rep_constant(&mut out, rest);
            }
        }
    }
    assert!(
        !out.is_empty(),
        "found no REP_* constants under {} — the scan is broken, not the registry",
        src.display()
    );
    out
}

/// Parse the tail of a `… const REP_<NAME>: &str = "<value>";` line into the registry.
fn record_rep_constant(out: &mut BTreeMap<String, String>, rest: &str) {
    let Some((name, tail)) = rest.split_once(':') else {
        return;
    };
    let Some(open) = tail.find('"') else {
        return;
    };
    let value_start = open + 1;
    let Some(close) = tail[value_start..].find('"') else {
        return;
    };
    out.insert(
        format!("REP_{}", name.trim()),
        tail[value_start..value_start + close].to_owned(),
    );
}

#[test]
fn every_carrier_blob_rep_has_a_registered_payload_schema() {
    let ds = gts_module();

    let registered: BTreeSet<String> = instances(&ds, &gm("PayloadSchema"))
        .into_iter()
        .flat_map(|schema| {
            let ids = objects(&ds, schema, &gm("payloadSchemaId"));
            assert_eq!(
                ids.len(),
                1,
                "{} must carry exactly one gmeow:payloadSchemaId",
                iri_of(&ds, schema)
            );
            ids.into_iter().map(|t| literal_of(&ds, t))
        })
        .collect();

    let constants = carrier_rep_constants();
    // Guard the SCAN itself: a regressed parser that found only a handful of constants
    // would let both directions of this test pass vacuously. These six span the three
    // visibilities (`pub`, `pub(crate)`, private) and three different carrier modules,
    // so losing any one of them means the scan stopped seeing a whole shape of constant.
    for required in [
        "REP_MAPPINGS",
        "REP_SHAPES",
        "REP_DENIED",
        "REP_GENERATED",
        "REP_LANG_SURFACE",
        "REP_SHACL_SARIF",
    ] {
        assert!(
            constants.contains_key(required),
            "the carrier REP_* scan missed {required} — the scan is broken, not the registry"
        );
    }

    let mut missing: Vec<String> = Vec::new();
    for (name, rep) in &constants {
        if !registered.contains(rep) {
            missing.push(format!("{name} = {rep:?}"));
        }
    }
    assert!(
        missing.is_empty(),
        "every blob representation the carrier can emit needs a gmeow:PayloadSchema \
         individual carrying its gmeow:payloadSchemaId — otherwise the rep decodes as an \
         unclassified blob with an UNDEFINED medium assignment. Unregistered: {missing:?}"
    );

    // The snapshot wire schema has no REP_* constant (it is the payload the blob channel
    // rides IN, not a blob), so it is registered explicitly and must stay registered.
    assert!(
        registered.contains("gmeow:snapshot/wire"),
        "the snapshot wire schema must be registered — it is the self-referential envelope \
         that motivates gmeow:envelopeDigestStratum"
    );
}

#[test]
fn the_payload_schema_registry_carries_no_labels_the_carrier_never_emits() {
    let ds = gts_module();
    let emitted: BTreeSet<String> = carrier_rep_constants().into_values().collect();

    let mut orphans: Vec<String> = Vec::new();
    for schema in instances(&ds, &gm("PayloadSchema")) {
        for label in objects(&ds, schema, &gm("payloadSchemaId")) {
            let label = literal_of(&ds, label);
            // The snapshot wire schema is the one deliberate non-blob registration.
            if label != "gmeow:snapshot/wire" && !emitted.contains(&label) {
                orphans.push(label);
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "the payload-schema registry must not carry labels no carrier REP_* constant emits \
         — a stale registration hides a removed archive: {orphans:?}"
    );
}
