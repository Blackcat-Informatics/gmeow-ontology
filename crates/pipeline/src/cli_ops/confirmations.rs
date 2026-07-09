// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Thin CLI-glue confirmations over ALREADY-native authorities.
//!
//! The Python surfaces `up_projection_audit.py`, `mappings.py`, `gts_views.py`
//! (`export_docs_site`) is a thin glue over Rust that
//! already exists. This module exposes each confirmed native authority under one
//! stable call name so a `gmeow` / `gmeow-dev` binary can invoke it directly — it
//! duplicates NONE of the native logic; every function delegates.

use std::collections::BTreeMap;
use std::path::Path;

/// The up-projection invertibility audit — the gate-derived verdict ledger plus its
/// rendered Markdown report.
///
/// Confirms and exposes the native authority behind `up_projection_audit.py`:
/// [`crate::up_projection_gates::gate_derived_audit`] +
/// [`crate::up_projection_report::render_audit_markdown`]. No count is computed here
/// (the Python glue computed none either); the corpus Turtle→N-Triples conversion is
/// the native [`crate::up_projection_corpus::ttl_to_nt`]. Inputs are passed in by the
/// caller (SSSOM texts, projection TTLs, `(name, turtle)` corpus), keeping the
/// driver consumer-safe.
///
/// Returns the gate-verdict [`AuditLedger`](crate::up_projection_gates::AuditLedger)
/// alongside the rendered Markdown.
///
/// # Errors
///
/// - A corpus Turtle fails to convert, or the gate audit fails.
pub fn up_projection_gate_audit(
    sssom_texts: &[String],
    projection_ttls: &[String],
    corpus_ttls: &[(String, String)],
) -> Result<(crate::up_projection_gates::AuditLedger, String), gmeow_errors::Diag> {
    let mut corpus_nts = Vec::with_capacity(corpus_ttls.len());
    for (name, ttl) in corpus_ttls {
        let nt = crate::up_projection_corpus::ttl_to_nt(ttl).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "up-projection-audit".to_string(),
                message: format!("corpus {name} ttl→nt: {e}"),
            })
        })?;
        corpus_nts.push((name.clone(), nt));
    }
    let ledger =
        crate::up_projection_gates::gate_derived_audit(sssom_texts, projection_ttls, &corpus_nts)
            .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "up-projection-audit".to_string(),
                message: e.to_string(),
            })
        })?;
    let markdown = crate::up_projection_report::render_audit_markdown(&ledger);
    Ok((ledger, markdown))
}

/// Compile every mapping family (SSSOM + FnO + EDOAL + SPARQL + standpoint) plus the
/// DSL surface-count summary and the per-correspondence loss ledger from `root`.
///
/// Confirms and exposes the native authority behind `mappings.py`, which is
/// superseded: [`crate::stages::mappings::compile_mappings`]. The Rust stage owns all
/// five families and the loss ledger; this is a one-name pass-through so a bin need
/// not reach into `stages::mappings` directly.
///
/// # Errors
///
/// - Any mapping compile failure (prefix-consistency / purity gate, lowering error).
pub fn compile_mappings(
    root: &Path,
) -> Result<crate::stages::mappings::CompiledMappings, gmeow_errors::Diag> {
    crate::stages::mappings::compile_mappings(root)
}

/// The `x-gmeow-english/` prefix the English-only book / print doc blobs are keyed
/// by (the mdbook source tree and the Typst/PDF print projection carry a single
/// English carrier language, never a per-language fan-out).
const ENGLISH_DOC_PREFIX: &str = "x-gmeow-english/";

/// Strip `prefix` from each member path, dropping members outside it — the shared
/// language-subtree selector for the docs export families.
fn select_prefixed_subtree(
    members: BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> BTreeMap<String, Vec<u8>> {
    members
        .into_iter()
        .filter_map(|(member, bytes)| {
            member
                .strip_prefix(prefix)
                .map(|rel| (rel.to_string(), bytes))
        })
        .collect()
}

/// Re-key every `terms/<slug>/card.md` member of an already-language-stripped docs
/// tree to a flattened `terms/<slug>.md` snippet.
///
/// **A11**: every term subtree (any `terms/<slug>/…` member) MUST carry a `card.md`;
/// a term with none is a HARD FAIL naming the term (a card is expected per term, never
/// silently skipped).
///
/// # Errors
///
/// - A term subtree carries no `card.md`.
fn card_snippets(
    lang_tree: &BTreeMap<String, Vec<u8>>,
    internal_lang: &str,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let mut slugs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for key in lang_tree.keys() {
        if let Some(rest) = key.strip_prefix("terms/")
            && let Some((slug, _)) = rest.split_once('/')
        {
            slugs.insert(slug.to_string());
        }
    }
    let mut snippets: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for slug in &slugs {
        let card = format!("terms/{slug}/card.md");
        match lang_tree.get(&card) {
            Some(bytes) => {
                snippets.insert(format!("terms/{slug}.md"), bytes.clone());
            }
            None => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "export-docs".to_string(),
                    message: format!(
                        "term {slug:?} in {internal_lang} has a docs subtree but no \
                         terms/{slug}/card.md; every term is expected to carry a prompt-ready card"
                    ),
                }));
            }
        }
    }
    Ok(snippets)
}

/// Extract one language subtree of the bundled `ontology-docs` static site from a
/// `gmeow.gts` snapshot as `{relative-path → bytes}`.
///
/// Confirms and exposes the native authority behind `export_docs_site`:
/// [`crate::bundle_blobs::bundled_ontology_docs`], which unpacks the Rust-rendered
/// site blob folded at `regenerate` time (nothing is re-projected here). Member paths
/// carry the internal language prefix (`x-gmeow-english/index.html`, …); this selects
/// `internal_lang` and strips its `<lang>/` prefix so the caller writes a clean tree.
///
/// # Errors
///
/// - The snapshot cannot be folded, or carries no `ontology-docs` blob.
/// - No members exist for `internal_lang` (a missing language is a HARD FAIL, never a
///   silent empty extraction).
pub fn export_docs_site(
    snapshot: &[u8],
    internal_lang: &str,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let site = crate::bundle_blobs::bundled_ontology_docs(snapshot).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!("fold ontology-docs blob: {e}"),
        })
    })?;
    let selected = select_prefixed_subtree(site, &format!("{internal_lang}/"));
    if selected.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!(
                "no ontology-docs members for language {internal_lang:?}; \
                 regenerate the bundle or pick an available language"
            ),
        }));
    }
    Ok(selected)
}

/// Extract the bundled mdbook source tree (`book.toml`, `SUMMARY.md`, `src/…`) from a
/// `gmeow.gts` snapshot as `{relative-path → bytes}`.
///
/// Unpacks [`crate::bundle_blobs::bundled_docs_book`] and strips the English carrier
/// prefix ([`ENGLISH_DOC_PREFIX`]); the book is English-only, so no language selector
/// applies.
///
/// # Errors
///
/// - The snapshot cannot be folded, or carries no `docs-book` blob / no English members
///   (a missing book is a HARD FAIL, never a silent empty extraction).
pub fn export_docs_book(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let book = crate::bundle_blobs::bundled_docs_book(snapshot).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!("fold docs-book blob: {e}"),
        })
    })?;
    let selected = select_prefixed_subtree(book, ENGLISH_DOC_PREFIX);
    if selected.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: "no docs-book members under the English carrier; regenerate the bundle"
                .to_string(),
        }));
    }
    Ok(selected)
}

/// Extract the bundled print documentation projection (`gmeow.pdf`, `gmeow.typ`) from a
/// `gmeow.gts` snapshot as `{relative-path → bytes}`.
///
/// Unpacks [`crate::bundle_blobs::bundled_docs_print`] and strips the English carrier
/// prefix ([`ENGLISH_DOC_PREFIX`]); the print projection is English-only.
///
/// # Errors
///
/// - The snapshot cannot be folded, or carries no `docs-print` blob / no English members
///   (a missing print projection is a HARD FAIL, never a silent empty extraction).
pub fn export_docs_print(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let print = crate::bundle_blobs::bundled_docs_print(snapshot).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!("fold docs-print blob: {e}"),
        })
    })?;
    let selected = select_prefixed_subtree(print, ENGLISH_DOC_PREFIX);
    if selected.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: "no docs-print members under the English carrier; regenerate the bundle"
                .to_string(),
        }));
    }
    Ok(selected)
}

/// Extract the flattened prompt-ready term-card snippets for one language of the
/// bundled `ontology-docs` site as `{terms/<slug>.md → bytes}`.
///
/// Folds [`crate::bundle_blobs::bundled_ontology_docs`], selects `internal_lang`, and
/// re-keys every `terms/<slug>/card.md` to `terms/<slug>.md` (see [`card_snippets`],
/// which enforces the per-term A11 card requirement).
///
/// # Errors
///
/// - The snapshot cannot be folded, or carries no `ontology-docs` blob.
/// - No members exist for `internal_lang`, a term subtree carries no `card.md`, or the
///   language yields no snippets at all (each is a HARD FAIL).
pub fn export_docs_snippets(
    snapshot: &[u8],
    internal_lang: &str,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let site = crate::bundle_blobs::bundled_ontology_docs(snapshot).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!("fold ontology-docs blob: {e}"),
        })
    })?;
    let lang_tree = select_prefixed_subtree(site, &format!("{internal_lang}/"));
    if lang_tree.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!(
                "no ontology-docs members for language {internal_lang:?}; \
                 regenerate the bundle or pick an available language"
            ),
        }));
    }
    let snippets = card_snippets(&lang_tree, internal_lang)?;
    if snippets.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!(
                "no term-card snippets for language {internal_lang:?}; \
                 regenerate the bundle or pick an available language"
            ),
        }));
    }
    Ok(snippets)
}

/// The internal language tags (`x-gmeow-*`) that actually carry an `ontology-docs`
/// subtree in the snapshot — the selectable set for [`export_docs_site`].
///
/// # Errors
///
/// - The snapshot cannot be folded, or carries no `ontology-docs` blob.
pub fn available_doc_languages(snapshot: &[u8]) -> Result<Vec<String>, gmeow_errors::Diag> {
    let site = crate::bundle_blobs::bundled_ontology_docs(snapshot).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export-docs".to_string(),
            message: format!("fold ontology-docs blob: {e}"),
        })
    })?;
    let mut langs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for member in site.keys() {
        if let Some((lang, _rest)) = member.split_once('/') {
            langs.insert(lang.to_string());
        }
    }
    Ok(langs.into_iter().collect())
}

/// Summarise every term in a folded GTS snapshot as `(curie, label, definition)`.
///
/// Confirms and exposes the native authority behind `bundle_term_summaries`:
/// [`crate::stages::export::FoldView`] + [`crate::stages::export::collect_terms`]. A
/// term with an empty `label` / `definition` is a "missing" one for the source-free
/// bundle checks (`gmeow verify`). No logic is duplicated here — this is a one-name
/// pass-through so a `gmeow` / `gmeow-dev` binary need not reach into `stages::export`.
///
/// # Errors
///
/// - The snapshot cannot be folded into the carrier dataset.
pub fn bundle_term_summaries(
    gts_bytes: &[u8],
) -> Result<Vec<(String, String, String)>, gmeow_errors::Diag> {
    let dataset = purrdf::gts::flattened_dataset_from_bytes(gts_bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "bundle-checks".to_string(),
            message: format!("fold gts snapshot: {e}"),
        })
    })?;
    let view = crate::stages::export::FoldView::new(&dataset);
    let terms = crate::stages::export::collect_terms(&view);
    Ok(terms
        .into_iter()
        .map(|t| (t.curie, t.label, t.definition))
        .collect())
}

/// Render every flat consumer export view from a folded GTS snapshot and write each to
/// `out_dir`, returning the written paths (sorted).
///
/// Confirms and exposes the native authority behind the retired Python
/// `gmeow_tools.export` orchestration:
/// [`crate::stages::export::render_all_with_languages`] (honoring the requested public
/// BCP-47 `languages`; an empty list falls back to `["en"]`). Each artifact is keyed by
/// its canonical `dist/<basename>` name; this writes `<out_dir>/<basename>`. No logic is
/// duplicated here — every rendered byte comes from the native export stage.
///
/// # Errors
///
/// - The snapshot cannot be folded, an export view fails to render, `out_dir` cannot be
///   created, or an artifact cannot be written.
pub fn export_views(
    gts_bytes: &[u8],
    out_dir: &Path,
    languages: &[String],
) -> Result<Vec<String>, gmeow_errors::Diag> {
    let dataset = purrdf::gts::flattened_dataset_from_bytes(gts_bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export".to_string(),
            message: format!("fold gts snapshot: {e}"),
        })
    })?;
    let langs: Vec<String> = if languages.is_empty() {
        vec!["en".to_string()]
    } else {
        languages.to_vec()
    };
    let artifacts = crate::stages::export::render_all_with_languages(&dataset, &langs)?;
    std::fs::create_dir_all(out_dir).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export".to_string(),
            message: format!("cannot create {}: {e}", out_dir.display()),
        })
    })?;
    let mut written: Vec<String> = Vec::with_capacity(artifacts.len());
    for (dist_path, bytes) in &artifacts {
        let name = Path::new(dist_path).file_name().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "export".to_string(),
                message: format!("export artifact {dist_path:?} has no file name"),
            })
        })?;
        let dest = out_dir.join(name);
        std::fs::write(&dest, bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "export".to_string(),
                message: format!("cannot write {}: {e}", dest.display()),
            })
        })?;
        written.push(dest.display().to_string());
    }
    written.sort();
    Ok(written)
}

/// Rewrite a Turtle document in native canonical, review-friendly form.
///
/// Confirms and exposes the native authority behind `normalize.canonicalize`:
/// `purrdf::turtle_normalize::canonical_turtle` (the oxigraph-free replacement for
/// rdflib `longturtle`, serialized over the gmeow-rdf IR). `extra_prefixes` supplies
/// the project's standard prefix bindings (only those a file uses are emitted).
///
/// # Errors
///
/// - The Turtle fails to parse.
pub fn canonicalize_turtle(
    bytes: &[u8],
    extra_prefixes: &[(String, String)],
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let canonical =
        purrdf::turtle_normalize::canonical_turtle(bytes, extra_prefixes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "normalize".to_string(),
                message: format!("canonical turtle failed: {e}"),
            })
        })?;
    Ok(canonical.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed `generated/dist/gmeow.gts` snapshot bytes, read from the worktree.
    fn committed_snapshot() -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("generated/dist/gmeow.gts");
        std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn prefixes() -> Vec<(String, String)> {
        crate::stages::superset::rdf_prefixes()
    }

    const SAMPLE_TTL: &str = r#"@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .

<http://example.org/B> a owl:Class ; rdfs:label "B" .
<http://example.org/A> a owl:Class ; rdfs:label "A" .
"#;

    #[test]
    fn normalize_is_idempotent() {
        let once = canonicalize_turtle(SAMPLE_TTL.as_bytes(), &prefixes()).expect("first pass");
        let twice = canonicalize_turtle(&once, &prefixes()).expect("second pass");
        assert_eq!(
            once, twice,
            "canonicalization must be a fixed point (idempotent)"
        );
        // The canonical form is non-empty valid Turtle.
        let text = String::from_utf8(once).expect("utf-8");
        assert!(text.contains("example.org/A"), "content preserved");
        assert!(text.contains("example.org/B"), "content preserved");
    }

    #[test]
    fn export_docs_site_selects_and_strips_a_language() {
        let snapshot = committed_snapshot();
        let langs = available_doc_languages(&snapshot).expect("languages");
        assert!(
            langs.iter().any(|l| l.starts_with("x-gmeow-")),
            "at least one internal-tagged doc language, got: {langs:?}"
        );
        let english = "x-gmeow-english";
        let tree = export_docs_site(&snapshot, english).expect("english subtree");
        assert!(!tree.is_empty(), "english docs subtree is non-empty");
        assert!(
            tree.contains_key("slice-quality/index.html"),
            "export-docs must expose the bundled slice-quality HTML report"
        );
        // The language prefix is stripped from every member path.
        assert!(
            tree.keys().all(|k| !k.starts_with("x-gmeow-")),
            "language prefix stripped from member paths"
        );
    }

    #[test]
    fn export_docs_site_hard_fails_on_unknown_language() {
        let snapshot = committed_snapshot();
        let err = export_docs_site(&snapshot, "x-gmeow-nonexistent")
            .expect_err("unknown language must fail");
        assert!(err.to_string().contains("no ontology-docs members"));
    }

    #[test]
    fn export_docs_snippets_flattens_term_cards() {
        let snapshot = committed_snapshot();
        let snippets =
            export_docs_snippets(&snapshot, "x-gmeow-english").expect("english snippets");
        assert!(!snippets.is_empty(), "english snippets are non-empty");
        // Every emitted key is a flattened `terms/<slug>.md` (no nested card path,
        // no language prefix).
        assert!(
            snippets.keys().all(|k| {
                k.starts_with("terms/")
                    && k.ends_with(".md")
                    && !k.contains("/card.md")
                    && !k.starts_with("x-gmeow-")
            }),
            "snippets re-key card.md members to flattened terms/<slug>.md: {:?}",
            snippets.keys().take(3).collect::<Vec<_>>()
        );
    }

    #[test]
    fn export_docs_snippets_hard_fails_on_unknown_language() {
        let snapshot = committed_snapshot();
        let err = export_docs_snippets(&snapshot, "x-gmeow-nonexistent")
            .expect_err("unknown language must fail");
        assert!(err.to_string().contains("no ontology-docs members"));
    }

    /// The pure card-snippet selector re-keys `terms/<slug>/card.md` to
    /// `terms/<slug>.md`, ignoring non-card members of the same subtree.
    #[test]
    fn card_snippets_selection_rekeys_cards() {
        let mut tree: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        tree.insert("index.html".to_string(), b"<html>".to_vec());
        tree.insert("terms/foo/index.html".to_string(), b"<foo>".to_vec());
        tree.insert("terms/foo/index.md".to_string(), b"# foo".to_vec());
        tree.insert("terms/foo/card.md".to_string(), b"foo card".to_vec());
        let snippets = card_snippets(&tree, "x-gmeow-english").expect("snippets");
        assert_eq!(snippets.len(), 1, "one term, one snippet");
        assert_eq!(
            snippets.get("terms/foo.md").map(|b| b.as_slice()),
            Some(b"foo card".as_slice()),
            "the card body is re-keyed to terms/foo.md"
        );
    }

    /// **A11**: a term subtree with no `card.md` is a HARD FAIL naming the term.
    #[test]
    fn card_snippets_hard_fails_on_missing_card() {
        let mut tree: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        tree.insert("terms/bar/index.md".to_string(), b"# bar".to_vec());
        tree.insert("terms/bar/index.html".to_string(), b"<bar>".to_vec());
        let err = card_snippets(&tree, "x-gmeow-english")
            .expect_err("a term subtree without a card must hard-fail");
        let msg = err.to_string();
        assert!(
            msg.contains("bar"),
            "the message names the card-less term: {msg}"
        );
        assert!(
            msg.contains("card"),
            "the message explains the missing card: {msg}"
        );
    }

    /// The English-only book / print selector strips `x-gmeow-english/` and hard-fails
    /// when a synthetic archive carries no English members.
    #[test]
    fn english_subtree_selection_and_empty_guard() {
        let mut archive: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        archive.insert("x-gmeow-english/book.toml".to_string(), b"[book]".to_vec());
        archive.insert(
            "x-gmeow-english/src/index.md".to_string(),
            b"# root".to_vec(),
        );
        let selected = select_prefixed_subtree(archive, ENGLISH_DOC_PREFIX);
        assert!(selected.contains_key("book.toml"), "prefix stripped");
        assert!(selected.contains_key("src/index.md"), "nested member kept");
        assert!(
            selected.keys().all(|k| !k.starts_with("x-gmeow-")),
            "no member retains the English carrier prefix"
        );

        // A synthetic archive with only a foreign prefix yields no English members.
        let mut other: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        other.insert("x-gmeow-french/book.toml".to_string(), b"[livre]".to_vec());
        assert!(
            select_prefixed_subtree(other, ENGLISH_DOC_PREFIX).is_empty(),
            "a non-English archive strips to an empty English subtree (hard-fail input)"
        );
    }

    #[test]
    fn up_projection_gate_audit_smoke() {
        // A minimal corpus with no lift rules: the audit runs end to end and renders a
        // Markdown report without panicking (the wiring smoke; the heavy real-corpus
        // audit is exercised by the up_projection_gates own tests).
        let corpus = vec![(
            "smoke".to_string(),
            "@prefix ex: <http://example.org/> .\nex:a ex:p ex:b .\n".to_string(),
        )];
        let (ledger, markdown) =
            up_projection_gate_audit(&[], &[], &corpus).expect("audit runs end to end");
        assert!(!markdown.trim().is_empty(), "a report is rendered");
        // Sanity: the ledger total is the partition sum of its tiers.
        assert_eq!(ledger.total(), ledger.totals.total());
    }
}
