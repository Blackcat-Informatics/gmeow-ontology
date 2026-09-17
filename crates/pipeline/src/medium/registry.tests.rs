// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn registry(extra: &str) -> MediumRegistry {
    MediumRegistry::from_dataset(&fixture::dataset(extra)).expect("fixture registry")
}

fn error(extra: &str) -> gmeow_errors::Diag {
    MediumRegistry::from_dataset(&fixture::dataset(extra))
        .expect_err("the fixture addition must be rejected")
}

#[test]
fn reads_dictionaries_corpora_schemas_media_and_the_assignment() {
    let registry = registry("");
    assert_eq!(registry.dictionaries().len(), 2);
    let core = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("registered by id");
    assert_eq!(core.version, "1");
    assert_eq!(core.strategy, DictionaryStrategy::Trained);
    assert_eq!(core.target_length, 4096);
    assert_eq!(core.corpus, gm("corpusCore"));
    assert_eq!(
        registry
            .dictionary_by_id("gmeow-terms-v1")
            .expect("registered")
            .strategy,
        DictionaryStrategy::TermTable
    );

    assert_eq!(registry.corpora().len(), 2);
    assert_eq!(registry.schemas().len(), 3);
    assert_eq!(registry.media().len(), 2);

    let dist = registry.media().get(&gm("mediumDist")).expect("declared");
    assert_eq!(dist.zstd_level, 12);
    assert_eq!(dist.source_kind, MediumSourceKind::PerRep);
    assert_eq!(dist.dictionaries.len(), 2);
    assert!(dist.reader_capabilities.contains("zstd-dictionary"));

    let cells = registry.assignment_for("cells-archive").expect("assigned");
    assert_eq!(cells.dictionary, DictSelection::Named(gm("dictCore")));
    // The baseline medium declares NO dictionary, and that IS its selection.
    let snapshot = registry
        .assignment_for(SNAPSHOT_WIRE_REP)
        .expect("assigned");
    assert_eq!(snapshot.dictionary, DictSelection::Baseline);
}

/// A rep with no registered schema and a registered rep with no assignment are
/// DIFFERENT defects with different fixes, so they raise different classes.
#[test]
fn an_unknown_rep_and_an_unassigned_rep_raise_different_failures() {
    let registry = registry("");
    assert_eq!(
        registry
            .assignment_for("never-registered")
            .expect_err("unregistered rep")
            .code(),
        crate::error::MediumUnknownSchema::register()
    );
    assert_eq!(
        registry
            .assignment_for("orphan-archive")
            .expect_err("registered but unassigned rep")
            .code(),
        crate::error::MediumUndeclaredDictionary::register()
    );
}

/// A rep assigned a dictionary-declaring medium but selecting no dictionary is
/// `MediumUndeclaredDictionary` — the baseline exemption is scoped to a medium
/// that declares an EMPTY dictionary set, not to "any schema that forgot the
/// field".
#[test]
fn an_assigned_rep_with_no_dictionary_selection_is_undeclared() {
    let diag = error("gmeow:payloadSchemaOrphan gmeow:payloadSchemaMedium gmeow:mediumDist .");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
}

/// A dictionary the medium does not declare is outside the medium's BOUND.
#[test]
fn a_rep_selecting_outside_the_medium_bound_is_unknown() {
    let diag = error(
        "gmeow:dictStray a gmeow:CompressionDictionary ;\n\
             \x20   gmeow:dictionaryId \"stray-v1\" ; gmeow:dictionaryVersion \"1\" ;\n\
             \x20   gmeow:dictionaryStrategy gmeow:dictStrategyTrained ;\n\
             \x20   gmeow:dictionaryTargetLength 4096 ;\n\
             \x20   gmeow:trainsOverCorpus gmeow:corpusCore .\n\
             gmeow:payloadSchemaOrphan gmeow:payloadSchemaMedium gmeow:mediumDist ;\n\
             \x20   gmeow:payloadSchemaDictionary gmeow:dictStray .",
    );
    assert_eq!(
        diag.code(),
        crate::error::MediumUnknownDictionary::register(),
        "{diag}"
    );
}

/// Two `gmeow:payloadSchemaMedium` values on one schema leave that rep's medium
/// underivable — which is exactly the per-call decision the axis exists to
/// remove, so it is rejected rather than resolved by precedence.
#[test]
fn a_rep_assigned_two_media_is_rejected() {
    let diag = error("gmeow:payloadSchemaCells gmeow:payloadSchemaMedium gmeow:mediumBaseline .");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
    assert!(
        diag.to_string()
            .contains("2 gmeow:payloadSchemaMedium values"),
        "{diag}"
    );
}

/// A rep whose assignment names an undeclared medium has no round-trip law.
#[test]
fn a_rep_assigned_an_undeclared_medium_is_rejected() {
    let diag = error("gmeow:payloadSchemaOrphan gmeow:payloadSchemaMedium gmeow:mediumInvented .");
    assert_eq!(
        diag.code(),
        crate::error::InvalidDeclaration::register(),
        "{diag}"
    );
    assert!(diag.to_string().contains("mediumInvented"), "{diag}");
}

/// An exactly-one field that is missing has nothing to default to.
#[test]
fn a_dictionary_missing_an_exactly_one_field_is_rejected() {
    let diag = error(
        "gmeow:dictNoVersion a gmeow:CompressionDictionary ;\n\
             \x20   gmeow:dictionaryId \"no-version-v1\" ;\n\
             \x20   gmeow:dictionaryStrategy gmeow:dictStrategyTrained ;\n\
             \x20   gmeow:dictionaryTargetLength 4096 ;\n\
             \x20   gmeow:trainsOverCorpus gmeow:corpusCore .",
    );
    assert!(diag.to_string().contains("dictionaryVersion"), "{diag}");
}

/// An unrecognized strategy individual is a hard fail, not a fallback to trained.
#[test]
fn an_unrecognized_strategy_individual_is_rejected() {
    let diag = error(
        "gmeow:dictWeird a gmeow:CompressionDictionary ;\n\
             \x20   gmeow:dictionaryId \"weird-v1\" ; gmeow:dictionaryVersion \"1\" ;\n\
             \x20   gmeow:dictionaryStrategy gmeow:dictStrategyInvented ;\n\
             \x20   gmeow:dictionaryTargetLength 4096 ;\n\
             \x20   gmeow:trainsOverCorpus gmeow:corpusCore .",
    );
    assert!(diag.to_string().contains("dictStrategyInvented"), "{diag}");
}

/// The trained bytes of every declared fixture dictionary — the plan pins them
/// ALL, so a plan test must supply them all.
fn trained_all() -> BTreeMap<String, Vec<u8>> {
    [
        ("gmeow-core-v1".to_string(), vec![1, 2, 3]),
        ("gmeow-terms-v1".to_string(), vec![4, 5]),
    ]
    .into()
}

#[test]
fn the_medium_plan_renders_the_assignment_for_the_authorship_door() {
    let registry = registry("");
    let plan = registry
        .medium_plan_under(
            &MediumSelection::Authored,
            &["cells-archive".to_string()],
            &trained_all(),
        )
        .expect("plan");
    assert_eq!(plan.zstd_level, Some(12));
    // EVERY declared dictionary is pinned, not merely the selected one: the
    // pack is the distribution channel for the dictionary family, so a
    // declared-but-unselected dictionary must still be obtainable from it.
    assert_eq!(
        plan.dicts,
        vec![
            ("gmeow-core-v1".to_string(), vec![1, 2, 3]),
            ("gmeow-terms-v1".to_string(), vec![4, 5]),
        ]
    );
    assert_eq!(
        plan.assignment
            .get(&FrameSlot::Blob("cells-archive".into())),
        Some(&WireDictSelection::Named("gmeow-core-v1".into()))
    );
    // The snapshot slot is read from the registry, not defaulted.
    assert_eq!(
        plan.assignment.get(&FrameSlot::Snapshot),
        Some(&WireDictSelection::Baseline)
    );
}

/// A rep the emission actually authors but the registry does not assign is a
/// HARD FAIL at plan time — the point where it is still fixable, rather than at
/// decode time on a shipped artifact.
#[test]
fn the_medium_plan_hard_fails_on_an_unassigned_rep() {
    let registry = registry("");
    let diag = registry
        .medium_plan_under(
            &MediumSelection::Authored,
            &["orphan-archive".to_string()],
            &trained_all(),
        )
        .expect_err("an unassigned rep must not produce a plan");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
    assert!(
        diag.to_string().contains("orphan-archive"),
        "the failure names the unassigned rep, not a missing dictionary: {diag}"
    );
}

/// The dictionary ids `slices/core/gts/module.ttl` ships, spelled out so the
/// inventory is pinned in BOTH directions: a dropped dictionary and an added one
/// each fail [`the_live_gts_slice_reads_as_a_complete_registry`].
///
/// It is SIX, and the seventh term the inventory was first drafted with —
/// `gmeow-math-v1` — is absent for a reason no measurement can overturn: a
/// dictionary primes a FRAME, `gmeow:payloadSchemaDictionary` is
/// `maxQualifiedCardinality 1`, every `math:` named graph is unioned into the
/// single snapshot frame, and that frame already binds `gmeow-core-v1`. There is
/// no mathematical BYTE family to give one instead (the archive fold's sources are
/// `dsl/mappings/**`, the per-slice `mappings/`+`tests/` trees, and the shape
/// surfaces), and manufacturing one by de-folding a named graph would trade
/// queryable structure for compression. The mathematical content is therefore
/// primed in full by `gmeow-core-v1`. See the note in the slice.
const SHIPPED_DICTIONARY_IDS: [&str; 6] = [
    "gmeow-core-v1",
    "gmeow-lang-ast-v1",
    "gmeow-logic-v1",
    "gmeow-memory-compact-v1",
    "gmeow-memory-hot-v1",
    "gmeow-prooftrace-v1",
];

/// NON-VACUITY: the reader is exercised against the REAL authored declaration,
/// not only the fixture. Every shipped dictionary, its corpus, the
/// payload-schema registry, and both declared media must all read cleanly — if
/// the reader silently disagreed with `slices/core/gts/module.ttl`, every
/// fixture-based test above would still pass.
///
/// (The authored Turtle is parsed here because a unit test has no carrier to
/// read; PRODUCTION always reads the in-memory dataset. That asymmetry is the
/// point of the test — it proves the two agree.)
#[test]
fn the_live_gts_slice_reads_as_a_complete_registry() {
    let registry =
        crate::medium::source_observation::authenticated(&gmeow_conformance::paths::repo_root())
            .expect("authenticated source medium registry")
            .registry;

    let ids: BTreeSet<&str> = registry
        .dictionaries()
        .values()
        .map(|d| d.id.as_str())
        .collect();
    // BOTH directions, and the count on its own line: a dropped dictionary orphans
    // every artifact primed with it, and an added one would be trained, measured,
    // pinned in the header, and projected onto a committed `.zdict` while priming
    // nothing (or priming a population too small to pay for its own bytes) — which
    // is exactly how a math dictionary shipped as dead weight until the frame
    // layout was checked. Neither direction may pass unnoticed.
    assert_eq!(
        ids.len(),
        SHIPPED_DICTIONARY_IDS.len(),
        "the bundle ships {} dictionaries; got {ids:?}",
        SHIPPED_DICTIONARY_IDS.len()
    );
    assert_eq!(
        ids,
        SHIPPED_DICTIONARY_IDS
            .into_iter()
            .collect::<BTreeSet<&str>>(),
        "the declared dictionary inventory drifted"
    );
    for id in SHIPPED_DICTIONARY_IDS {
        let def = registry.dictionary_by_id(id).expect("shipped dictionary");
        assert!(
            registry.corpora().contains_key(&def.corpus),
            "{id} trains over <{}>, which must be a declared corpus",
            def.corpus
        );
        assert!(def.target_length > 0, "{id} declares a zero target length");
    }

    // TOTALITY IN THE OTHER DIRECTION — the check that turns "gmeow-math-v1 primed
    // nothing" from a thing someone had to measure into a thing the gate refuses.
    // A dictionary has exactly two legitimate homes:
    //
    //   * it is selected by a registered `gmeow:PayloadSchema`, so some emitted
    //     frame is actually primed with it; or
    //   * it is bound by a `gmeow:mediumSourceHeaderDict` medium — the runtime-store
    //     family, whose frames are written by a CONSUMER out of the shipped header
    //     rather than by this emission, so no bundle rep names it.
    //
    // Anything else is a dictionary the bundle trains, measures, pins, and projects
    // while no payload cites it: pure dead weight, and (Constitution §18) high-entropy
    // bytes handed to a compressor for nothing.
    let primed: BTreeSet<&str> = registry
        .schemas()
        .values()
        .filter_map(
            |schema| match &registry.assignment_for(&schema.rep).ok()?.dictionary {
                DictSelection::Named(iri) => {
                    registry.dictionaries().get(iri).map(|d| d.id.as_str())
                }
                DictSelection::Baseline => None,
            },
        )
        .collect();
    let consumer_primed: BTreeSet<&str> = registry
        .media()
        .values()
        .filter(|m| m.source_kind == MediumSourceKind::HeaderDict)
        .flat_map(|m| m.dictionaries.iter())
        .filter_map(|iri| registry.dictionaries().get(iri).map(|d| d.id.as_str()))
        .collect();
    for id in SHIPPED_DICTIONARY_IDS {
        assert!(
            primed.contains(id) || consumer_primed.contains(id),
            "{id} primes no registered gmeow:PayloadSchema and is bound by no \
                 header-dict medium — it would be trained, measured, pinned and projected \
                 while no frame cites it. Assign it to a rep or retire it; do NOT weaken \
                 this assertion (primed: {primed:?}, consumer-primed: {consumer_primed:?})"
        );
    }

    // The two memory dictionaries are the ones whose corpora must be
    // BUNDLE-INTERNAL rather than a user's runtime store: a zstd dictionary
    // carries verbatim substrings of its corpus, so this is a privacy property,
    // not a tidiness one.
    for id in ["gmeow-memory-compact-v1", "gmeow-memory-hot-v1"] {
        let def = registry.dictionary_by_id(id).expect("shipped dictionary");
        let corpus = registry.corpora().get(&def.corpus).expect("declared");
        for selector in &corpus.selectors {
            if let crate::medium::corpus::CorpusSelector::PathPrefix(prefix) = selector {
                assert!(
                    !prefix.contains(".gmeow"),
                    "{id} must never train on a user's runtime store: {prefix}"
                );
            }
        }
    }

    assert!(
        registry.schemas().len() >= 20,
        "one gmeow:PayloadSchema per emittable rep; got {}",
        registry.schemas().len()
    );
    assert!(
        registry
            .schemas()
            .values()
            .any(|s| s.rep == SNAPSHOT_WIRE_REP),
        "the snapshot wire schema is registered like any other payload"
    );
    // TOTALITY: the authored assignment covers EVERY registered rep. This is
    // the property that makes `MediumUndeclaredDictionary` unreachable on the
    // live tree — without it every real rep would raise it at emission time.
    for schema in registry.schemas().values() {
        let row = registry
            .assignment_for(&schema.rep)
            .unwrap_or_else(|err| panic!("rep {:?} is unassigned: {err}", schema.rep));
        assert!(
            registry.media().contains_key(&row.medium),
            "rep {:?} names undeclared medium <{}>",
            schema.rep,
            row.medium
        );
        match &row.dictionary {
            DictSelection::Named(iri) => assert!(
                registry.dictionaries().contains_key(iri),
                "rep {:?} selects unregistered dictionary <{iri}>",
                schema.rep
            ),
            DictSelection::Baseline => assert!(
                registry
                    .media()
                    .get(&row.medium)
                    .is_some_and(|m| m.dictionaries.is_empty()),
                "rep {:?} is unprimed under a medium that declares dictionaries",
                schema.rep
            ),
        }
    }
    // baseline + dist + store. The three are not decoration: each is the ONLY
    // medium under which one of the three `gmeow:MediumSourceKind` branches is
    // reachable, so a missing one would leave that branch with no live producer.
    assert_eq!(registry.media().len(), 3, "baseline + dist + store");
    let mut kinds: BTreeSet<MediumSourceKind> = BTreeSet::new();
    for medium in registry.media().values() {
        assert_eq!(
            medium
                .codec_wire_name()
                .expect("declared codec is supported"),
            "zstd-rsyncable",
            "<{}> must declare the mandated codec",
            medium.iri
        );
        assert_eq!(
            medium.zstd_level, 12,
            "<{}> must declare the mandated level",
            medium.iri
        );
        assert!(
            !medium.reader_capabilities.is_empty(),
            "<{}> uses a non-baseline codec, so it must declare its reader contract",
            medium.iri
        );
        kinds.insert(medium.source_kind);
    }
    assert_eq!(
        kinds,
        [
            MediumSourceKind::PerRep,
            MediumSourceKind::HeaderDict,
            MediumSourceKind::WholeArtifact,
        ]
        .into_iter()
        .collect::<BTreeSet<MediumSourceKind>>(),
        "every declared gmeow:MediumSourceKind must be realized by a live medium"
    );
    // The store medium's bound is exactly the two memory dictionaries: a store
    // primed with anything else could never be re-primed by a consumer holding only
    // the shipped bundle.
    let store = registry
        .media()
        .get(&gm("mediumProfileStoreL12"))
        .expect("the store medium is declared");
    assert_eq!(store.source_kind, MediumSourceKind::HeaderDict);
    assert_eq!(
        store.dictionaries,
        [gm("dictGmeowMemoryCompactV1"), gm("dictGmeowMemoryHotV1")]
            .into_iter()
            .collect::<BTreeSet<String>>()
    );
}

/// A selected dictionary with no trained bytes would emit frames citing a
/// dictionary the pack does not carry.
#[test]
fn the_medium_plan_hard_fails_when_a_selected_dictionary_has_no_bytes() {
    let registry = registry("");
    let diag = registry
        .medium_plan_under(
            &MediumSelection::Authored,
            &["cells-archive".to_string()],
            &BTreeMap::new(),
        )
        .expect_err("a selected dictionary with no bytes must not produce a plan");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
}
