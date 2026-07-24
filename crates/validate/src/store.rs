// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free RDF ingestion for the validation lints (native `RdfDataset` IR).
//!
//! Every validation engine (coverage, lint, gUFO, statement, constitution, the DSL
//! SHACL merge, the data-graph path) reads a frozen [`purrdf::RdfDataset`]: the
//! sources are parsed once with the native gmeow-rdf codecs ([`parse_dataset`]),
//! merged under per-file blank scopes via [`purrdf::RdfDatasetBuilder`], and
//! queried through the indexed [`purrdf::DatasetView::quads_for_pattern`]. The
//! SHACL engine is itself native ([`shacl_validate_dataset`]).
//!
//! This module is fully oxigraph-free: every helper returns or queries the
//! native [`purrdf::RdfDataset`].
//!
//! Parsing is **lenient by construction**: the native codecs accept the GMEOW
//! ontology's private-use `@x-gmeow-*` language tags whose subtag exceeds BCP-47's
//! 8-char limit (e.g. `@x-gmeow-afrikaans`), while still surfacing every real Turtle
//! syntax error.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_errors::Diag;
use purrdf::{RdfDataset, RdfDatasetBuilder, parse_dataset};

use crate::model::owl;

/// Validate a native [`RdfDataset`] against parsed SHACL
/// [`purrdf::shapes::shapes::Shapes`] over the native
/// IR engine.
///
/// The SHACL engine is fully native (it takes an `RdfDataset` directly), so this is a
/// thin wrapper that surfaces the validation report and treats engine failure as
/// infallible for a frozen, validated dataset.
pub fn shacl_validate_dataset(
    dataset: &RdfDataset,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> purrdf::shapes::report::ValidationReport {
    purrdf::shapes::engine::validate_dataset(dataset, shapes)
        .expect("validation over a frozen dataset is infallible")
}

/// Parse a single Turtle file into a frozen native [`RdfDataset`].
///
/// Lenient parsing (accepts GMEOW's private-use `@x-gmeow-*` language tags). The
/// returned dataset is uniquely owned (fresh off the native parser).
///
/// # Errors
///
/// Returns `Err` if the file cannot be read or the Turtle fails to parse.
pub fn parse_file_dataset(path: &Path) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let bytes = std::fs::read(path).map_err(|e| {
        Diag::of_kind(crate::error::Io {
            detail: e.to_string(),
        })
    })?;
    parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        Diag::of_kind(crate::error::Parse {
            detail: e.to_string(),
        })
    })
}

/// Build one merged frozen [`RdfDataset`] from every Turtle source in `paths`.
///
/// Each file is parsed under a fresh blank scope ([`RdfDatasetBuilder::push_dataset`])
/// so anonymous blanks across files stay disjoint (C0.2 — the native twin of the old
/// per-source blank-prefix scoping). Quads dedup at freeze (C0.5), matching the old
/// `Store::insert` set semantics. A malformed file aborts with an error naming the
/// file.
///
/// # Errors
///
/// Returns `Err` if any file fails to read or parse.
pub fn dataset_from_paths(paths: &[PathBuf]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for path in paths {
        let path_str = path.display().to_string();
        let bytes = std::fs::read(path).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("failed to read {path_str}: {e}"),
            })
        })?;
        let dataset = parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            Diag::of_kind(crate::error::Parse {
                detail: format!("syntax error in {path_str}: {e}"),
            })
        })?;
        builder.push_dataset(&dataset);
    }
    builder.freeze().map_err(|e| {
        Diag::of_kind(crate::error::Serialize {
            detail: format!("dataset freeze failed: {e}"),
        })
    })
}

/// Build a frozen native [`RdfDataset`] from an N-Triples document, flattening any
/// graph slot to the default graph (N-Triples is graphless).
///
/// Lenient parsing (private-use `@x-gmeow-*` language tags) — the data seam for the
/// rdflib-free validation path.
///
/// # Errors
///
/// Returns `Err` if the N-Triples fails to parse.
pub fn dataset_from_nt(data_nt: &str) -> gmeow_errors::Result<Arc<RdfDataset>> {
    parse_dataset(data_nt.as_bytes(), "application/n-triples", None).map_err(|e| {
        Diag::of_kind(crate::error::Parse {
            detail: format!("N-Triples parse error: {e}"),
        })
    })
}

/// Build a frozen native [`RdfDataset`] from a GTS byte bundle, flattening every named
/// graph into the default graph (so the lints/shapes see the whole graph).
///
/// Routes through the oxigraph-free [`purrdf::gts::flattened_dataset_from_bytes`]:
/// `read_all_segments` → the native statement-layer fold → `freeze` with every quad
/// re-homed to the default graph. A non-empty diagnostic list is a hard failure.
///
/// # Errors
///
/// Returns `Err` if the GTS fold reports any diagnostics or the projected
/// quads cannot be folded into the IR.
pub fn dataset_from_gts(bytes: &[u8]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    purrdf::gts::flattened_dataset_from_bytes(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: e.to_string(),
        })
    })
}

/// Project a full `gmeow.gts` bundle into a **core browser bundle** — graph-preserving
/// N-Quads text carrying only the object-level ontology (the default graph) plus any
/// explicitly kept named graphs, with every derived/heavy graph dropped (the
/// documentation projection, the `graph/fanout/*` flat-file re-embeds, diagnostics,
/// authoring briefs, the reasoned closure, …).
///
/// The FULL bundle extracts to ~948 MB of N-Quads — far too large to load and query
/// in a browser (it OOMs the wasm engine). This projection keeps the queryable
/// object-level ontology (~124 k quads → ~24 MB N-Quads, well within a browser's
/// reach once the web server gzips it) so the in-browser playground/explorer can
/// parse and SPARQL over the SAME authored ontology the pipeline shipped. It is
/// shipped as N-Quads TEXT (not a GTS container) so the in-page purrdf RDF engine
/// parses it directly with no container codec, and it is a pure, deterministic
/// function of the input bytes (order-preserving filter + deterministic serializer),
/// so the emitted asset is byte-reproducible.
///
/// `keep_named_graphs` is the allow-list of named-graph IRIs to retain ALONGSIDE the
/// default graph (e.g. grounding graphs); pass an empty slice for object-level only.
///
/// # Errors
///
/// Returns `Err` if the container cannot be read, the statement layer cannot be
/// folded, or the filtered dataset cannot be serialized.
pub fn core_browser_bundle_nquads(
    full_bytes: &[u8],
    keep_named_graphs: &[&str],
) -> gmeow_errors::Result<String> {
    use std::collections::HashSet;
    let to_diag = |e: purrdf::RdfDiagnostic| {
        Diag::of_kind(crate::error::Dataset {
            detail: e.to_string(),
        })
    };
    let mut graph = purrdf::gts::read_all_segments(full_bytes).map_err(to_diag)?;
    // Term ids whose value is a kept named-graph IRI. The default graph (`None` slot)
    // is always retained; every other named graph is dropped.
    let keep: HashSet<usize> = graph
        .terms
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match t.value.as_deref() {
            Some(v) if keep_named_graphs.contains(&v) => Some(i),
            _ => None,
        })
        .collect();
    let kept = |slot: Option<usize>| slot.is_none_or(|gid| keep.contains(&gid));
    graph.quads.retain(|q| kept(q.3));
    graph.reifiers.retain(|r| kept(r.2));
    graph.annotations.retain(|a| kept(a.3));
    // Fold the filtered graph into a graph-preserving dataset and serialize to
    // N-Quads over the full dataset selection (the term table rides in the codec, so
    // no term-pruning is needed — dropped quads simply do not appear).
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph).map_err(to_diag)?;
    let bytes = purrdf::serialize_dataset(
        &*dataset,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(to_diag)?;
    String::from_utf8(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: format!("core browser bundle N-Quads is not valid UTF-8: {e}"),
        })
    })
}

/// Read a `gmeow.gts` bundle's bytes into **graph-preserving** N-Quads text — every
/// base quad keeps its named-graph component (unlike [`dataset_from_gts`], which
/// folds them into the default graph). This is the browser bundle-read primitive:
/// the wasm shim (`gmeow-validate-wasm::bundle_dataset`) hands the resulting N-Quads
/// to the in-page purrdf RDF engine so the documentation playground/explorer query
/// the SAME bundle the pipeline shipped, rather than a second curated data path.
///
/// Uses the oxigraph-free container reader (`read_all_segments` →
/// `dataset_from_gts_graph`, which retains each quad's graph) and the native
/// N-Quads serializer over the full dataset selection; both are wasm-clean (no
/// reasoner, no filesystem).
///
/// # Errors
///
/// Returns `Err` if the GTS container cannot be read, the statement layer cannot be
/// folded, or the dataset cannot be serialized.
pub fn dataset_nquads_from_gts(bytes: &[u8]) -> gmeow_errors::Result<String> {
    let to_diag = |e: purrdf::RdfDiagnostic| {
        Diag::of_kind(crate::error::Dataset {
            detail: e.to_string(),
        })
    };
    let graph = purrdf::gts::read_all_segments(bytes).map_err(to_diag)?;
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph).map_err(to_diag)?;
    let bytes = purrdf::serialize_dataset(
        &*dataset,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(to_diag)?;
    String::from_utf8(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: format!("bundle N-Quads is not valid UTF-8: {e}"),
        })
    })
}

/// Render a resolved subject term the way the legacy `_ox_term_display` did:
/// IRI → its value; blank → `_:b`.
///
/// A triple subject is exactly an IRI or a blank node in well-formed RDF; a
/// literal/triple subject stringifies defensively (never reached on the validation
/// path).
pub fn subject_display(subject: purrdf::TermRef<'_>) -> String {
    use purrdf::TermRef;
    match subject {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        TermRef::Literal { lexical, .. } => lexical.to_owned(),
        TermRef::Triple { .. } => "<<triple>>".to_owned(),
    }
}

/// Scan a frozen native [`RdfDataset`] for Principle 5 `owl:sameAs`-to-external-entity
/// violations, in document (dataset) order.
///
/// A violation is every `owl:sameAs` triple whose object is an IRI that does NOT start
/// with `namespace`, unless `(subject_display, object)` is in `allowlist`. Returns the
/// `(subject_display, object)` pair for each violation — the caller frames the
/// user-facing message so the file path can be interpolated exactly as before.
pub fn sameas_violations(
    dataset: &RdfDataset,
    namespace: &str,
    allowlist: &[(String, String)],
) -> Vec<(String, String)> {
    use purrdf::{DatasetView, GraphMatch, TermRef, TermValue};

    let Some(sameas_id) = dataset.term_id_by_value(&TermValue::iri(owl::SAME_AS)) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = Vec::new();
    for quad in dataset.quads_for_pattern(None, Some(sameas_id), None, GraphMatch::Any) {
        let TermRef::Iri(obj) = dataset.resolve(quad.o) else {
            continue;
        };
        if obj.starts_with(namespace) {
            continue;
        }
        let subject_text = subject_display(dataset.resolve(quad.s));
        if allowlist
            .iter()
            .any(|(s, o)| s == &subject_text && o == obj)
        {
            continue;
        }
        out.push((subject_text, obj.to_owned()));
    }
    out
}

/// Parse GTS bytes into a [`purrdf::gts::model::Graph`].
///
/// Folds the GTS bytes with all segments enabled (`allow_segments = true`). Any
/// non-empty diagnostic list is treated as a hard failure (fail-fast) so callers
/// never receive a silent partial graph from malformed or truncated GTS bytes.
///
/// # Errors
///
/// Returns `Err` if the GTS fold reports any diagnostics (corruption,
/// truncation, empty input, or unfolded segments).
pub fn read_gts_graph(bytes: &[u8]) -> gmeow_errors::Result<purrdf::gts::model::Graph> {
    purrdf::gts::read_all_segments(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: e.to_string(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    use purrdf::{DatasetView, GraphMatch};

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    #[test]
    fn parse_file_dataset_rejects_bad_turtle() {
        let path = write_tmp("gmeow_validate_store_bad.ttl", "this is not turtle <<< @@@");
        let result = parse_file_dataset(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err(), "malformed Turtle must parse-error");
    }

    #[test]
    fn parse_file_dataset_accepts_good_turtle() {
        let path = write_tmp(
            "gmeow_validate_store_good.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        );
        let result = parse_file_dataset(&path);
        std::fs::remove_file(&path).ok();
        let ds = result.expect("well-formed Turtle must parse");
        assert_eq!(ds.quad_count(), 1);
    }

    #[test]
    fn dataset_from_paths_loads_multiple_files() {
        let a = write_tmp(
            "gmeow_validate_store_multi_a.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        );
        let b = write_tmp(
            "gmeow_validate_store_multi_b.ttl",
            "@prefix ex: <https://example.org/> .\nex:c ex:p ex:d .\n",
        );
        let ds = dataset_from_paths(&[a.clone(), b.clone()]).expect("both files must load");
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
        assert_eq!(ds.quad_count(), 2);
    }

    #[test]
    fn dataset_from_paths_propagates_parse_error() {
        let good = write_tmp(
            "gmeow_validate_parsed_err_good.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        );
        let bad = write_tmp(
            "gmeow_validate_parsed_err_bad.ttl",
            "this is not turtle @@@ <<<",
        );
        let result = dataset_from_paths(&[good.clone(), bad.clone()]);
        std::fs::remove_file(&good).ok();
        std::fs::remove_file(&bad).ok();
        assert!(result.is_err(), "a malformed file must propagate");
        let err = result.err().unwrap();
        let msg = err.message();
        assert!(
            msg.contains("syntax error in") && msg.contains("gmeow_validate_parsed_err_bad.ttl"),
            "error must use 'syntax error in' format naming the bad file; got: {msg}"
        );
    }

    #[test]
    fn sameas_flags_external_object() {
        let ds = parse_dataset(
            "@prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:a owl:sameAs ex:b .\n"
                .as_bytes(),
            "text/turtle",
            None,
        )
        .unwrap();
        let violations = sameas_violations(&ds, NS, &[]);
        assert_eq!(
            violations,
            vec![(
                "https://example.org/a".to_owned(),
                "https://example.org/b".to_owned()
            )]
        );
    }

    #[test]
    fn sameas_skips_internal_and_allowlisted() {
        let ds = parse_dataset(
            format!(
                "@prefix gmeow: <{NS}> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                 gmeow:A owl:sameAs gmeow:B .\n\
                 ex:a owl:sameAs ex:b .\n"
            )
            .as_bytes(),
            "text/turtle",
            None,
        )
        .unwrap();
        let allowlist = vec![(
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned(),
        )];
        assert!(sameas_violations(&ds, NS, &allowlist).is_empty());
    }

    #[test]
    fn dataset_from_nt_loads_and_rejects_malformed() {
        let nt = "<https://example.org/a> <https://example.org/p> <https://example.org/b> .\n";
        let ds = dataset_from_nt(nt).expect("valid N-Triples must load");
        assert_eq!(ds.quad_count(), 1);
        assert!(dataset_from_nt("this is not n-triples @@@").is_err());
    }

    #[test]
    fn dataset_from_gts_loads_single_triple_in_default_graph() {
        use purrdf::gts::model::{Term, TermKind};
        use purrdf::gts::writer::Writer;

        let mut graph = purrdf::gts::model::Graph::default();
        for iri in [
            "https://example.org/s",
            "https://example.org/p",
            "https://example.org/o",
        ] {
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(iri.to_owned()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            });
        }
        // Named graph slot to verify the flatten.
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://blackcatinformatics.ca/gmeow/graph/metadata".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.quads.push((0, 1, 2, Some(3)));

        let writer = Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
        let ds = dataset_from_gts(&writer.to_bytes()).expect("GTS bytes must fold into dataset");
        assert_eq!(ds.quad_count(), 1);
        // The named-graph quad is flattened to the default graph.
        assert_eq!(
            ds.quads_for_pattern(None, None, None, GraphMatch::Default)
                .count(),
            1
        );
    }

    #[test]
    fn dataset_from_gts_accepts_private_lang_tag_and_flattens_named_graph() {
        use purrdf::gts::model::{Term, TermKind};
        use purrdf::gts::writer::Writer;

        // A literal with a private-use `@x-gmeow-*` tag (BCP-47 subtag > 8 chars)
        // in a NAMED graph. The lenient native fold must accept the tag and collapse
        // the named graph into the default graph.
        let mut graph = purrdf::gts::model::Graph::default();
        for value in ["https://example.org/s", "https://example.org/p"] {
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(value.to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            });
        }
        graph.terms.push(Term {
            kind: TermKind::Literal,
            value: Some("hallo".to_string()),
            datatype: None,
            lang: Some("x-gmeow-afrikaans".to_string()),
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://blackcatinformatics.ca/gmeow/graph/metadata".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        // Object (term 2) carries the private lang tag; quad lives in named graph (term 3).
        graph.quads.push((0, 1, 2, Some(3)));

        let writer = Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
        let ds = dataset_from_gts(&writer.to_bytes())
            .expect("private lang tag in a named graph must load leniently");

        assert_eq!(ds.quad_count(), 1);
        // Flattened: the triple is in the default graph and the lang tag survives.
        let q = ds
            .quads_for_pattern(None, None, None, GraphMatch::Default)
            .next()
            .expect("one default-graph quad");
        match ds.resolve(q.o) {
            purrdf::TermRef::Literal { language, .. } => {
                assert_eq!(language, Some("x-gmeow-afrikaans"), "lang tag preserved");
            }
            other => panic!("object must be a literal, got {other:?}"),
        }
    }

    #[test]
    fn core_browser_bundle_keeps_default_drops_named_graphs() {
        use purrdf::gts::model::{Term, TermKind};
        use purrdf::gts::writer::Writer;

        // A bundle with one quad in the DEFAULT graph (object-level) and one quad in
        // a heavy named graph (`graph/documentation`). The core browser projection
        // must keep the default-graph quad and DROP the named-graph quad.
        let mut graph = purrdf::gts::model::Graph::default();
        for value in [
            "https://blackcatinformatics.ca/gmeow/Cat", // 0: default s
            "http://www.w3.org/2000/01/rdf-schema#label", // 1: default p
            "https://blackcatinformatics.ca/gmeow/DocNode", // 2: named s
            "https://blackcatinformatics.ca/gmeow/docTitle", // 3: named p
            "https://blackcatinformatics.ca/gmeow/graph/documentation", // 4: named graph
        ] {
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(value.to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            });
        }
        for lit in ["Cat", "A documentation node"] {
            graph.terms.push(Term {
                kind: TermKind::Literal,
                value: Some(lit.to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            });
        }
        // default-graph quad: Cat rdfs:label "Cat" .
        graph.quads.push((0, 1, 5, None));
        // named-graph quad: DocNode docTitle "A documentation node" <graph/documentation>
        graph.quads.push((2, 3, 6, Some(4)));

        let writer = Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
        let nq = core_browser_bundle_nquads(&writer.to_bytes(), &[])
            .expect("core browser bundle must serialize");
        assert!(
            nq.contains("https://blackcatinformatics.ca/gmeow/Cat"),
            "core keeps the default-graph object-level quad:\n{nq}"
        );
        assert!(
            !nq.contains("graph/documentation") && !nq.contains("DocNode"),
            "core drops the heavy named graph and its quads:\n{nq}"
        );
    }

    #[test]
    fn dataset_nquads_from_gts_preserves_named_graph() {
        use purrdf::gts::model::{Term, TermKind};
        use purrdf::gts::writer::Writer;

        // The same one-quad-in-a-named-graph bundle, but read through the
        // graph-PRESERVING browser primitive: the emitted N-Quads MUST carry the
        // named-graph IRI as the fourth term (a flatten would drop it to the default
        // graph and the assertion would fail).
        let mut graph = purrdf::gts::model::Graph::default();
        for value in [
            "https://example.org/s",
            "https://example.org/p",
            "https://example.org/o",
            "https://blackcatinformatics.ca/gmeow/graph/metadata",
        ] {
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(value.to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            });
        }
        graph.quads.push((0, 1, 2, Some(3)));

        let writer = Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
        let nquads = dataset_nquads_from_gts(&writer.to_bytes())
            .expect("graph-preserving bundle N-Quads must serialize");
        assert!(
            nquads.contains("https://blackcatinformatics.ca/gmeow/graph/metadata"),
            "bundle N-Quads must retain the named-graph component (graph-preserving), got:\n{nquads}"
        );
        assert!(
            nquads.contains("https://example.org/s") && nquads.contains("https://example.org/o"),
            "bundle N-Quads must carry the quad's subject and object:\n{nquads}"
        );
    }

    #[test]
    fn dataset_from_gts_rejects_malformed_bytes() {
        // Clearly non-GTS bytes must trigger a fold diagnostic and return Err, not a
        // silent empty dataset. This exercises the fail-fast contract.
        let result = dataset_from_gts(b"this is not a valid gts file");
        assert!(
            result.is_err(),
            "malformed GTS bytes must return Err, not a silent empty dataset"
        );
        // Also verify raw garbage bytes (not even ASCII text).
        let result2 = dataset_from_gts(&[0u8, 1, 2, 3, 0xFF, 0xFE]);
        assert!(
            result2.is_err(),
            "binary garbage bytes must return Err, not a silent empty dataset"
        );
    }

    #[test]
    fn read_gts_graph_rejects_malformed_bytes() {
        let result = read_gts_graph(b"not a gts bundle");
        assert!(
            result.is_err(),
            "malformed bytes must be rejected by read_gts_graph"
        );
        let err = result.err().unwrap();
        let msg = err.message();
        assert!(
            msg.contains("magic")
                || msg.contains("header")
                || msg.contains("parse")
                || msg.contains("diagnostic"),
            "error message should mention magic, header, parse, or diagnostics; got: {msg}"
        );
    }

    #[test]
    fn read_gts_graph_populates_segment_heads() {
        use purrdf::gts::model::{Term, TermKind};
        use purrdf::gts::writer::Writer;

        let mut graph = purrdf::gts::model::Graph::default();
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/s".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/p".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/o".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.quads.push((0, 1, 2, None));

        let writer = Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
        let graph = read_gts_graph(&writer.to_bytes()).expect("valid GTS bytes must parse");

        assert!(
            !graph.segment_heads.is_empty(),
            "read_gts_graph must populate segment_heads"
        );
    }
}
