// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn source(path: &str, kind: OriginKind, text: &str) -> ParsedAuthoredSource {
    ParsedAuthoredSource {
        path: Path::new("/not-a-repository").join(path),
        relative_path: path.to_owned(),
        content_digest: purrdf::ContentDigest::of(text.as_bytes()).to_hex(),
        blake3_digest: blake3::hash(text.as_bytes()).to_hex().to_string(),
        kind,
        ingested: PurrdfAdapter
            .ingest(path, "text/turtle", text.as_bytes())
            .expect("fixture parse"),
    }
}

#[test]
fn shared_inputs_keep_document_occurrences_and_import_roles() {
    let sources = ParsedAuthoredSources {
        sources: vec![
            source(
                "ontology/gmeow.ttl",
                OriginKind::RootOntology,
                "@prefix ex: <https://example.org/> . ex:law ex:p ex:Thing . _:node ex:p ex:Root .",
            ),
            source(
                "slices/core/example/module.ttl",
                OriginKind::Source,
                "@prefix ex: <https://example.org/> . ex:law ex:p ex:Thing . _:node ex:p ex:Module .",
            ),
            source(
                "imports/example.ttl",
                OriginKind::Import,
                "@prefix ex: <https://example.org/> . ex:law ex:p ex:Thing . _:node ex:p ex:Import . ex:importOnly ex:p ex:Thing .",
            ),
        ],
    };
    // These paths do not exist. Every consumer must use the admitted native
    // inputs, with no source reread, parse or corpus-producer invocation.
    assert_eq!(sources.merged_dataset().quads().count(), 5);
    let authored = sources.authored_dataset();
    let imports = sources.imports_dataset();
    assert_eq!(authored.quads().count(), 3);
    assert_eq!(imports.quads().count(), 3);
    assert!(
        authored
            .term_id_by_iri("https://example.org/importOnly")
            .is_none()
    );
    assert!(
        imports
            .term_id_by_iri("https://example.org/importOnly")
            .is_some()
    );

    let (provenance, expected) = super::super::attributed_parsed_provenance(&sources);
    purrdf::provenance::check_provenance(&provenance, &expected)
        .expect("all occurrences attributed");
    assert_eq!(expected.len(), 5);
    assert_eq!(provenance.public_projection().len(), 7);
    let mut counts = std::collections::BTreeMap::new();
    for (handle, _, _, _, _) in provenance.public_projection() {
        *counts.entry(handle).or_insert(0) += 1;
    }
    let mut counts: Vec<usize> = counts.into_values().collect();
    counts.sort_unstable();
    assert_eq!(
        counts,
        [1, 1, 1, 1, 3],
        "one shared law retains all three origins"
    );

    let public_spans = sources.source_span_index();
    assert_eq!(
        public_spans
            .lookup("https://example.org/law")
            .expect("root span")
            .path
            .as_ref(),
        "ontology/gmeow.ttl"
    );
    assert!(
        public_spans
            .lookup("https://example.org/importOnly")
            .is_none()
    );
    let import = &sources.sources()[2];
    assert!(
        import
            .ingested
            .spans
            .index()
            .lookup("https://example.org/importOnly")
            .is_some()
    );
    assert!(
        import
            .ingested
            .dataset
            .term_id_by_iri("https://example.org/importOnly")
            .is_some()
    );
}

#[test]
fn catalog_maps_original_blank_anchors_through_the_actual_aggregate() {
    use crate::stages::parse_sources::SourceCatalog;
    use gmeow_logic_compile::frontend::PreparedLogicSource;
    use purrdf::{DatasetView, GraphMatch, TermRef};

    let text =
        "@prefix ex: <https://example.org/> . _:same ex:p ex:Thing . ex:shared ex:p ex:Thing .";
    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources {
        sources: vec![
            source("ontology/gmeow.ttl", OriginKind::RootOntology, text),
            source("slices/core/example/module.ttl", OriginKind::Source, text),
            source("imports/example.ttl", OriginKind::Import, text),
        ],
    })
    .unwrap();
    let prepared = PreparedLogicSource::new(catalog.materialized()).unwrap();
    let mut blank_targets = std::collections::BTreeSet::new();
    let mut shared_targets = std::collections::BTreeSet::new();
    for source in catalog.sources().sources() {
        // Original document dictionaries survive deduplication of the named assertion.
        let original = catalog.document(&source.relative_path).unwrap();
        assert!(std::ptr::eq(original, source.ingested.dataset.as_ref()));
        let blank = original
            .quads()
            .find(|q| matches!(original.resolve(q.s), TermRef::Blank { .. }))
            .unwrap()
            .s;
        let aggregate = catalog
            .materialized_term(&source.relative_path, blank)
            .unwrap();
        let canonical = prepared.source_term(aggregate).unwrap();
        assert_eq!(
            prepared
                .dataset()
                .quads_for_pattern(Some(canonical), None, None, GraphMatch::Default)
                .count(),
            1
        );
        blank_targets.insert(canonical);
        let shared = original
            .term_id_by_iri("https://example.org/shared")
            .unwrap();
        shared_targets.insert(
            catalog
                .materialized_term(&source.relative_path, shared)
                .unwrap(),
        );
    }
    assert_eq!(
        blank_targets.len(),
        3,
        "identical labels and automorphic structure retain three document origins"
    );
    assert_eq!(
        shared_targets.len(),
        1,
        "duplicate named assertions share data, while source occurrences remain distinct"
    );
    assert_eq!(catalog.sources().sources().len(), 3);
    assert_eq!(catalog.materialized().quad_count(), 4);
    assert!(catalog.document("missing.ttl").is_err());
}

#[test]
fn catalog_compiles_every_document_once_for_concurrent_consumers() {
    use crate::stages::parse_sources::SourceCatalog;
    use gmeow_logic_compile::frontend::{OwnerDisposition, OwnerFamily};

    let sources = [
        ("ontology/gmeow.ttl", OriginKind::RootOntology, "root"),
        (
            "slices/core/example/module.ttl",
            OriginKind::Source,
            "slice",
        ),
        ("imports/example.ttl", OriginKind::Import, "import"),
    ]
    .into_iter()
    .map(|(path, role, name)| {
        source(
            path,
            role,
            &format!(
                r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
            @prefix ex: <https://example.org/> .
            ex:{name} a logic:Rule ;
                logic:head [ rdf:subject ex:{name} ; rdf:predicate ex:p ; rdf:object ex:value ] .
        "#
            ),
        )
    })
    .collect();
    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources { sources }).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (first, second) = std::thread::scope(|scope| {
        let read = || {
            barrier.wait();
            catalog.compiled_logic().unwrap()
        };
        let first = scope.spawn(read);
        let second = scope.spawn(read);
        (first.join().unwrap(), second.join().unwrap())
    });
    assert!(Arc::ptr_eq(&first, &second));
    drop(catalog);
    assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
    let subjects: std::collections::BTreeSet<_> = first
        .program()
        .rules
        .iter()
        .map(|rule| rule.head.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        std::collections::BTreeSet::from([
            "https://example.org/root",
            "https://example.org/slice",
            "https://example.org/import",
        ])
    );
    assert_eq!(first.source().origins().len(), 3);
    for owner in first
        .owner_lowerings()
        .iter()
        .filter(|owner| owner.family == OwnerFamily::Rule)
    {
        let OwnerDisposition::Emitted { index } = owner.disposition else {
            panic!("required rule was not emitted: {owner:?}");
        };
        assert!(index < first.program().rules.len());
        assert!(first.source().origins().iter().any(|origin| {
            origin
                .bindings
                .iter()
                .any(|binding| binding.canonical == owner.source)
        }));
    }
}

#[test]
fn prepared_structural_units_retain_each_document_origin_and_exact_blank_mapping() {
    use crate::stages::parse_sources::SourceCatalog;
    use gmeow_logic_compile::frontend::{
        SourceBaseOrigin, SourceOccurrencePosition, SourceUnitKind,
    };

    let text = "@base <https://example.org/> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            <law> a logic:Formula ; logic:not <atom> .
            _:local a logic:Formula ; logic:not <atom> .
            <atom> a logic:Formula ; logic:relation <p> ;
                logic:argument [ logic:termIndex 0 ; logic:termIri <a> ] .";
    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources {
        sources: vec![
            source("slices/core/example/module.ttl", OriginKind::Source, text),
            source("imports/example.ttl", OriginKind::Import, text),
        ],
    })
    .unwrap();
    let prepared = catalog.prepare_logic().unwrap();
    assert_eq!(prepared.origins().len(), 2);
    let law = prepared
        .dataset()
        .term_id_by_iri("https://example.org/law")
        .unwrap();
    let mut blanks = std::collections::BTreeSet::new();
    for (index, origins) in prepared.origins().iter().enumerate() {
        assert_eq!(
            origins.document.content_digest,
            purrdf::ContentDigest::of(text.as_bytes()).to_hex()
        );
        assert!(matches!(
            origins.document.base.as_ref().unwrap().origin,
            SourceBaseOrigin::Directive { .. }
        ));
        assert!(
            origins
                .bindings
                .iter()
                .any(|binding| binding.canonical.term == law
                    && binding.position == SourceOccurrencePosition::Subject)
        );
        let original = &catalog.sources().sources()[index].ingested.dataset;
        for binding in origins.bindings.iter().filter(|binding| {
            binding.position == SourceOccurrencePosition::Subject
                && prepared
                    .source_graph()
                    .declares(binding.canonical, SourceUnitKind::Formula)
                && matches!(
                    original.resolve(binding.original.term),
                    purrdf::TermRef::Blank { .. }
                )
        }) {
            let selected = catalog
                .materialized_term(&origins.document.path, binding.original.term)
                .unwrap();
            assert_eq!(prepared.source_term(selected), Some(binding.canonical.term));
            blanks.insert(binding.canonical.term);
        }
    }
    assert_eq!(
        blanks.len(),
        2,
        "equal anonymous formulas still have distinct source anchors"
    );
    assert_eq!(
        prepared.origins()[0].document.role,
        OriginKind::Source.to_string()
    );
    assert_eq!(
        prepared.origins()[1].document.role,
        OriginKind::Import.to_string()
    );
    let selected = catalog.prepare_document("imports/example.ttl").unwrap();
    assert_eq!(selected.origins().len(), 1);
    assert_eq!(selected.origins()[0].document.path, "imports/example.ttl");
    assert!(catalog.prepare_document("absent.ttl").is_err());
}

#[test]
fn catalog_identity_binds_exact_bytes_roles_and_document_names() {
    use crate::stages::parse_sources::SourceCatalog;

    let text = "@base <https://example.org/> . <s> <p> <o> .";
    let make = |path, kind, text| {
        SourceCatalog::from_sources(ParsedAuthoredSources {
            sources: vec![source(path, kind, text)],
        })
        .unwrap()
    };
    let first = make("original.ttl", OriginKind::Source, text);
    let same = make("original.ttl", OriginKind::Source, text);
    assert_eq!(first.identity(), same.identity());
    for changed in [
        make("renamed.ttl", OriginKind::Source, text),
        make("original.ttl", OriginKind::Import, text),
        make(
            "original.ttl",
            OriginKind::Source,
            &format!("{text} # changed source span"),
        ),
    ] {
        assert!(purrdf::datasets_isomorphic(
            first.materialized(),
            changed.materialized()
        ));
        assert_ne!(
            first.identity(),
            changed.identity(),
            "RDF equivalence does not erase original source identity"
        );
    }
}

#[test]
fn persistent_cache_refuses_the_ephemeral_native_source_catalog() {
    use crate::bundle::PipelineHandle;
    use crate::cache::{PipelineCache, ReceiptOutputSelection, StageKeyContext};
    use crate::node::StageProduct;
    use crate::stages::parse_sources::{GRAPH_SOURCE_CATALOG, SourceCatalog};

    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources {
        sources: vec![source(
            "synthetic.ttl",
            OriginKind::Source,
            "<urn:s> <urn:p> <urn:o> .",
        )],
    })
    .unwrap();
    let receipt = purrdf::parse_dataset(
        format!(
            "<urn:receipt> <urn:identity> \"{}\" <{GRAPH_SOURCE_CATALOG}> .",
            catalog.identity()
        )
        .as_bytes(),
        "application/n-quads",
        None,
    )
    .unwrap();
    let mut bundle =
        crate::bundle::bundle_from_artifacts_over(receipt, Default::default(), Default::default());
    let pin = bundle.graph_digest(GRAPH_SOURCE_CATALOG);
    bundle
        .pin_handle(
            GRAPH_SOURCE_CATALOG,
            PipelineHandle::SourceCatalog(Arc::new(catalog)),
            pin,
        )
        .unwrap();
    let product = StageProduct::from_bundle("stage-parse-sources", Arc::new(bundle));
    let selection = ReceiptOutputSelection {
        graphs: vec![GRAPH_SOURCE_CATALOG.to_owned()],
        handles: vec![GRAPH_SOURCE_CATALOG.to_owned()],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let context =
        StageKeyContext::new("stage-parse-sources", "test-native-catalog", vec![], vec![]);
    let error = cache
        .put(&context, "stable", "persistent", &selection, &product)
        .unwrap_err();
    assert!(
        error.to_string().contains("ephemeral source catalogs"),
        "{error}"
    );
    assert!(
        cache.get(&context).unwrap().is_none(),
        "no recoverable action is published"
    );
}
