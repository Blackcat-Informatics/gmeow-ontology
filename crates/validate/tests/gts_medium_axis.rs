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

use serde::Deserialize;
use std::sync::OnceLock;

#[path = "source_observations/reader.rs"]
mod observation_reader;

#[derive(Deserialize)]
struct Observations {
    source_path: String,
    source_digest: String,
    dictionaries: Vec<Node>,
    corpora: Vec<Node>,
    splits: Vec<Node>,
    schemas: Vec<Node>,
    classes: BTreeMap<String, Option<Node>>,
    gmn_terms: BTreeSet<String>,
    shacl_terms: BTreeSet<String>,
}

#[derive(Deserialize)]
struct Node {
    name: Value,
    values: BTreeMap<String, Vec<Value>>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
enum Value {
    Iri(String),
    Literal(String),
    Other(String),
}

impl Value {
    fn iri(&self) -> &str {
        match self {
            Self::Iri(iri) => iri,
            other => panic!("expected an IRI term, got {other:?}"),
        }
    }
    fn literal(&self) -> &str {
        match self {
            Self::Literal(lexical) => lexical,
            other => panic!("expected a literal term, got {other:?}"),
        }
    }
}

impl Node {
    fn objects(&self, predicate: &str) -> &[Value] {
        self.values
            .get(predicate)
            .unwrap_or_else(|| panic!("producer omitted selected field {predicate}"))
            .as_slice()
    }
}

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LOGIC_CLASS: &str = "https://blackcatinformatics.ca/logic/Class";

/// The six shipped dictionaries, by `gmeow:dictionaryId`.
/// Each dictionary is justified by the frame population it primes.
///
/// `gmeow-math-v1` is absent because a
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

fn gts_module() -> &'static Observations {
    static OBSERVED: OnceLock<gmeow_errors::Result<observation_reader::Selected<Observations>>> =
        OnceLock::new();
    let observed = observation_reader::selected(
        &OBSERVED,
        "stage-conformance",
        "pipeline/medium-axis-observations.json",
    );
    assert_eq!(observed.source_path, "slices/core/gts/module.ttl");
    assert!(
        observed.source_digest.len() == 64
            && observed
                .source_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "exact original medium source identity"
    );
    observed
}

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

// --------------------------------------------------------------------------- //
// (a) Every shipped dictionary resolves to exactly one corpus with a selector.
// --------------------------------------------------------------------------- //

fn every_shipped_dictionary_resolves_to_one_corpus_with_at_least_one_selector() {
    let observed = gts_module();
    let mut by_id = BTreeMap::new();
    for dictionary in &observed.dictionaries {
        let ids = dictionary.objects(&gm("dictionaryId"));
        assert_eq!(
            ids.len(),
            1,
            "{} must carry exactly one gmeow:dictionaryId",
            dictionary.name.iri()
        );
        assert!(
            by_id.insert(ids[0].literal(), dictionary).is_none(),
            "duplicate gmeow:dictionaryId"
        );
    }
    let found: BTreeSet<_> = by_id.keys().copied().collect();
    assert_eq!(
        found.len(),
        SHIPPED_DICTIONARY_IDS.len(),
        "the exact shipped dictionary count"
    );
    assert_eq!(
        found,
        SHIPPED_DICTIONARY_IDS.into_iter().collect(),
        "the exact shipped dictionary inventory"
    );
    for (dictionary_id, dictionary) in by_id {
        let corpora = dictionary.objects(&gm("trainsOverCorpus"));
        assert_eq!(
            corpora.len(),
            1,
            "{dictionary_id} must train over exactly one corpus"
        );
        let corpus_iri = corpora[0].iri();
        let corpus = observed
            .corpora
            .iter()
            .find(|corpus| corpus.name == corpora[0])
            .unwrap_or_else(|| {
                panic!("{dictionary_id}'s corpus {corpus_iri} must be a declared DictionaryCorpus")
            });
        let selectors: usize = [
            "corpusSelectsBlobRep",
            "corpusSelectsGraph",
            "corpusSelectsPathPrefix",
            "corpusSelectsStageProduct",
        ]
        .into_iter()
        .map(|predicate| corpus.objects(&gm(predicate)).len())
        .sum();
        assert!(
            selectors >= 1,
            "{dictionary_id}'s corpus {corpus_iri} declares no selector"
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
fn exactly_one_held_out_split_governs_every_archive_backed_corpus() {
    let observed = gts_module();
    assert_eq!(
        observed.splits.len(),
        1,
        "exactly one held-out split must govern every archive-backed dictionary"
    );
    let split = &observed.splits[0];
    let stride = split.objects(&gm("splitHeldOutStride"));
    let offset = split.objects(&gm("splitHeldOutOffset"));
    assert_eq!(stride.len(), 1, "exactly one held-out stride");
    assert_eq!(offset.len(), 1, "exactly one held-out offset");
    let stride: u64 = stride[0].literal().parse().expect("an integer");
    let offset: u64 = offset[0].literal().parse().expect("an integer");
    assert!(stride >= 2, "a stride below two leaves no training set");
    assert!(
        offset < stride,
        "an offset outside the stride holds out nothing"
    );
    for corpus in &observed.corpora {
        for property in ["splitHeldOutStride", "splitHeldOutOffset"] {
            assert!(
                corpus.objects(&gm(property)).is_empty(),
                "{} carries a per-corpus split override",
                corpus.name.iri()
            );
        }
    }
    assert!(
        observed
            .corpora
            .iter()
            .any(|corpus| !corpus.objects(&gm("corpusSelectsBlobRep")).is_empty()),
        "the held-out split must govern an archive-backed corpus"
    );
}

// --------------------------------------------------------------------------- //
// (b) The medium axis and the GMN dialect axis share no vocabulary.
// --------------------------------------------------------------------------- //

fn gts_module_declares_no_gmn_terms() {
    let offenders = &gts_module().gmn_terms;
    assert!(
        offenders.is_empty(),
        "the medium and GMN dialect axes share no vocabulary; found {offenders:?}"
    );
}

// --------------------------------------------------------------------------- //
// (c) The slice ships no hand-authored shapes.ttl.
// --------------------------------------------------------------------------- //

fn gts_slice_ships_no_hand_authored_shapes_file() {
    assert!(
        !repo_root().join("slices/core/gts/shapes.ttl").exists(),
        "the gts slice must author validation only through logic: and derive its shapes"
    );
    let shacl_terms = &gts_module().shacl_terms;
    assert!(
        shacl_terms.is_empty(),
        "the gts module must author no SHACL vocabulary; found {shacl_terms:?}"
    );
}

// --------------------------------------------------------------------------- //
// (d) Every medium-axis class carries its logic: meta-type and docsConcern.
// --------------------------------------------------------------------------- //

fn every_medium_axis_class_carries_a_ufo_meta_type_and_a_docs_concern() {
    let observed = gts_module();
    let meta_types: BTreeSet<String> = UFO_META_TYPES
        .into_iter()
        .map(|local| format!("{LOGIC}{local}"))
        .collect();
    assert_eq!(
        observed.classes.keys().cloned().collect::<BTreeSet<_>>(),
        MEDIUM_AXIS_CLASSES.into_iter().map(gm).collect()
    );
    for local in MEDIUM_AXIS_CLASSES {
        let declaration = observed.classes[&gm(local)]
            .as_ref()
            .unwrap_or_else(|| panic!("gmeow:{local} is not declared"));
        let types: BTreeSet<String> = declaration
            .objects(RDF_TYPE)
            .iter()
            .map(|term| term.iri().to_owned())
            .collect();
        assert!(
            types.contains(LOGIC_CLASS),
            "gmeow:{local} must be a logic:Class"
        );
        let stereotypes: Vec<_> = types.intersection(&meta_types).collect();
        assert_eq!(
            stereotypes.len(),
            1,
            "gmeow:{local} must carry exactly one logic: UFO meta-type; found {stereotypes:?}"
        );
        assert!(
            !declaration.objects(&gm("docsConcern")).is_empty(),
            "gmeow:{local} must carry a docsConcern"
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
    // BOTH carrier crates: `gmeow-bundle-view` OWNS the representation ids the read side
    // addresses, and `gmeow-pipeline` defines the write-side-only ones. Scanning just one
    // of them is how this check silently stopped seeing REP_DENIED — a scan that misses a
    // constant reports a clean registry rather than an unregistered payload.
    let roots = [
        repo_root().join("crates/pipeline/src"),
        repo_root().join("crates/bundle-view/src"),
    ];
    let mut out = BTreeMap::new();
    let mut stack: Vec<_> = roots.to_vec();
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
        "found no REP_* constants under {roots:?} — the scan is broken, not the registry"
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

fn every_carrier_blob_rep_has_a_registered_payload_schema() {
    let observed = gts_module();

    let registered: BTreeSet<String> = observed
        .schemas
        .iter()
        .flat_map(|schema| {
            let ids = schema.objects(&gm("payloadSchemaId"));
            assert_eq!(
                ids.len(),
                1,
                "{} must carry exactly one gmeow:payloadSchemaId",
                schema.name.iri()
            );
            ids.iter().map(|term| term.literal().to_owned())
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

fn the_payload_schema_registry_carries_no_labels_the_carrier_never_emits() {
    let observed = gts_module();
    let emitted: BTreeSet<String> = carrier_rep_constants().into_values().collect();

    let mut orphans: Vec<String> = Vec::new();
    for schema in &observed.schemas {
        for label in schema.objects(&gm("payloadSchemaId")) {
            let label = label.literal();
            // The snapshot wire schema is the one deliberate non-blob registration.
            if label != "gmeow:snapshot/wire" && !emitted.contains(label) {
                orphans.push(label.to_owned());
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "the payload-schema registry must not carry labels no carrier REP_* constant emits \
         — a stale registration hides a removed archive: {orphans:?}"
    );
}

#[test]
fn seven_medium_axis_contracts_share_one_authenticated_observation() {
    let contracts: [(&str, fn()); 7] = [
        (
            "every_shipped_dictionary_resolves_to_one_corpus_with_at_least_one_selector",
            every_shipped_dictionary_resolves_to_one_corpus_with_at_least_one_selector,
        ),
        (
            "exactly_one_held_out_split_governs_every_archive_backed_corpus",
            exactly_one_held_out_split_governs_every_archive_backed_corpus,
        ),
        (
            "gts_module_declares_no_gmn_terms",
            gts_module_declares_no_gmn_terms,
        ),
        (
            "gts_slice_ships_no_hand_authored_shapes_file",
            gts_slice_ships_no_hand_authored_shapes_file,
        ),
        (
            "every_medium_axis_class_carries_a_ufo_meta_type_and_a_docs_concern",
            every_medium_axis_class_carries_a_ufo_meta_type_and_a_docs_concern,
        ),
        (
            "every_carrier_blob_rep_has_a_registered_payload_schema",
            every_carrier_blob_rep_has_a_registered_payload_schema,
        ),
        (
            "the_payload_schema_registry_carries_no_labels_the_carrier_never_emits",
            the_payload_schema_registry_carries_no_labels_the_carrier_never_emits,
        ),
    ];
    assert_eq!(
        contracts
            .iter()
            .map(|(name, _)| *name)
            .collect::<BTreeSet<_>>()
            .len(),
        7
    );
    let mut failures = Vec::new();
    for (name, contract) in contracts {
        if let Err(payload) = std::panic::catch_unwind(contract) {
            let detail = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "non-string assertion panic".to_owned());
            failures.push(format!("{name}: {detail}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 7 medium contracts failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
