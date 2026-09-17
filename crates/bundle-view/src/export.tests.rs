// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::path::Path;
use std::sync::{Arc, OnceLock};

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The authenticated bundle's `gmeow.schema.json` `$defs` key set — the same
/// model-existence signal production reads, for tests exercising
/// `term_to_card`/`consumer_llms_full`/`doc_card_build` against the real
/// `english_terms()` corpus (so a modeled term like `gmeow:EntityExistence`
/// still carries its `python_model` link in these tests, not a synthetic one).
fn authenticated_modeled_defs() -> &'static BTreeSet<String> {
    static DEFS: OnceLock<BTreeSet<String>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let bytes = gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
            .expect("authenticated producer bundle; tests never rebuild it");
        let bundle = crate::bundle_blobs::Bundle::from_snapshot(&bytes)
            .expect("authenticated producer bundle parses");
        let schema = bundle
            .schema()
            .expect("authenticated schema archive reads")
            .expect("authenticated bundle carries gmeow.schema.json");
        let parsed: serde_json::Value =
            serde_json::from_slice(&schema).expect("gmeow.schema.json parses as JSON");
        parsed
            .get("$defs")
            .and_then(|v| v.as_object())
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default()
    })
}

fn authenticated_fold() -> Arc<RdfDataset> {
    static FOLD: OnceLock<Arc<RdfDataset>> = OnceLock::new();
    Arc::clone(FOLD.get_or_init(|| {
        gmeow_bundle_import::load_authenticated_repository_bundle(&repo_root())
            .expect("authenticated repository corpus; tests never rebuild it")
            .dataset
    }))
}

fn json_str_ascii_matches_cpython_short_escapes() {
    // `json.dumps(s)` (ensure_ascii=True) uses the short escapes `\b`/`\f`/
    // `\n`/`\r`/`\t` for these control chars, NOT `\uXXXX`. Pin byte-parity.
    assert_eq!(json_str_ascii("\u{08}"), r#""\b""#);
    assert_eq!(json_str_ascii("\u{0c}"), r#""\f""#);
    assert_eq!(json_str_ascii("\n\r\t"), r#""\n\r\t""#);
    // Other C0 controls still fall through to lowercase `\uXXXX`.
    assert_eq!(json_str_ascii("\u{00}\u{1f}"), "\"\\u0000\\u001f\"");
}

/// The selector threads `requested` through `collect_terms`: the English
/// default keeps the carrier label, a `fr` request selects the French
/// translation, and an absent translation falls back to English (flagged).
/// Pins the multilingual generalization of `FoldView` and guards the English
/// path from regression (the default view is unchanged for
/// `gmeow:EntityExistence`, a documented term carrying a French translation).
fn selector_threads_requested_language() {
    let graph = authenticated_fold();

    let label_of = |requested: Vec<String>, curie_q: &str| -> Term {
        let view = FoldView::with_requested(&graph, requested);
        collect_terms(&view)
            .into_iter()
            .find(|t| t.curie == curie_q)
            .unwrap_or_else(|| panic!("term {curie_q} not in snapshot"))
    };

    // English default: the carrier label, not a fallback.
    let en = label_of(vec!["en".to_string()], "gmeow:EntityExistence");
    assert_eq!(en.label, "Entity Existence");
    assert!(!en.label_fallback);

    // `new()` (the export generator's view) agrees with `["en"]` — the
    // English path is unchanged by the generalization.
    let default_view = FoldView::new(&graph);
    let default_fr = collect_terms(&default_view)
        .into_iter()
        .find(|t| t.curie == "gmeow:EntityExistence")
        .expect("term present");
    assert_eq!(default_fr.label, en.label);
    assert_eq!(default_fr.label_fallback, en.label_fallback);

    // `fr` request: the French translation (from the lifecycle fr.po), non-fallback.
    let fr = label_of(vec!["fr".to_string()], "gmeow:EntityExistence");
    assert_eq!(fr.label, "Existence d'entité");
    assert!(!fr.label_fallback);
}

fn english_terms() -> (&'static [Term], &'static str, &'static str) {
    static CORPUS: OnceLock<(Vec<Term>, String, String)> = OnceLock::new();
    let (terms, title, version) = CORPUS.get_or_init(|| {
        let graph = authenticated_fold();
        let view = FoldView::new(&graph);
        let (title, version) = fold_meta(&view).expect("fold_meta");
        (collect_terms(&view), title, version)
    });
    (terms, title, version)
}

/// The exact producer-selected GMN-1 primer from the shared teachability artifact.
/// Tests consume its typed rows without rebuilding the dictionary or primer.
fn english_primer() -> &'static gmeow_docs_model::gmn1_primer::Gmn1Primer {
    static PRIMER: OnceLock<gmeow_docs_model::gmn1_primer::Gmn1Primer> = OnceLock::new();
    PRIMER.get_or_init(|| {
        let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(
            &repo_root(),
            gmeow_docs_model::gmn1_primer::teachability::ARTIFACT,
        )
        .expect("authenticated producer primer; tests never rebuild it");
        let observation: gmeow_docs_model::gmn1_primer::teachability::Observations =
            serde_json::from_slice(&bytes).expect("typed producer primer artifact");
        observation.primer
    })
}

/// The consumer/MCP term surface resolves grounding-namespace terms (the twin of
/// `gmeow describe`): CURIE, full IRI, and bare local name across `logic:`/
/// `math:`/`lang:`, from the real folded bundle. Before the fix `collect_terms`
/// filtered to `gmeow:`, so these were absent entirely.
fn resolve_term_iri_spans_grounding_namespaces() {
    let (terms, _t, _v) = english_terms();
    assert_eq!(
        resolve_term_iri(terms, "lang:Denotation").resolved(),
        Some("https://blackcatinformatics.ca/lang/Denotation")
    );
    assert_eq!(
        resolve_term_iri(terms, "math:Function").resolved(),
        Some("https://blackcatinformatics.ca/math/Function")
    );
    // Full IRI.
    assert_eq!(
        resolve_term_iri(terms, "https://blackcatinformatics.ca/logic/Formula").resolved(),
        Some("https://blackcatinformatics.ca/logic/Formula")
    );
    // Bare local name (namespace-agnostic), unambiguous → resolves.
    assert_eq!(
        resolve_term_iri(terms, "Denotation").resolved(),
        Some("https://blackcatinformatics.ca/lang/Denotation")
    );
    // A grounding term carries a real CURIE (proving the `lang` prefix fix).
    let denotation = terms
        .iter()
        .find(|t| t.iri == "https://blackcatinformatics.ca/lang/Denotation")
        .expect("lang:Denotation folded into the term set");
    assert_eq!(denotation.curie, "lang:Denotation");
    assert!(
        !denotation.label.is_empty(),
        "the folded grounding term carries a label"
    );
}

/// `lookup_term`: exact CURIE match → `as_record` with `ok:true`;
/// unknown → `{"ok": false, "error": "Term not found: …"}`; per-language label.
fn lookup_envelope_matches_consumer_contract() {
    let (terms, _t, _v) = english_terms();

    let hit: serde_json::Value =
        serde_json::from_str(&lookup_envelope(terms, "gmeow:EntityExistence")).unwrap();
    assert_eq!(hit["ok"], serde_json::json!(true));
    assert_eq!(hit["curie"], serde_json::json!("gmeow:EntityExistence"));
    assert_eq!(hit["label"], serde_json::json!("Entity Existence"));
    assert_eq!(hit["category"], serde_json::json!("class"));

    // Local-name resolution (IRI minus the gmeow namespace) is accepted.
    let by_local: serde_json::Value =
        serde_json::from_str(&lookup_envelope(terms, "EntityExistence")).unwrap();
    assert_eq!(
        by_local["curie"],
        serde_json::json!("gmeow:EntityExistence")
    );

    let miss: serde_json::Value =
        serde_json::from_str(&lookup_envelope(terms, "gmeow:NoSuchTerm")).unwrap();
    assert_eq!(miss["ok"], serde_json::json!(false));
    assert_eq!(
        miss["error"],
        serde_json::json!("Term not found: gmeow:NoSuchTerm")
    );

    // Per-language record: `fr` selects the French label, and the envelope is
    // ASCII-escaped (`json.dumps` default) — `é` is emitted as `é`.
    let graph = authenticated_fold();
    let fr_terms = collect_terms(&FoldView::with_requested(&graph, vec!["fr".to_string()]));
    let fr_raw = lookup_envelope(&fr_terms, "gmeow:EntityExistence");
    assert!(
        fr_raw.contains("\\u00e9"),
        "lookup envelope must be ASCII-escaped (ensure_ascii)"
    );
    assert!(
        !fr_raw.contains('é'),
        "raw non-ASCII leaked into lookup envelope"
    );
    let fr: serde_json::Value = serde_json::from_str(&fr_raw).unwrap();
    assert_eq!(fr["label"], serde_json::json!("Existence d'entité"));
}

/// `llms_txt`: the STANDARD llmstxt.org format — H1 + canonical
/// summary blockquote + unified `⊑`/`→` signatures + bullets linking into the
/// published docs site (URLs recovered from the doc graph). One format across
/// the dist/MCP/site surfaces; the old consumer-specific format is retired.
fn consumer_llms_txt_uses_standard_format() {
    let (terms, title, version) = english_terms();
    let graph = authenticated_fold();
    let doc_urls = doc_url_map(&FoldView::new(&graph));
    let txt = consumer_llms_txt(terms, title, version, &doc_urls);

    // Standard header: a single H1 then the canonical summary blockquote.
    assert!(
        txt.starts_with(&format!(
            "# {title}\n\n> {}\n\n",
            gmeow_docs_model::llms::GMEOW_SUMMARY
        )),
        "header mismatch:\n{}",
        &txt[..240]
    );
    assert!(txt.contains(&format!("Vocabulary {version}. Namespace: {NAMESPACE}.")));
    assert!(txt.contains("\n## Classes\n\n"));
    assert!(txt.contains("\n## Properties\n\n"));
    assert!(txt.ends_with('\n'));

    // Unified signature marker (`(⊑ ` for a class) — the consumer index now
    // matches the dist/site format; the old `subClassOf`/ASCII-`->` markers
    // and the export box-roles suffix are gone.
    assert!(txt.contains("(⊑ "), "missing unified subclass marker");
    assert!(
        !txt.contains("(subClassOf "),
        "old consumer subclass marker leaked"
    );
    assert!(
        !txt.contains("[box roles:"),
        "leaked export-format box roles"
    );

    // When the doc graph is present in the snapshot, bullets are markdown links
    // into the published site (the same URLs the docs site emits).
    if !doc_urls.is_empty() {
        assert!(
            txt.contains("](terms/"),
            "doc-graph URLs should link the term bullets"
        );
    }

    // `fr` request threads the French selector: the corpus has almost no French
    // text, so the observable effect is the `[fallback: en]` markers added when
    // a term's requested-language text resolves via the English fallback.
    let fr_terms = collect_terms(&FoldView::with_requested(&graph, vec!["fr".to_string()]));
    let fr_txt = consumer_llms_txt(&fr_terms, title, version, &doc_urls);
    assert_ne!(fr_txt, txt, "fr index did not thread the French selection");
    assert!(
        !txt.contains("[fallback: en]"),
        "English index must not carry fallback markers"
    );
    assert!(
        fr_txt.contains("[fallback: en]"),
        "fr index must carry English-fallback markers (proves the selector threaded)"
    );
}

/// `doc_card`: resolves a term and renders a `# {curie}` card with the
/// metadata + definition through the shared builder + renderer; an unresolved
/// query yields `None` (the caller supplies the not-found envelope).
fn doc_card_build_renders_card_and_not_found() {
    let (terms, _t, _v) = english_terms();
    let modeled_defs = authenticated_modeled_defs();
    let (title, built) = doc_card_build(terms, "gmeow:EntityExistence", modeled_defs)
        .resolved()
        .expect("known term resolves");
    let card = gmeow_docs_model::card::render_card(
        &title,
        &built,
        gmeow_docs_model::card::CardDetail::Standard,
    );
    assert!(
        card.starts_with("# gmeow:EntityExistence"),
        "card head:\n{card}"
    );
    // Canonical card convention (the shared `gmeow_docs_model::card` renderer):
    // human-cased category, and term→slice provenance recovered from the
    // documentation graph (the docs generator dogfoods `gmeow:docOwnerSlice`
    // into the bundle; the fold reads it back).
    assert!(card.contains("- category: Class"));
    assert!(card.contains("- iri: https://blackcatinformatics.ca/gmeow/EntityExistence"));
    let slice_line = card
        .lines()
        .find(|l| l.starts_with("- slice: "))
        .expect("folded card must carry the owning slice (term→slice provenance)");
    assert!(
        !slice_line.trim_start_matches("- slice: ").trim().is_empty(),
        "slice value must be non-blank, got {slice_line:?}"
    );
    assert!(card.ends_with('\n'));
    // `gmeow:EntityExistence` genuinely has a generated Pydantic model (it
    // names a `$defs` entry), so the MCP card must carry the link.
    assert!(
        card.contains("**Python model:**"),
        "a modeled class must carry the python_model line:\n{card}"
    );

    assert!(
        matches!(
            doc_card_build(terms, "gmeow:NoSuchTerm", modeled_defs),
            ConsumerResolution::NotFound
        ),
        "an unresolved query yields NotFound"
    );
}

/// `class_is_modeled` gate: a Class
/// with NO `$defs` entry (an abstract class with no SHACL NodeShape) must
/// never get a fabricated `python_model` link, even though `term_to_card`'s
/// PRE-fix gate (`category == "class" && !owner_slice.is_empty()`) would have
/// shown one. `gmeow:Proposition` is a real production example.
fn doc_card_build_omits_python_model_for_an_unmodeled_class() {
    let (terms, _t, _v) = english_terms();
    let modeled_defs = authenticated_modeled_defs();
    assert!(
        !modeled_defs.contains("Proposition"),
        "sanity: gmeow:Proposition must genuinely have no $defs entry today"
    );
    let (_, built) = doc_card_build(terms, "gmeow:Proposition", modeled_defs)
        .resolved()
        .expect("gmeow:Proposition resolves");
    assert_eq!(
        built.python_model, None,
        "an unmodeled class must never fabricate a python_model link"
    );
    assert_eq!(built.python_snippet, None);
}

/// `llms_full` / `llms-full.txt`: the standard header then `### ` term blocks
/// inlined in full (no links), emitted in CURIE order and bounded by the fixed
/// token budget, with the elided remainder disclosed (never silently dropped).
fn consumer_llms_full_inlines_terms_within_the_token_budget() {
    let (terms, title, version) = english_terms();
    let full = consumer_llms_full(
        terms,
        title,
        version,
        authenticated_modeled_defs(),
        english_primer(),
    );
    assert!(full.starts_with(&format!(
        "# {title}\n\n> {}\n\n",
        gmeow_docs_model::llms::GMEOW_SUMMARY
    )));
    assert!(full.contains("## Terms\n\n"));
    // No markdown links in the complete form (it is self-contained).
    assert!(
        !full.contains("](terms/"),
        "llms-full must be link-free (inlined content)"
    );
    // Blocks are emitted in a deterministic CURIE order, so the CURIE-first
    // term is always inlined.
    let mut ordered: Vec<&Term> = terms.iter().collect();
    ordered.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    let headings = full.lines().filter(|l| l.starts_with("### ")).count();
    assert!(headings >= 1, "expected at least one inlined term block");
    assert!(
        full.contains(&format!("### {}", ordered[0].curie)),
        "the CURIE-first term ({}) must be inlined",
        ordered[0].curie
    );
    // The full vocabulary far exceeds the token budget, so some terms are
    // elided — and the elision is disclosed, not silent.
    assert!(
        headings < terms.len(),
        "full vocab should exceed the budget"
    );
    assert!(
        full.contains("elided to fit"),
        "the token-budget elision must be disclosed"
    );
    // The emitted document respects the budget (plus at most one overflow block
    // and the trailing disclosure line).
    assert!(
        gmeow_docs_model::llms::estimate_tokens(&full)
            <= gmeow_docs_model::llms::LLMS_FULL_TOKEN_BUDGET * 2,
        "llms-full must stay within a small multiple of the token budget"
    );
}

/// The MCP/consumer `llms_txt` and `llms_full` surfaces must each carry the
/// standing-page `## Reference` expansion (Competency questions, Conformance
/// fixtures, Notation grammars, Glossary, Build pipeline) plus the offline
/// snippet-corpus note — the same expansion the docs-site `llms_txt`/
/// `llms_full_txt` render. This had previously landed ONLY on the docs site;
/// the native `gmeow mcp` binary's `llms_txt`/`llms_full` tool output carried
/// zero occurrences of any of these. Falsifiable per page name so a future
/// dropped page fails loudly instead of a vague substring match.
fn consumer_llms_surfaces_carry_the_standing_reference_pages() {
    let (terms, title, version) = english_terms();
    let graph = authenticated_fold();
    let doc_urls = doc_url_map(&FoldView::new(&graph));
    let primer = english_primer();

    let txt = consumer_llms_txt(terms, title, version, &doc_urls);
    let full = consumer_llms_full(terms, title, version, authenticated_modeled_defs(), primer);

    assert!(
        txt.contains("## Reference\n"),
        "consumer llms_txt must carry a '## Reference' section"
    );
    assert!(
        full.contains("## Reference\n"),
        "consumer llms_full must carry a '## Reference' section"
    );

    for page in gmeow_docs_model::llms::STANDING_REFERENCE_PAGES {
        assert!(
            txt.contains(page),
            "consumer llms_txt must name the standing reference page {page:?}"
        );
        assert!(
            full.contains(page),
            "consumer llms_full must name the standing reference page {page:?}"
        );
    }

    let note = gmeow_docs_model::llms::SNIPPETS_CORPUS_NOTE;
    assert!(
        txt.contains(note),
        "consumer llms_txt must carry the offline snippet-corpus note"
    );
    assert!(
        full.contains(note),
        "consumer llms_full must carry the offline snippet-corpus note"
    );

    // The write_llms_txt (dist/llms.txt tarball) surface shares the same
    // section-append path — it must not silently regress either.
    let dist_txt = String::from_utf8(write_llms_txt(terms, title, version, primer)).unwrap();
    assert!(dist_txt.contains("## Reference\n"));
    for page in gmeow_docs_model::llms::STANDING_REFERENCE_PAGES {
        assert!(
            dist_txt.contains(page),
            "dist llms.txt must name the standing reference page {page:?}"
        );
    }
    assert!(dist_txt.contains(note));
}

/// Both flat llms surfaces (`dist/llms.txt` and the shared `llms-full.txt`) carry the
/// GMN-1 teachability primer — appended through the SAME `gmeow_docs_model::gmn1_primer::section`
/// path so a fresh model reads GMN emission guidance inline, not just the vocabulary. The
/// primer heading and its graph-derived rows (a record sigil, the repair card, an operator
/// glyph) must survive into both surfaces.
fn llms_surfaces_carry_the_gmn1_teachability_primer() {
    let (terms, title, version) = english_terms();
    let primer = english_primer();
    let heading = format!("## {}", gmeow_docs_model::gmn1_primer::PRIMER_HEADING);

    let dist_txt = String::from_utf8(write_llms_txt(terms, title, version, primer)).unwrap();
    let full = consumer_llms_full(terms, title, version, authenticated_modeled_defs(), primer);

    for surface in [&dist_txt, &full] {
        assert!(
            surface.contains(&heading),
            "an llms surface must carry the GMN-1 primer heading {heading:?}"
        );
        // A repair-loop card (the NL→GMN→gmn_validate→@err/@patch workflow's vocabulary).
        assert!(
            surface.contains("gmeow:GmnErr"),
            "the primer must teach the @err repair record"
        );
        // The primer body is the SAME rendered section on both surfaces.
        assert!(
            surface.contains(primer.rendered().trim()),
            "the primer section must appear verbatim in the surface"
        );
    }
}

/// The twin-contract lock (§19 one-path): the MCP card and the
/// docs-site card share ONE renderer (`gmeow_docs_model::card::render_card_body`)
/// AND one convention. This test pins the shared renderer's output for a card
/// whose SHARED fields are set, then proves the folded-`Term` builder
/// (`term_to_card`) maps those same fields into the SAME canonical `Card`,
/// so the two sources can never re-diverge field-for-field.
///
/// (The docs-site builder `gmeow_docs::render::doc_term_card` is private; both
/// builders are thin field-copies into `gmeow_docs_model::card::Card`, so locking
/// `term_to_card` against an explicit `Card` of the same shared values — fed
/// through the SOLE body renderer — is the determinism guard. The docs side's
/// own routing through `render_card_body` is pinned by gmeow-docs' tests.)
fn term_card_shares_one_renderer_and_convention() {
    // A folded Term with every SHARED card field populated.
    let folded = Term {
        category: "property",
        iri: "https://blackcatinformatics.ca/gmeow/hasFoo".to_string(),
        curie: "gmeow:hasFoo".to_string(),
        label: "has foo".to_string(),
        definition: "Relates a thing to its foo.".to_string(),
        prop_kind: "object",
        domain: "Thing".to_string(),
        range: "Foo".to_string(),
        sub_property_of: vec!["gmeow:relates".to_string()],
        alignments: vec!["exactMatch=ex:hasFoo".to_string()],
        box_roles: vec!["gmeow:boxTBox".to_string()],
        scope_notes: vec!["A scope note.".to_string()],
        examples: vec!["An example.".to_string()],
        use_when: vec!["When there is a foo.".to_string()],
        avoid_when: vec!["When there is no foo.".to_string()],
        how_to_use: vec!["Use idiomatically.".to_string()],
        use_for_consumer: vec!["gmeow:profileMemory".to_string()],
        avoid_for_consumer: vec!["gmeow:profileNarrative".to_string()],
        logic_stereotypes: vec!["logic:Relator".to_string()],
        related_terms: vec!["gmeow:Bar".to_string()],
        owner_slice: "https://blackcatinformatics.ca/gmeow/slice/zoo".to_string(),
        ..Term::default()
    };

    // The canonical Card the docs side would build for the SAME shared values
    // (a property → parents come from sub_property_of; the slice is the LOCAL
    // NAME of the owning slice IRI, recovered identically on both sides).
    let expected = gmeow_docs_model::card::Card {
        category: "Property".to_string(),
        iri: "https://blackcatinformatics.ca/gmeow/hasFoo".to_string(),
        label: Some("has foo".to_string()),
        slice: Some("zoo".to_string()),
        box_roles: vec!["gmeow:boxTBox".to_string()],
        definition: Some("Relates a thing to its foo.".to_string()),
        parents: vec!["gmeow:relates".to_string()],
        domain: vec!["Thing".to_string()],
        range: vec!["Foo".to_string()],
        use_when: vec!["When there is a foo.".to_string()],
        avoid_when: vec!["When there is no foo.".to_string()],
        how_to_use: vec!["Use idiomatically.".to_string()],
        scope_notes: vec!["A scope note.".to_string()],
        examples: vec!["An example.".to_string()],
        logic_stereotypes: vec!["logic:Relator".to_string()],
        related_terms: vec!["gmeow:Bar".to_string()],
        use_for_consumer: vec!["gmeow:profileMemory".to_string()],
        avoid_for_consumer: vec!["gmeow:profileNarrative".to_string()],
        aligns: vec!["exactMatch=ex:hasFoo".to_string()],
        ..gmeow_docs_model::card::Card::default()
    };

    // The folded builder must produce exactly that Card (field-for-field).
    assert_eq!(
        term_to_card(&folded, &BTreeSet::new()),
        expected,
        "term_to_card must map the folded Term into the canonical shared Card"
    );

    // …and both render IDENTICALLY through the SOLE body renderer.
    let from_folded = gmeow_docs_model::card::render_card_body(
        &term_to_card(&folded, &BTreeSet::new()),
        gmeow_docs_model::card::CardDetail::Standard,
    );
    let from_expected = gmeow_docs_model::card::render_card_body(
        &expected,
        gmeow_docs_model::card::CardDetail::Standard,
    );
    assert_eq!(
        from_folded, from_expected,
        "shared renderer must agree byte-for-byte"
    );

    // Canonical convention: bold labels, `; ` delimiters, no per-item backticks.
    assert!(from_folded.contains("**Use when:** When there is a foo.\n\n"));
    assert!(from_folded.contains("**Aligns:** exactMatch=ex:hasFoo\n\n"));
    assert!(!from_folded.contains('`'), "card body carries no backticks");
    assert!(
        !from_folded.contains("\n*Use when:* "),
        "labels are bold, not italic"
    );
}

/// `term_to_card` slice handling: a recovered `owner_slice` IRI renders as its
/// local name; an absent one yields `None` (no blank `slice:` line). Locks
/// both arms of the term→slice provenance recovery.
fn term_to_card_slice_uses_local_name_or_omits() {
    let with_slice = Term {
        category: "class",
        iri: "https://blackcatinformatics.ca/gmeow/Cat".to_string(),
        curie: "gmeow:Cat".to_string(),
        owner_slice: "https://blackcatinformatics.ca/gmeow/slice/zoo".to_string(),
        ..Term::default()
    };
    assert_eq!(
        term_to_card(&with_slice, &BTreeSet::new()).slice,
        Some("zoo".to_string())
    );
    assert!(
        gmeow_docs_model::card::render_card_body(
            &term_to_card(&with_slice, &BTreeSet::new()),
            gmeow_docs_model::card::CardDetail::Standard,
        )
        .contains("- slice: zoo\n")
    );

    let no_slice = Term {
        category: "class",
        iri: "https://blackcatinformatics.ca/gmeow/Dog".to_string(),
        curie: "gmeow:Dog".to_string(),
        ..Term::default()
    };
    assert_eq!(term_to_card(&no_slice, &BTreeSet::new()).slice, None);
    assert!(
        !gmeow_docs_model::card::render_card_body(
            &term_to_card(&no_slice, &BTreeSet::new()),
            gmeow_docs_model::card::CardDetail::Standard,
        )
        .contains("- slice:")
    );
}

/// `class_is_modeled` gate: a class whose IRI names a `$defs` entry gets the
/// `python_model` link; an otherwise-identical class that does not is honestly
/// omitted — never a fabricated ImportError-inducing link.
fn term_to_card_gates_python_model_on_schema_defs_membership() {
    let modeled = Term {
        category: "class",
        iri: "https://blackcatinformatics.ca/gmeow/Cat".to_string(),
        curie: "gmeow:Cat".to_string(),
        owner_slice: "https://blackcatinformatics.ca/gmeow/slice/zoo".to_string(),
        ..Term::default()
    };
    let mut defs = BTreeSet::new();
    defs.insert("Cat".to_string());
    let card = term_to_card(&modeled, &defs);
    assert_eq!(
        card.python_model,
        Some(gmeow_docs_model::card::python_model_path(
            &modeled.owner_slice,
            &modeled.iri
        ))
    );
    assert!(card.python_snippet.is_some());

    // Same shape, but `Cat` is absent from the `$defs` set: no link.
    let unmodeled = Term {
        iri: "https://blackcatinformatics.ca/gmeow/Ferret".to_string(),
        curie: "gmeow:Ferret".to_string(),
        ..modeled.clone()
    };
    let card = term_to_card(&unmodeled, &defs);
    assert_eq!(card.python_model, None);
    assert_eq!(card.python_snippet, None);
}

/// `okf_index`: the manifest envelope wraps `ok`/`format`/`lossy`/`count`
/// around per-document `{path, type, title, resource}` records.
fn okf_index_envelope_shape() {
    let (terms, _t, _v) = english_terms();
    let env: serde_json::Value = serde_json::from_str(&okf_index_envelope(terms)).unwrap();
    assert_eq!(env["ok"], serde_json::json!(true));
    assert_eq!(env["format"], serde_json::json!("okf"));
    assert_eq!(env["lossy"], serde_json::json!(true));
    assert_eq!(env["count"].as_u64().unwrap() as usize, terms.len());

    let docs = env["documents"].as_array().unwrap();
    assert_eq!(docs.len(), terms.len());
    // A known class document path/type/resource.
    let entity_existence = terms
        .iter()
        .find(|t| t.curie == "gmeow:EntityExistence")
        .expect("term present");
    let doc = docs
        .iter()
        .find(|d| d["resource"] == serde_json::json!(entity_existence.iri))
        .expect("okf doc present");
    assert_eq!(
        doc["path"],
        serde_json::json!("gmeow-okf/classes/EntityExistence.md")
    );
    assert_eq!(doc["type"], serde_json::json!("Class"));
    assert_eq!(doc["title"], serde_json::json!("Entity Existence"));
}

/// One process owns every real-bundle export assertion, so the authenticated fold,
/// term corpus, schema definition set, and producer-selected GMN primer are each restored once.
/// Nextest otherwise invokes each unit test in a fresh process and defeats all four
/// process-local caches above.
#[test]
fn export_contracts_share_one_authenticated_fold() {
    let cases: &[(&str, fn())] = &[
        (
            "json_str_ascii_matches_cpython_short_escapes",
            json_str_ascii_matches_cpython_short_escapes,
        ),
        (
            "selector_threads_requested_language",
            selector_threads_requested_language,
        ),
        (
            "resolve_term_iri_spans_grounding_namespaces",
            resolve_term_iri_spans_grounding_namespaces,
        ),
        (
            "lookup_envelope_matches_consumer_contract",
            lookup_envelope_matches_consumer_contract,
        ),
        (
            "consumer_llms_txt_uses_standard_format",
            consumer_llms_txt_uses_standard_format,
        ),
        (
            "doc_card_build_renders_card_and_not_found",
            doc_card_build_renders_card_and_not_found,
        ),
        (
            "doc_card_build_omits_python_model_for_an_unmodeled_class",
            doc_card_build_omits_python_model_for_an_unmodeled_class,
        ),
        (
            "consumer_llms_full_inlines_terms_within_the_token_budget",
            consumer_llms_full_inlines_terms_within_the_token_budget,
        ),
        (
            "consumer_llms_surfaces_carry_the_standing_reference_pages",
            consumer_llms_surfaces_carry_the_standing_reference_pages,
        ),
        (
            "llms_surfaces_carry_the_gmn1_teachability_primer",
            llms_surfaces_carry_the_gmn1_teachability_primer,
        ),
        (
            "term_card_shares_one_renderer_and_convention",
            term_card_shares_one_renderer_and_convention,
        ),
        (
            "term_to_card_slice_uses_local_name_or_omits",
            term_to_card_slice_uses_local_name_or_omits,
        ),
        (
            "term_to_card_gates_python_model_on_schema_defs_membership",
            term_to_card_gates_python_model_on_schema_defs_membership,
        ),
        ("okf_index_envelope_shape", okf_index_envelope_shape),
    ];
    let mut failures = Vec::new();
    for (name, case) in cases {
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(*case)) {
            let detail = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_string())
                })
                .unwrap_or_else(|| "non-string panic".to_string());
            failures.push(format!("{name}: {detail}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} bundled export contract(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
