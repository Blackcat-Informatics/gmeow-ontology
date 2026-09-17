// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Carrier fixture helpers, excluded from the production source closure.

use super::*;

/// The pure set-comparison the OKF-coverage gate delegates to: given the bundle-relative
/// paths the OKF projection actually emits and the ordered list of link targets the docs
/// site would generate (`None` for categories the OKF bundle deliberately skips), return
/// the indices of `links` whose target the OKF bundle does not emit. Kept as a standalone
/// function so the hard-fail logic itself is directly unit-testable, independent of a
/// live `DocsModel`/carrier fixture.
#[cfg(test)]
pub(super) fn okf_link_targets_missing_from(
    emitted: &std::collections::BTreeSet<String>,
    links: &[Option<String>],
) -> Vec<usize> {
    links
        .iter()
        .enumerate()
        .filter_map(|(i, link)| {
            let link = link.as_ref()?;
            let relpath = link.strip_prefix("gmeow-okf/").unwrap_or(link);
            if emitted.contains(relpath) {
                None
            } else {
                Some(i)
            }
        })
        .collect()
}

/// The canonical serialization of the snapshot payload's quad set MINUS the
/// medium-envelope subgraph — the region `gmeow:stratumPayloadExcludingMediumEnvelope`
/// names, taken over the pass-1 union (which is exactly that region, because the
/// envelopes do not exist yet).
///
/// RDFC-1.0 canonical N-Quads, not a raw dump: the stratum digest must be a function
/// of the quad SET, so it has to survive the blank-node relabelling a GTS round-trip
/// is free to perform. A reader recomputes it from the bundle it holds and compares.
///
/// # Errors
/// The union fails dataset validation or canonicalization.
#[cfg(test)]
pub(super) fn stratum_nquads(
    carrier: &purrdf::RdfDataset,
    extra_graphs: &[std::sync::Arc<purrdf::RdfDataset>],
) -> Result<String, gmeow_errors::Diag> {
    let union = flat_stratum_union(carrier, extra_graphs)?;
    Ok(purrdf::canonicalize(&union).nquads)
}

/// Render the mdbook `src/` source tree and pack it into the single `docs-book` archive blob
/// — the producer half of the mdbook documentation projection.
///
/// [`gmeow_docs::mdbook::render_book`] emits a flat, un-prefixed [`gmeow_docs::render::Site`]
/// (`book.toml`, `SUMMARY.md`, `src/<page>/index.md`). We render ONLY the English carrier and
/// prefix every member with English's INTERNAL tag (`x-gmeow-english/…`), taken from
/// `Translations::internal_tag` exactly as [`build_docs_archive`] does, so the archive member
/// scheme matches the ontology-docs archive and a docs consumer selects the same way.
#[cfg(test)]
pub(super) fn build_docs_book_archive(
    root: &Path,
    model: &gmeow_docs::model::DocsModel,
    exec: &gmeow_docs::ExecutableDocsData,
) -> Result<BlobRow, gmeow_errors::Diag> {
    let catalog =
        purrdf::slice::SliceCatalog::discover(&root.join("slices"), gmeow_ns::gmeow_slice_vocab())
            .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let translations = gmeow_docs::Translations::from_catalog(&catalog);
    let prefix = translations.internal_tag(gmeow_docs::i18n::ENGLISH);

    let site = gmeow_docs::mdbook::render_book(model, exec);
    let mut members: Vec<(String, Vec<u8>)> = site
        .files
        .into_iter()
        .map(|(path, bytes)| (format!("{prefix}/{path}"), bytes))
        .collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));
    archive_blob(REP_DOCS_BOOK, &members)
}

/// Render the deterministic Typst source, compile the byte-reproducible print PDF, and pack
/// both into the single `docs-print` archive blob — the producer half of the print
/// documentation projection.
///
/// The renderer reads THIS run's compiled logic/DL axiom surface ([`AXIOM_FILES`], sourced
/// from the `stage-compile-logic` product exactly as [`build_archive_blobs`]'s REP_AXIOMS
/// fold does — never a stale disk read) as its axiom-listing input, and the bibliography
/// database from the `stage-export-references` product. The loss appendix reads the shared
/// per-format capability table. Both `gmeow.pdf` and `gmeow.typ` ride under English's internal
/// tag (`x-gmeow-english/…`) so the member scheme matches the sibling docs archives.
#[cfg(test)]
pub(super) fn build_docs_print_blob(
    model: &gmeow_docs::model::DocsModel,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<(BlobRow, String), gmeow_errors::Diag> {
    let bib = producer_artifact(
        "stage-export-references",
        crate::stages::references::BIB_PATH,
        upstream,
    )?;
    // The axiom-listing input: THIS run's compiled logic/DL projections, keyed by their
    // repo-relative path, pulled from the stage-compile-logic product (fail-closed on absence,
    // mirroring `build_archive_blobs`' REP_AXIOMS guard — a partial listing would silently ship
    // an incomplete PDF).
    let axiom_artifacts = producer_artifacts("stage-compile-logic", upstream)?;
    let mut axioms: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for rel in AXIOM_FILES {
        let bytes = axiom_artifacts.get(rel).ok_or_else(|| {
            stage_err(&format!(
                "missing axiom artifact {rel} in the stage-compile-logic product for the print PDF (fail-closed)"
            ))
        })?;
        axioms.insert(rel.to_string(), bytes.clone());
    }
    let losses: Vec<gmeow_docs::formats::SurfaceCapabilities> = [
        gmeow_docs::formats::DocFormat::Site,
        gmeow_docs::formats::DocFormat::Mdbook,
        gmeow_docs::formats::DocFormat::Pdf,
        gmeow_docs::formats::DocFormat::Snippets,
    ]
    .into_iter()
    .map(gmeow_docs::formats::format_capabilities)
    .collect();

    let typ = docs_print::render_typ(model, &axioms, &bib, &losses);
    let pdf = docs_print::compile_pdf(&typ, &bib)?;
    // The raw `gmeow.pdf` byte digest — BEFORE it is packed into the docs-print tar —
    // so the docs-format grounding graph (F4) can attest the PDF itself, not just the
    // archive that carries it. Computed here, the ONLY point the un-tarred bytes exist.
    let pdf_digest = purrdf::gts::writer::digest_string(&pdf);

    let prefix = model.translations.internal_tag(gmeow_docs::i18n::ENGLISH);
    let members = vec![
        (format!("{prefix}/gmeow.pdf"), pdf),
        (format!("{prefix}/gmeow.typ"), typ.into_bytes()),
    ];
    Ok((archive_blob(REP_DOCS_PRINT, &members)?, pdf_digest))
}

/// One worked example's authored source — its slice IRI, logical path (extension drives
/// the parse dispatch), and raw text. The reason-and-attribute core takes these instead of
/// the whole discovered [`gmeow_docs::model::DocsModel`] so it is exercisable over a fixed
/// fixture without a full pipeline product map.
#[cfg(test)]
pub(crate) struct ExampleSource {
    pub slice: String,
    pub logical_path: String,
    pub text: String,
}

/// The reason-and-attribute core of the executable "try it" docs (see
/// [`build_executable_docs_data`] for how the pipeline gathers the inputs).
///
/// Reason over `(reason_seed ∪ every example ABox)`, subtract the committed `base_closure`
/// (witness-insensitively) and each example's own assertions, and attribute every
/// remaining (example-induced) inference to the example that owns its subject. Inferences
/// with no owning example subject (shared / Skolem witnesses) go to the `cross_example`
/// bucket — never silently dropped.
///
/// `reason_seed` is the authored default-world ontology (not the full object-level EDB):
/// the examples can only propagate through the same-world authored axioms, so this small
/// seed reproduces the full-EDB attribution exactly without re-deriving the base closure.
///
/// This used to take the whole `carrier` as a fourth argument, for the sole purpose of
/// projecting the playground's TriG asset out of it. The asset is retired, so the argument
/// is too: this function attributes inferences and nothing else.
#[cfg(test)]
pub(crate) fn executable_docs_from_sources(
    reason_seed: &purrdf::RdfDataset,
    base_closure_bytes: &[u8],
    examples: &[ExampleSource],
) -> Result<gmeow_docs::ExecutableDocsData, gmeow_errors::Diag> {
    use std::collections::{BTreeMap as StdBTreeMap, BTreeSet, HashSet};

    // Parse every worked example's ABox; remember its subjects + asserted display lines.
    struct ExampleAbox {
        key: String,
        subjects: BTreeSet<String>,
        asserted: Vec<String>,
        dataset: std::sync::Arc<purrdf::RdfDataset>,
    }
    let mut parsed: Vec<ExampleAbox> = Vec::new();
    for ex in examples {
        let ds = parse_example(&ex.logical_path, &ex.text)?;
        let mut subjects = BTreeSet::new();
        let mut asserted = Vec::new();
        for q in ds.owned_quads() {
            if let RdfTerm::Iri(iri) = &q.subject {
                subjects.insert(iri.clone());
            }
            asserted.push(format_triple(&q));
        }
        asserted.sort();
        asserted.dedup();
        parsed.push(ExampleAbox {
            key: gmeow_docs::example_key(&ex.slice, &ex.logical_path),
            subjects,
            asserted,
            dataset: ds,
        });
    }

    // Reason over (reason_seed ∪ every example ABox). push_dataset standardizes blanks
    // apart per merged dataset, so example blanks never collide.
    let mut union = RdfDatasetBuilder::new();
    union.push_dataset(reason_seed);
    for ex in &parsed {
        union.push_dataset(ex.dataset.as_ref());
    }
    let union_ds = union
        .freeze()
        .map_err(|e| stage_err(&format!("freeze try-it union EDB: {e}")))?;
    let reasoned = crate::stages::reason::reason_test_dataset(union_ds.as_ref())?;
    let union_closure = parse_dataset(reasoned.closure.as_bytes(), "text/turtle", None)
        .map_err(|e| stage_err(&format!("try-it union closure parse: {e}")))?;

    // The base ontology-only closure (already committed by the reason stage): subtract
    // it so only EXAMPLE-INDUCED inferences remain (reuse, not a second authority).
    //
    // Witness-insensitive: a Skolem witness edge (an `X ⊑ ∃r.C` restriction materialized
    // as `X ⊑ <skolem>`) carries a content-addressed IRI that depends on the reasoning
    // context, so raw-IRI matching would leak the ontology-level edge into `cross_example`.
    // Normalizing the witness object lets an ontology edge cancel against the base
    // regardless of context, while example-SUBJECT facts (absent from the base) are kept.
    let witness_norm = |line: &str| -> String {
        line.split(' ')
            .map(|t| {
                if t.starts_with("_:") || t.contains("blackcatinformatics.ca/gmeow/skolem/") {
                    "<skolem>".to_string()
                } else {
                    t.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let base_closure = parse_dataset(base_closure_bytes, "text/turtle", None)
        .map_err(|e| stage_err(&format!("base closure parse: {e}")))?;
    let base_set: HashSet<String> = base_closure
        .owned_quads()
        .map(|q| witness_norm(&format_triple(&q)))
        .collect();
    let asserted_set: HashSet<String> = parsed
        .iter()
        .flat_map(|e| e.asserted.iter().cloned())
        .collect();

    // Map each example subject to its owning example key. A subject named by exactly one
    // example maps to `Some(key)`; a subject shared by 2+ examples is ambiguous (we cannot
    // tell which example induced a given inference on it) and is recorded as `None` so it
    // routes to `cross_example` rather than being silently misattributed to whichever
    // example happened to insert last.
    let mut subject_to_example: StdBTreeMap<String, Option<String>> = StdBTreeMap::new();
    for ex in &parsed {
        for s in &ex.subjects {
            subject_to_example
                .entry(s.clone())
                .and_modify(|owner| *owner = None)
                .or_insert_with(|| Some(ex.key.clone()));
        }
    }

    // Attribute each example-induced inference to its example, else the cross bucket.
    let mut per_example: StdBTreeMap<String, Vec<String>> = StdBTreeMap::new();
    let mut cross_example: Vec<String> = Vec::new();
    for q in union_closure.owned_quads() {
        let line = format_triple(&q);
        if base_set.contains(&witness_norm(&line)) || asserted_set.contains(&line) {
            continue; // ontology-only inference or the example's own assertion.
        }
        let subject_iri = match &q.subject {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        };
        // Unknown subject or an ambiguous (multi-example) subject both fall through to
        // `cross_example`; only an unambiguous single-owner subject attributes directly.
        match subject_iri
            .and_then(|s| subject_to_example.get(&s).cloned())
            .flatten()
        {
            Some(key) => per_example.entry(key).or_default().push(line),
            None => cross_example.push(line),
        }
    }
    cross_example.sort();
    cross_example.dedup();

    // Assemble the per-example asserted-vs-inferred diffs.
    let mut example_inferences: StdBTreeMap<String, gmeow_docs::InferenceDiff> = StdBTreeMap::new();
    for ex in &parsed {
        let mut inferred = per_example.remove(&ex.key).unwrap_or_default();
        inferred.sort();
        inferred.dedup();
        let diff = gmeow_docs::InferenceDiff {
            asserted: ex.asserted.clone(),
            inferred,
        };
        if !diff.is_empty() {
            example_inferences.insert(ex.key.clone(), diff);
        }
    }

    Ok(gmeow_docs::ExecutableDocsData {
        example_inferences,
        cross_example,
        // `term_entailments` is NOT this core's concern (it needs the discovered
        // term-IRI set, not just the reduced reasoning seed): `build_executable_docs_data`
        // fills it in afterward via `term_entailments_from_upstream`, so this fixture-only
        // core stays exercisable without a full docs model.
        ..Default::default()
    })
}

/// Parse one worked example into a dataset, dispatching on its file extension —
/// examples are authored in Turtle, but also JSON-LD-star and YAML-LD-star.
#[cfg(test)]
pub(super) fn parse_example(
    logical_path: &str,
    text: &str,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let ext = logical_path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let media = match ext.as_str() {
        "ttl" | "turtle" => "text/turtle",
        "nt" | "ntriples" => "application/n-triples",
        "nq" | "nquads" => "application/n-quads",
        "trig" => "application/trig",
        "rdf" | "xml" => "application/rdf+xml",
        "jsonld" => {
            return purrdf::native_codecs::jsonld::parse_jsonld(text.as_bytes(), None)
                .map_err(|e| stage_err(&format!("example jsonld parse {logical_path}: {e}")));
        }
        "yamlld" | "yaml" | "yml" => {
            let json = purrdf::native_codecs::jsonld::yamlld_to_jsonld(text.as_bytes())
                .map_err(|e| stage_err(&format!("example yamlld convert {logical_path}: {e}")))?;
            return purrdf::native_codecs::jsonld::parse_jsonld(json.as_bytes(), None)
                .map_err(|e| stage_err(&format!("example yamlld parse {logical_path}: {e}")));
        }
        other => {
            return Err(stage_err(&format!(
                "example {logical_path}: unsupported format .{other}"
            )));
        }
    };
    parse_dataset(text.as_bytes(), media, None)
        .map_err(|e| stage_err(&format!("example parse {logical_path}: {e}")))
}

/// Format an owned quad's `(s, p, o)` as a compact, deterministic display line for the
/// "try it" asserted-vs-inferred surfaces. The graph is dropped (these are triples).
#[cfg(test)]
pub(super) fn format_triple(q: &RdfQuad) -> String {
    triple_display(&q.subject, &q.predicate, &q.object)
}

/// Canonicalize N-Quads bytes and route them into `graph_name` on `builder` — the
/// oxigraph-ingestion path the byte-golden tests use to author fixture snapshots.
/// Production assembly now goes through the native carrier ([`assemble_carrier`]).
#[cfg(test)]
pub(super) fn add_named(
    builder: &mut SnapshotBuilder,
    nq_bytes: &[u8],
    graph_name: &str,
    scope: &str,
) -> Result<(), gmeow_errors::Diag> {
    let canon = canonicalize_nq(nq_bytes, scope)?;
    let quads = parse_nq(canon.as_bytes())?;
    reject_quoted_triples(&quads, graph_name)?;
    let dataset = parse_dataset(canon.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("add_named parse: {e}")))?;
    builder
        .add_dataset_scoped(&dataset, Some(graph_name), Some(scope))
        .map_err(|e| stage_err(&e))?;
    Ok(())
}

/// Ingest a default-graph N-Quads fixture under a blank scope (test-only): canonicalize
/// → native parse → `add_dataset_scoped`, the carrier-test analogue of [`add_named`] for
/// the default graph (no graph name).
#[cfg(test)]
pub(super) fn add_base_nq(
    builder: &mut SnapshotBuilder,
    nq_bytes: &[u8],
    scope: &str,
) -> Result<(), gmeow_errors::Diag> {
    let canon = canonicalize_nq(nq_bytes, scope)?;
    let quads = parse_nq(canon.as_bytes())?;
    reject_quoted_triples(&quads, "default")?;
    let dataset = parse_dataset(canon.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("add_base_nq parse: {e}")))?;
    builder
        .add_dataset_scoped(&dataset, None, Some(scope))
        .map_err(|e| stage_err(&e))?;
    Ok(())
}

/// The standardize-apart union of several Turtle sources into ONE default-graph
/// dataset. Each source is parsed independently (its own blank scope) and merged via
/// [`RdfDataset::union`], whose per-input `BlankScope` keeps structurally-distinct
/// blank-node axioms (e.g. two `owl:AllDisjointClasses` lists) disjoint — the native
/// replacement for the removed `ingest_turtle_scoped` string-prefix scoping.
#[cfg(test)]
pub(super) fn union_turtle_datasets(
    sources: &[Vec<u8>],
) -> Result<purrdf::RdfDataset, gmeow_errors::Diag> {
    let owned: Vec<std::sync::Arc<purrdf::RdfDataset>> = sources
        .iter()
        .map(|bytes| parse_turtle_dataset(bytes))
        .collect::<Result<_, _>>()?;
    let refs: Vec<&purrdf::RdfDataset> = owned.iter().map(AsRef::as_ref).collect();
    Ok(purrdf::RdfDataset::union(&refs))
}
