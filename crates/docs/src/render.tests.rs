// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::DocTermStability;

/// `CONTROLLER_HOOKS` equals the hooks the SHIPPED controller module actually binds.
///
/// Scrapes `assets/gmeow-docs.js` for every element it looks up
/// (`getElementById` / `querySelectorAll`) AND registers a listener on, then asserts
/// set-EQUALITY in both directions — so a looked-up-but-never-bound element and a
/// bound-but-never-declared hook are both failures. Without it the const is prose.
#[test]
fn controller_hooks_match_the_shipped_controller_selectors() {
    /// The identifier a `document.…` lookup is bound to, given the text preceding
    /// the lookup: `const form = ` (assignment) or `for (const btn of ` (iteration).
    fn binding_target(head: &str) -> Option<&str> {
        let head = head.trim_end();
        let head = head
            .strip_suffix('=')
            .or_else(|| head.strip_suffix(" of"))?
            .trim_end();
        let start = head
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .map_or(0, |i| i + 1);
        let ident = &head[start..];
        (!ident.is_empty() && ident != "const" && ident != "let" && ident != "var").then_some(ident)
    }

    /// The quoted argument that a `…("` prefix opens, if the line carries one.
    fn quoted_after<'a>(line: &'a str, opener: &str) -> Option<(usize, &'a str)> {
        let pos = line.find(opener)?;
        let rest = &line[pos + opener.len()..];
        let end = rest.find('"')?;
        Some((pos, &rest[..end]))
    }

    let mut looked_up: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
    for line in DOCS_JS.lines() {
        if let Some((pos, id)) = quoted_after(line, "document.getElementById(\"")
            && let Some(var) = binding_target(&line[..pos])
        {
            looked_up.insert(var, format!("id=\"{id}\""));
        }
        if let Some((pos, class)) = quoted_after(line, "document.querySelectorAll(\".")
            && let Some(var) = binding_target(&line[..pos])
        {
            looked_up.insert(var, class.to_string());
        }
    }

    let mut scraped: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in DOCS_JS.lines() {
        let Some(pos) = line.find(".addEventListener(") else {
            continue;
        };
        let head = &line[..pos];
        let start = head
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .map_or(0, |i| i + 1);
        if let Some(hook) = looked_up.get(&head[start..]) {
            scraped.insert(hook.clone());
        }
    }

    // A scraper that matched nothing would make the equality below vacuous the day
    // someone empties the const, so the floor is asserted explicitly.
    assert!(
        scraped.len() >= 5,
        "the controller-selector scrape found almost nothing ({scraped:?}) — the \
             module's binding shape changed and this gate stopped seeing it"
    );
    let declared: std::collections::BTreeSet<String> =
        CONTROLLER_HOOKS.iter().map(|h| (*h).to_string()).collect();
    assert_eq!(
        scraped, declared,
        "CONTROLLER_HOOKS must equal the hooks assets/gmeow-docs.js binds to"
    );
}

#[test]
fn rel_computes_relative_dir_paths() {
    assert_eq!(rel("", "slices"), "slices/");
    assert_eq!(rel("slices", ""), "../");
    assert_eq!(rel("terms/cat", "slices/zoo"), "../../slices/zoo/");
    assert_eq!(rel("terms/cat", "terms/cat"), "");
    assert_eq!(rel("classes", "terms/cat"), "../terms/cat/");
}

#[test]
fn root_href_counts_depth() {
    assert_eq!(root_href(""), "");
    assert_eq!(root_href("slices"), "../");
    assert_eq!(root_href("terms/cat"), "../../");
}

fn term(iri: &str, category: DocTermCategory) -> DocTerm {
    DocTerm {
        iri: iri.to_string(),
        category,
        ..Default::default()
    }
}

#[test]
fn stage_page_self_explains_its_attached_graphs_and_blob_reps() {
    // docs-on <stage> surfaces the stage's declared carrier contribution: the
    // attached graph/documentation + the attached blob-rep lanes (Step 4 self-explain).
    use crate::model::{DocPipeline, DocStage};
    let stage_iri = "https://blackcatinformatics.ca/gmeow/stage-docs-render";
    let model = DocsModel {
        pipeline: Some(DocPipeline {
            stages: vec![DocStage {
                iri: stage_iri.to_string(),
                attaches_graphs: vec![
                    "https://blackcatinformatics.ca/gmeow/graph/documentation".to_string(),
                ],
                attaches_blob_reps: vec!["diagnostics:nodes".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let doc_term = term(stage_iri, DocTermCategory::Individual);
    let mut out = String::new();
    append_stage_section(&mut out, &model, &doc_term, "terms/x");
    assert!(
        out.contains(model.ui("body_pipeline_attaches")),
        "the stage page must carry an Attaches heading: {out}"
    );
    assert!(
        out.contains("https://blackcatinformatics.ca/gmeow/graph/documentation"),
        "the stage page must surface the attached graph/documentation: {out}"
    );
    assert!(
        out.contains("diagnostics:nodes"),
        "the stage page must surface the attached blob-rep lane: {out}"
    );
}

#[test]
fn resolve_term_slugs_disambiguates_only_colliders_injectively() {
    let base = "https://blackcatinformatics.ca/gmeow/";
    let terms = vec![
        // A unique base — absent from the map, keeps its base slug.
        term(&format!("{base}Solo"), DocTermCategory::Class),
        // A class/property case-collision on `acceptancestatus`.
        term(&format!("{base}AcceptanceStatus"), DocTermCategory::Class),
        term(
            &format!("{base}acceptanceStatus"),
            DocTermCategory::Property,
        ),
        // A base+category collision (two Individuals slugging to `foo`).
        term(&format!("{base}Foo"), DocTermCategory::Individual),
        term(&format!("{base}foo"), DocTermCategory::Individual),
    ];

    let map = resolve_term_slugs(&terms);

    // The unique-base term is NOT in the map (falls back to base).
    assert!(!map.contains_key(&format!("{base}Solo")));
    // Case-collision resolved by category.
    assert_eq!(
        map[&format!("{base}AcceptanceStatus")],
        "acceptancestatus-class"
    );
    assert_eq!(
        map[&format!("{base}acceptanceStatus")],
        "acceptancestatus-property"
    );
    // Base+category collision: the IRI-lexically-first keeps `foo-individual`,
    // the other gets the digest tiebreak.
    assert_eq!(map[&format!("{base}Foo")], "foo-individual");
    let other = &map[&format!("{base}foo")];
    assert!(
        other.starts_with("foo-individual-") && other.len() > "foo-individual-".len(),
        "digest-disambiguated slug expected, got {other}"
    );

    // Deterministic: identical input → identical map.
    assert_eq!(resolve_term_slugs(&terms), map);

    // Injective over the whole surface: term_slug is distinct for every term.
    let mut resolved = terms.clone();
    for t in &mut resolved {
        if let Some(s) = map.get(&t.iri) {
            t.slug = s.clone();
        }
    }
    let slugs: std::collections::BTreeSet<String> = resolved.iter().map(term_slug).collect();
    assert_eq!(slugs.len(), resolved.len(), "slugs must be injective");
}

#[test]
fn slugify_is_filesystem_safe() {
    assert_eq!(slugify("HasOwner"), "hasowner");
    assert_eq!(slugify("Cat 9 Lives!"), "cat-9-lives");
    assert_eq!(slugify("--weird--"), "weird");
    assert_eq!(slugify(""), "unnamed");
}

#[test]
fn md_escape_neutralizes_table_and_inline_metachars() {
    assert_eq!(md_escape("a|b"), "a\\|b");
    assert_eq!(md_escape("<x>"), "\\<x\\>");
    assert_eq!(md_escape("line\nbreak"), "line break");
}

/// Decode a Python double-quoted literal body the way `json.loads` would
/// see it after Python's own string-literal decoding — i.e. undo exactly
/// the two escapes [`python_str_escape`] introduces (`\\` and `\"`, plus
/// the `\n`/`\r` it uses to stand in for a literal newline/CR that a
/// non-raw literal cannot otherwise carry) — so the test can assert the
/// round trip without invoking a Python interpreter.
fn decode_python_double_quoted(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len());
    let mut chars = escaped.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                other => panic!("unexpected escape \\{other:?} in {escaped:?}"),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn python_str_escape_round_trips_triple_quote_and_metachars() {
    // A payload containing `'''` would prematurely terminate the OLD
    // `r'''{payload}'''` raw literal; a payload containing `\` or `"`
    // exercises the two escapes a non-raw double-quoted literal needs.
    // A literal newline is what a pretty-printed JSON-LD payload actually
    // contains, and is exactly what a non-raw, non-triple-quoted literal
    // cannot carry unescaped.
    let payload = "{\"a\": \"it's '''not''' a \\\"quote\\\"\\\\end\",\n  \"b\": 1}";
    let literal = python_str_escape(payload);

    // No unescaped `"` may appear in the body (each `"` must be preceded
    // by exactly one `\`), and no raw newline may appear at all — both
    // would break a non-raw double-quoted Python literal.
    assert!(
        !literal.contains('\n'),
        "escaped body must not carry a literal newline: {literal:?}"
    );
    let mut prev = '\0';
    for ch in literal.chars() {
        if ch == '"' {
            assert_eq!(prev, '\\', "unescaped quote in {literal:?}");
        }
        prev = ch;
    }

    // Decoding what Python would decode reproduces the original payload
    // exactly, so `json.loads("{literal}")` gets back the exact JSON text.
    assert_eq!(decode_python_double_quoted(&literal), payload);
}

#[test]
fn rust_raw_fence_width_widens_past_embedded_hash_quote_runs() {
    // No `"#`-like run at all: the familiar single-`#` fence suffices.
    assert_eq!(rust_raw_fence_width("plain turtle, no quotes"), 1);
    assert_eq!(rust_raw_fence_width(r#"a "quoted" string"#), 1);

    // A literal string value immediately followed by a `#`-fragment IRI
    // puts a `"#` run in the content — this demands a wider fence than a
    // naive single-`#` fence (`r#"..."#`), the same twin hazard that
    // `python_syntax_tab`'s raw-triple-quote interpolation must widen for.
    let turtle = "ex:x ex:note \"ends right here\"##weird .";
    let width = rust_raw_fence_width(turtle);
    assert_eq!(
        width, 3,
        "content has a 2-`#` run after `\"`, needs a 3-`#` fence"
    );

    let literal = rust_raw_string_literal(turtle);
    let fence = "#".repeat(width);
    assert_eq!(literal, format!("r{fence}\"{turtle}\"{fence}"));
    // The fence must not appear as a `"`-followed-by-fence-or-more run
    // anywhere inside the raw content, or the literal would close early.
    let close = format!("\"{}", "#".repeat(width));
    assert!(
        !turtle.contains(&close),
        "chosen fence {width} still matches inside content: {turtle:?}"
    );
}

#[test]
fn align_tag_handles_trailing_separators() {
    assert_eq!(
        align_tag("http://www.w3.org/2004/02/skos/core#closeMatch"),
        "closeMatch"
    );
    assert_eq!(
        align_tag("http://www.w3.org/2002/07/owl#equivalentClass"),
        "equivalentClass"
    );
    // trailing separator must not yield an empty tag
    assert_eq!(align_tag("http://example.org/vocab#"), "vocab");
    assert_eq!(align_tag("http://example.org/vocab/"), "vocab");
    // no separator at all -> whole predicate
    assert_eq!(align_tag("bareword"), "bareword");
}

/// A minimal two-term model with one French translation, used to assert the
/// language-parametrized renderer picks the translation and falls back to
/// English elsewhere.
fn tiny_model() -> DocsModel {
    let foo = DocTerm {
        iri: format!("{GMEOW_NS}Foo"),
        curie: "gmeow:Foo".to_string(),
        label: Some("Foo".to_string()),
        definition: Some("A foo.".to_string()),
        category: DocTermCategory::Class,
        owner_slice: format!("{GMEOW_NS}slices/demo"),
        parents: Vec::new(),
        domain: Vec::new(),
        range: Vec::new(),
        scope_notes: Vec::new(),
        examples: Vec::new(),
        use_when: Vec::new(),
        avoid_when: Vec::new(),
        how_to_use: Vec::new(),
        use_for_consumer: Vec::new(),
        avoid_for_consumer: Vec::new(),
        ..Default::default()
    };
    let bar = DocTerm {
        iri: format!("{GMEOW_NS}Bar"),
        curie: "gmeow:Bar".to_string(),
        label: Some("Bar".to_string()),
        definition: Some("A bar.".to_string()),
        category: DocTermCategory::Class,
        owner_slice: format!("{GMEOW_NS}slices/demo"),
        parents: Vec::new(),
        domain: Vec::new(),
        range: Vec::new(),
        scope_notes: Vec::new(),
        examples: Vec::new(),
        use_when: Vec::new(),
        avoid_when: Vec::new(),
        how_to_use: Vec::new(),
        use_for_consumer: Vec::new(),
        avoid_for_consumer: Vec::new(),
        ..Default::default()
    };

    let translations = crate::i18n::Translations::from_entries(
        [
            (
                (
                    format!("{GMEOW_NS}Foo"),
                    RDFS_LABEL.to_string(),
                    "fr".to_string(),
                ),
                "Fou".to_string(),
            ),
            (
                (
                    format!("{GMEOW_NS}Foo"),
                    SKOS_DEFINITION.to_string(),
                    "fr".to_string(),
                ),
                "Un fou.".to_string(),
            ),
        ],
        ["fr".to_string()],
    );

    DocsModel {
        title: "Demo".to_string(),
        version: "test".to_string(),
        slices: Vec::new(),
        terms: vec![bar, foo],
        dependency_edges: Vec::new(),
        mapping_sets: Vec::new(),
        linkages: Vec::new(),
        examples: Vec::new(),
        fixtures: Vec::new(),
        shapes: Vec::new(),
        competencies: Vec::new(),
        grammars: Vec::new(),
        loss_targets: Vec::new(),
        worked_instances: Vec::new(),
        concerns: Vec::new(),
        external_terms: Vec::new(),
        seams: Vec::new(),
        recipes: Vec::new(),
        learning_paths: Vec::new(),
        constraint_rules: Vec::new(),
        advice_entries: Vec::new(),
        four_boxes: None,
        concept_doi: None,
        pipeline: None,
        available_languages: vec!["english".to_string(), "fr".to_string()],
        translations,
        ui_catalog: crate::i18n::UiCatalog::default(),
        reasoning: None,
        diagnostics: None,
        term_loss: None,
        schema_fragments: None,
        lang: String::new(),
    }
}

// ── seam-registry render ↔ drift-gate parser contract ────────────────────
//
// `gmeow_validate::authoring_integrity::detect_seam_registry_drift` reads this
// page BACK, per seam, to prove it never drifts from the authored `gmeow:Seam`
// registry. That gate parses the exact table shape `md_seam_registry` writes —
// bolded name cell, `;`-joined `from → to` legs whose slice identity is the slug
// in the rendered link's href, backticked (and possibly linked) carrying-term
// CURIEs, backticked owning-doc filenames. These tests pin that contract from
// the RENDERER's side, so a change to this function's markup that the gate
// cannot read fails here rather than turning the gate into a false positive (or,
// worse, a false negative) in production.

/// A two-slice, two-seam model whose seam registry exercises both rendered
/// forms: a resolvable slice (link) and a term with its own term page (link).
fn seam_model() -> DocsModel {
    use crate::model::{DocSeam, DocSeamDirection, DocSlice};
    fn slice(local: &str, title: &str) -> DocSlice {
        DocSlice {
            iri: format!("{GMEOW_NS}slices/{local}"),
            label: Some(local.to_string()),
            title: Some(title.to_string()),
            tier: None,
            identifier: None,
            creators: Vec::new(),
            consumers: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
            artifacts: Vec::new(),
            documents: Vec::new(),
            has_thesis_sentence: false,
            realized_state_complete: false,
        }
    }
    let mut model = tiny_model();
    model.slices = vec![
        slice("lang", "Language grounding"),
        slice("logic", "Logic grounding"),
        slice("math", "Mathematics grounding"),
    ];
    model.seams = vec![
        DocSeam {
            iri: format!("{GMEOW_NS}seam/compilation"),
            label: Some("Compilation seam".to_string()),
            definition: Some("The math → logic lowering seam.".to_string()),
            directions: vec![DocSeamDirection {
                from: format!("{GMEOW_NS}slices/math"),
                to: format!("{GMEOW_NS}slices/logic"),
            }],
            carrying_terms: vec![
                "https://blackcatinformatics.ca/math/compilesToLogicTerm".to_string(),
            ],
            owning_docs: vec!["MATHEMATICS-EXPRESSIONS.md".to_string()],
        },
        DocSeam {
            iri: format!("{GMEOW_NS}seam/denotation"),
            label: Some("Denotation seam".to_string()),
            definition: Some("The lang → logic meaning seam.".to_string()),
            directions: vec![
                DocSeamDirection {
                    from: format!("{GMEOW_NS}slices/lang"),
                    to: format!("{GMEOW_NS}slices/logic"),
                },
                DocSeamDirection {
                    from: format!("{GMEOW_NS}slices/math"),
                    to: format!("{GMEOW_NS}slices/logic"),
                },
            ],
            carrying_terms: vec![
                "https://blackcatinformatics.ca/lang/denotationKind".to_string(),
                "https://blackcatinformatics.ca/lang/denotationTarget".to_string(),
            ],
            owning_docs: vec!["LANG-MEANING.md".to_string()],
        },
    ];
    model
}

/// The `gmeow_validate` seam records the [`seam_model`] seams project to — the
/// authored side of the comparison, built by hand so this test needs no
/// repository (and no `generated/`) at all.
fn seam_model_records() -> Vec<gmeow_validate::slice_peerage::SeamRecord> {
    use gmeow_validate::slice_peerage::SeamRecord;
    seam_model()
        .seams
        .iter()
        .map(|seam| SeamRecord {
            iri: seam.iri.clone(),
            name: seam.label.clone().expect("the fixture labels every seam"),
            labels: vec![(
                seam.label.clone().expect("the fixture labels every seam"),
                Some("x-gmeow-english".to_string()),
            )],
            carrying_terms: seam.carrying_terms.iter().map(|t| to_curie(t)).collect(),
            carrying_term_iris: seam.carrying_terms.iter().cloned().collect(),
            directions: seam
                .directions
                .iter()
                .map(|d| (d.from.clone(), d.to.clone()))
                .collect(),
            owning_docs: seam.owning_docs.iter().cloned().collect(),
        })
        .collect()
}

#[test]
fn seam_registry_render_is_readable_by_the_drift_gate() {
    let page = md_seam_registry(&seam_model());
    // Non-vacuity: the render really produced the table and both seams.
    assert!(
        page.contains("| Seam | Direction | Carrying terms | Owning doc |"),
        "{page}"
    );
    assert!(page.contains("**Denotation seam**"), "{page}");
    assert!(page.contains("**Compilation seam**"), "{page}");
    let findings = gmeow_validate::authoring_integrity::detect_seam_registry_drift(
        &seam_model_records(),
        &page,
    );
    assert!(
        findings.is_empty(),
        "the drift gate must read this render back with zero drift; markup and \
             parser have diverged:\n{}\n--- page ---\n{page}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn seam_registry_render_parity_fails_when_a_seam_loses_a_direction_leg() {
    // Teeth for the parity test above: the gate is genuinely reading the
    // rendered legs, not passing whatever it is handed.
    let mut model = seam_model();
    model.seams[1].directions.remove(1);
    let findings = gmeow_validate::authoring_integrity::detect_seam_registry_drift(
        &seam_model_records(),
        &md_seam_registry(&model),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("Denotation seam") && f.message.contains("math → logic")),
        "a leg dropped from the render must be caught: {findings:?}"
    );
}

/// NON-VACUITY, over the REAL repository: the seams actually authored in the
/// grounding manifests render into a page the drift gate reads back with zero
/// drift. The two tests above prove the markup/parser contract on a hand-built
/// two-seam fixture; this one proves it over the live registry, so a newly
/// registered seam whose carrying terms or direction legs the renderer cannot
/// project (or the gate cannot parse) fails HERE, at authoring time, rather than
/// in a `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` run nobody has done yet. It needs the
/// `slices/` tree but no `generated/` tree.
#[test]
fn the_real_seam_registry_renders_into_a_drift_free_page() {
    use crate::model::{DocSeam, DocSeamDirection};

    let slices_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices")
        .canonicalize()
        .expect("the real slices/ tree");
    let records = gmeow_validate::authoring_integrity::seam_registry_of_slices(&slices_dir)
        .expect("read the authored gmeow:Seam registry");
    assert!(
        records.len() >= 6,
        "the authored registry must be non-vacuous; got {}",
        records.len()
    );

    let mut model = seam_model();
    model.seams = records
        .iter()
        .map(|r| {
            let mut directions: Vec<DocSeamDirection> = r
                .directions
                .iter()
                .map(|(from, to)| DocSeamDirection {
                    from: from.clone(),
                    to: to.clone(),
                })
                .collect();
            directions.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
            directions.dedup();
            DocSeam {
                iri: r.iri.clone(),
                label: Some(r.name.clone()),
                definition: None,
                directions,
                carrying_terms: r.carrying_term_iris.iter().cloned().collect(),
                owning_docs: r.owning_docs.iter().cloned().collect(),
            }
        })
        .collect();

    let page = md_seam_registry(&model);
    assert!(
        page.contains("| Seam | Direction | Carrying terms | Owning doc |"),
        "{page}"
    );
    let findings = gmeow_validate::authoring_integrity::detect_seam_registry_drift(&records, &page);
    assert!(
        findings.is_empty(),
        "the authored seam registry must render into a page the drift gate reads back \
             cleanly:\n{}\n--- page ---\n{page}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// The "What GMEOW enforces" page renders the advice
/// recommendation tier as a DISTINCT section from the compliance rules, headed by
/// the single `#advice-` anchor and carrying each realized term's verbatim
/// avoid/use/how-to prose. Self-contained (a hand-built model), so it proves the
/// production render function independently of a regenerated catalog `.nq`.
#[test]
fn constraint_catalog_renders_distinct_advice_section() {
    use crate::model::{AdviceEntry, ConstraintRule};
    let mut model = tiny_model();
    // The advice family rule — its slug is the `#advice-` section anchor.
    model.constraint_rules = vec![ConstraintRule {
        code: gmeow_validate::codes::ADVICE_FAMILY.to_string(),
        slug: gmeow_validate::rule_catalog::slugify(gmeow_validate::codes::ADVICE_FAMILY),
        category: "https://blackcatinformatics.ca/logic/FindingPolicyWarning".to_string(),
        severity: "advisory".to_string(),
        help_uri: gmeow_validate::rule_catalog::help_uri_for(gmeow_validate::codes::ADVICE_FAMILY),
        label: None,
        definition: None,
        applies_to_terms: Vec::new(),
        formalizes: None,
    }];
    model.advice_entries = vec![
        AdviceEntry {
            term: format!("{GMEOW_NS}Entity"),
            slug: "advice-Entity".to_string(),
            label: Some("Entity".to_string()),
            definition: Some("The universal endurant".to_string()),
            avoid_when: vec!["Avoid bare Entity when a sortal applies".to_string()],
            use_when: vec!["Use for category-neutral resources".to_string()],
            how_to_use: vec!["Reserve the unqualified type".to_string()],
            documented_by_rule: Some(format!("{GMEOW_NS}rule/family/advice")),
        },
        AdviceEntry {
            term: format!("{GMEOW_NS}Event"),
            slug: "advice-Event".to_string(),
            label: Some("Event".to_string()),
            definition: None,
            avoid_when: vec!["Avoid typing an endurant as an Event".to_string()],
            use_when: vec!["Use for occurrences with participants".to_string()],
            how_to_use: Vec::new(),
            documented_by_rule: Some(format!("{GMEOW_NS}rule/family/advice")),
        },
    ];

    let md = md_constraint_catalog(&model);

    // A distinct Advice section heading and the single `#advice-` anchor (once).
    assert!(
        md.contains("## Usage Advice"),
        "missing the distinct Advice section heading:\n{md}"
    );
    assert_eq!(
        md.matches("id=\"advice-\"").count(),
        1,
        "the #advice- section anchor must appear exactly once:\n{md}"
    );
    // Both realized terms' per-term sub-anchors and all three deontic legs.
    assert!(md.contains("id=\"advice-Entity\""));
    assert!(md.contains("id=\"advice-Event\""));
    assert!(md.contains("**Avoid when:**"));
    assert!(md.contains("**Use when:**"));
    assert!(md.contains("**How to use:**"));
    // Verbatim prose (period-free substrings; md_escape escapes the trailing dot).
    // (md_escape backslash-escapes `-`/`.`, so assert on separator-free spans.)
    assert!(md.contains("Avoid bare Entity when a sortal applies"));
    assert!(md.contains("neutral resources"));
    assert!(md.contains("Reserve the unqualified type"));
    assert!(md.contains("Avoid typing an endurant as an Event"));
}

#[test]
fn render_site_lang_uses_translation_with_english_fallback() {
    let model = tiny_model();

    // English page for Foo keeps the carrier values.
    let en = render_site_lang(&model, "english");
    let foo_en = String::from_utf8(en.files["terms/foo/index.md"].clone()).unwrap();
    assert!(foo_en.contains("Foo"), "english label present");
    // Definitions are md-escaped (`.` → `\.`), so match a bare substring.
    assert!(foo_en.contains("A foo"), "english definition present");
    assert!(!foo_en.contains("Fou"));

    // French page for Foo uses the translation.
    let fr = render_site_lang(&model, "fr");
    let foo_fr = String::from_utf8(fr.files["terms/foo/index.md"].clone()).unwrap();
    assert!(foo_fr.contains("Fou"), "french label used");
    assert!(foo_fr.contains("Un fou"), "french definition used");

    // Bar has no translation → English fallback even in the fr tree.
    let bar_fr = String::from_utf8(fr.files["terms/bar/index.md"].clone()).unwrap();
    assert!(bar_fr.contains("Bar"));
    assert!(bar_fr.contains("A bar"));

    // The page graph (file set) is identical across languages.
    let en_keys: Vec<&String> = en.files.keys().collect();
    let fr_keys: Vec<&String> = fr.files.keys().collect();
    assert_eq!(en_keys, fr_keys, "no dangling/extra links per language");
}

#[test]
fn per_term_card_json_and_full_md_are_emitted() {
    use crate::model::{
        DiagnosticsDigest, DocDiagFinding, DocFixture, DocFixtureKind, TermLossDigest, TermLossRow,
    };

    let mut model = tiny_model();
    let foo_iri = format!("{GMEOW_NS}Foo");

    // A Do (well-formed) and a Don't (counter-example) fixture referencing Foo.
    model.fixtures.push(DocFixture {
        slice: format!("{GMEOW_NS}slices/demo"),
        logical_path: "tests/conformance-fixtures/foo-ok.ttl".to_string(),
        title: "Well-formed Foo".to_string(),
        text: "ex:a a gmeow:Foo .".to_string(),
        kind: DocFixtureKind::Wellformed,
        terms_referenced: vec!["gmeow:Foo".to_string()],
        expected_outcome: Some("conforms".to_string()),
        violation_code: None,
        rationale: None,
        catalog_slug: None,
    });
    model.fixtures.push(DocFixture {
        slice: format!("{GMEOW_NS}slices/demo"),
        logical_path: "tests/counter-examples/foo-bad.ttl".to_string(),
        title: "Foo missing something".to_string(),
        text: "ex:b a gmeow:Foo .".to_string(),
        kind: DocFixtureKind::CounterExample,
        terms_referenced: vec!["gmeow:Foo".to_string()],
        expected_outcome: Some("violates".to_string()),
        violation_code: Some("shacl.MinCountConstraintComponent".to_string()),
        rationale: None,
        catalog_slug: None,
    });

    // A per-term diagnostic and a per-term projection-loss row for Foo.
    let mut diag_by_term = BTreeMap::new();
    diag_by_term.insert(
        foo_iri.clone(),
        vec![DocDiagFinding {
            code: "gmeow-range-missing".to_string(),
            severity: "error".to_string(),
            category: "structural".to_string(),
            message: "Foo is missing a range".to_string(),
            slice_iri: None,
            help_uri: None,
        }],
    );
    model.diagnostics = Some(DiagnosticsDigest {
        by_term: diag_by_term,
        by_slice: BTreeMap::new(),
        total: 1,
    });
    let mut loss_by_term = BTreeMap::new();
    loss_by_term.insert(
        foo_iri.clone(),
        vec![TermLossRow {
            target: "property-path:https://example/fooShape".to_string(),
            preservation_kind: "SoundUnderApproximation".to_string(),
            complexity_class: "PTIME".to_string(),
            lossy_drops: Vec::new(),
        }],
    );
    model.term_loss = Some(TermLossDigest {
        by_term: loss_by_term,
        total_property_path_rows: 1,
    });

    // Reasoned entailment for Foo (English-only executable data).
    let mut term_entailments = BTreeMap::new();
    term_entailments.insert(
        foo_iri.clone(),
        vec![crate::exec::Entailment {
            rule: "subClassOf-transitivity".to_string(),
            conclusion: "gmeow:Foo rdfs:subClassOf owl:Thing".to_string(),
            premises: vec!["gmeow:Foo rdfs:subClassOf gmeow:Bar".to_string()],
        }],
    );
    let exec = ExecutableDocsData {
        term_entailments,
        ..Default::default()
    };

    let site = render_site_lang_exec(&model, "english", &exec);
    let foo = model
        .terms
        .iter()
        .find(|t| t.curie == "gmeow:Foo")
        .expect("foo term");
    let slug = term_slug(foo);
    let json_key = format!("terms/{slug}/card.json");
    let full_key = format!("terms/{slug}/card-full.md");

    // Both machine surfaces ride alongside `card.md`.
    assert!(site.files.contains_key(&json_key), "card.json emitted");
    assert!(site.files.contains_key(&full_key), "card-full.md emitted");
    assert!(
        site.files.contains_key(&format!("terms/{slug}/card.md")),
        "card.md still emitted"
    );

    // card.json parses and EQUALS the standard-tier Card serialized through the
    // SAME `serde_json` path the MCP `doc_card format=json detail=standard` uses.
    let bytes = &site.files[&json_key];
    let parsed: serde_json::Value =
        serde_json::from_slice(bytes).expect("card.json parses as JSON");
    assert_eq!(parsed["category"], "Class");
    assert_eq!(parsed["iri"], foo_iri);
    // The standard tier carries NO rich-panel keys.
    assert!(parsed.get("entailments").is_none(), "standard omits panels");
    let facets = precompute_alignment_facets(&model);
    let expected = doc_term_card(foo, &facets, &model).projected(crate::card::CardDetail::Standard);
    let expected_bytes = serde_json::to_vec(&expected).expect("serialize standard card");
    assert_eq!(
        bytes, &expected_bytes,
        "packed card.json equals the standard Card via the same serializer"
    );

    // card-full.md carries the H1 title and EVERY rich panel (the data is present
    // for Foo), rendered by the ONE canonical renderer at the Full tier.
    let full_md = String::from_utf8(site.files[&full_key].clone()).unwrap();
    assert!(
        full_md.starts_with("# gmeow:Foo"),
        "full card H1: {full_md}"
    );
    assert!(full_md.contains("## Entailments"), "{full_md}");
    assert!(full_md.contains("## Do"), "{full_md}");
    assert!(full_md.contains("## Don't"), "{full_md}");
    assert!(full_md.contains("## Diagnostics"), "{full_md}");
    assert!(
        full_md.contains("## Degrades under projection"),
        "{full_md}"
    );
    // The full body is a strict superset of the standard body (single renderer).
    let standard_body = crate::card::render_card_body(
        &doc_term_card(foo, &facets, &model),
        crate::card::CardDetail::Standard,
    );
    assert!(
        full_md.contains(standard_body.trim_end()),
        "full card contains the whole standard body"
    );

    // Bar has NO fixtures / diagnostics / loss / entailments, so its full card is
    // an honest projection: identical to its standard card (no fabricated panels).
    let bar = model
        .terms
        .iter()
        .find(|t| t.curie == "gmeow:Bar")
        .expect("bar term");
    let bar_full =
        String::from_utf8(site.files[&format!("terms/{}/card-full.md", term_slug(bar))].clone())
            .unwrap();
    assert!(!bar_full.contains("## Entailments"), "{bar_full}");
    assert!(!bar_full.contains("## Do"), "{bar_full}");
}

#[test]
fn playground_asset_emitted_only_with_executable_data() {
    let model = tiny_model();

    // A model-only render (empty executable data) ships NO playground asset — the
    // base site is complete without the executable surfaces.
    let base = render_site_lang(&model, "english");
    assert!(
        !base.files.contains_key(PLAYGROUND_TRIG_PATH),
        "the model-only render must not emit the playground asset"
    );

    // With a playground asset supplied, it is emitted once, verbatim, under the
    // language-neutral path.
    let exec = ExecutableDocsData {
        playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
        ..Default::default()
    };
    let live = render_site_lang_exec(&model, "english", &exec);
    assert_eq!(
        live.files.get(PLAYGROUND_TRIG_PATH).map(Vec::as_slice),
        Some(exec.playground_trig.as_slice()),
        "the playground asset must be emitted verbatim when supplied"
    );
}

#[test]
fn bundle_assets_emitted_only_with_bundle_data() {
    let model = tiny_model();

    // Model-only render ships none of the browser-bundle assets.
    let base = render_site_lang(&model, "english");
    for path in [FULL_BUNDLE_GTS_PATH, BUNDLE_MANIFEST_PATH] {
        assert!(
            !base.files.contains_key(path),
            "the model-only render must not emit {path}"
        );
    }

    // With the bundle bytes supplied, the full gts + integrity manifest are
    // emitted verbatim, with the manifest carrying the asset's blake3 content
    // address and byte length.
    let full = b"\0asm-not-really-but-opaque-bytes".to_vec();
    let exec = ExecutableDocsData {
        playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
        full_bundle_gts: full.clone(),
        ..Default::default()
    };
    let live = render_site_lang_exec(&model, "english", &exec);
    assert_eq!(
        live.files.get(FULL_BUNDLE_GTS_PATH).map(Vec::as_slice),
        Some(full.as_slice()),
        "the full gts bundle must be emitted verbatim"
    );
    let manifest = String::from_utf8(
        live.files
            .get(BUNDLE_MANIFEST_PATH)
            .expect("integrity manifest emitted")
            .clone(),
    )
    .expect("manifest is utf-8");
    assert!(
        manifest.contains(&format!("blake3:{}", blake3::hash(&full).to_hex())),
        "manifest carries the full bundle's blake3 content address:\n{manifest}"
    );
    assert!(
        manifest.contains(&format!("\"bytes\": {}", full.len())),
        "manifest carries the full bundle's byte length:\n{manifest}"
    );
}

#[test]
fn conjecture_playground_ships_page_asset_and_manifest_entry() {
    let model = tiny_model();
    let full = b"\0asm-not-really-but-opaque-bytes".to_vec();
    let conjectures = b"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:demo a logic:Conjecture .\n"
        .to_vec();
    let exec = ExecutableDocsData {
        playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
        full_bundle_gts: full,
        conjectures_ttl: conjectures.clone(),
        ..Default::default()
    };
    assert!(exec.has_conjectures(), "the exec must be conjecture-backed");
    let live = render_site_lang_exec(&model, "english", &exec);

    // The demo library asset is emitted verbatim.
    assert_eq!(
        live.files.get(CONJECTURES_PATH).map(Vec::as_slice),
        Some(conjectures.as_slice()),
        "the conjecture demo library must be emitted verbatim"
    );
    // Its integrity entry rides the bundle manifest.
    let manifest = String::from_utf8(
        live.files
            .get(BUNDLE_MANIFEST_PATH)
            .expect("integrity manifest emitted")
            .clone(),
    )
    .expect("manifest is utf-8");
    assert!(
        manifest.contains(CONJECTURES_PATH)
            && manifest.contains(&format!("blake3:{}", blake3::hash(&conjectures).to_hex())),
        "manifest carries the conjecture library's content address:\n{manifest}"
    );
    // The playground page is emitted with the interactive form + both symmetric legs.
    let page_md = String::from_utf8(
        live.files
            .get(&Page::ConjecturePlayground.md_path())
            .expect("conjecture playground page emitted")
            .clone(),
    )
    .expect("page is utf-8");
    assert!(
        page_md.contains("gmeow-conjecture-form")
            && page_md.contains("proof")
            && page_md.contains("counterproof"),
        "the page presents the interactive form and both symmetric legs:\n{page_md}"
    );
}

#[test]
fn conjecture_assets_absent_without_conjecture_data() {
    let model = tiny_model();
    // A bundle-only exec (no conjecture library) must NOT emit the demo asset, the
    // playground page, or a conjectures entry in the manifest.
    let exec = ExecutableDocsData {
        full_bundle_gts: b"\0opaque".to_vec(),
        ..Default::default()
    };
    assert!(!exec.has_conjectures());
    let live = render_site_lang_exec(&model, "english", &exec);
    assert!(!live.files.contains_key(CONJECTURES_PATH));
    assert!(
        !live
            .files
            .contains_key(&Page::ConjecturePlayground.md_path())
    );
    let manifest =
        String::from_utf8(live.files.get(BUNDLE_MANIFEST_PATH).unwrap().clone()).unwrap();
    assert!(
        !manifest.contains(CONJECTURES_PATH),
        "a bundle-only manifest must not carry the conjectures entry:\n{manifest}"
    );
}

#[test]
fn playground_explains_a_chase_invented_witness() {
    let model = tiny_model();
    let exec = ExecutableDocsData {
        playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
        ..Default::default()
    };
    let page = md_playground(&model, &exec);

    // The affordance heading, the copy-pasteable query, and a prefilled `?q=` link.
    assert!(
        page.contains("Explain a chase-invented witness"),
        "witness explain heading present: {page}"
    );
    assert!(
        page.contains("gmeow:InventedWitness"),
        "the decomposition query text is emitted: {page}"
    );
    assert!(
        page.contains("gmeow:existentialOrdinal"),
        "the query decomposes the existential ordinal: {page}"
    );
    assert!(
        page.contains("sparql/index.html?q="),
        "a prefilled `?q=` playground link is emitted: {page}"
    );
    // The pin-a-null guidance (FILTER / DESCRIBE by exact Skolem IRI).
    assert!(
        page.contains("DESCRIBE <skolem-iri>") && page.contains("FILTER(?witness ="),
        "guidance to pin a specific null is present: {page}"
    );
}

#[test]
fn executable_surfaces_render_with_exec() {
    let mut model = tiny_model();
    let slice_iri = format!("{GMEOW_NS}slices/demo");
    // tiny_model has no slice; add the one its terms are owned by so the slice
    // page (and its executable sections) render.
    model.slices.push(crate::model::DocSlice {
        iri: slice_iri.clone(),
        label: Some("Demo".to_string()),
        title: None,
        tier: None,
        identifier: None,
        creators: Vec::new(),
        consumers: Vec::new(),
        profiles: Vec::new(),
        depends_on: Vec::new(),
        artifacts: Vec::new(),
        documents: Vec::new(),
        has_thesis_sentence: false,
        realized_state_complete: false,
    });
    // Add a worked example so the "try it" surface has something to render.
    model.examples.push(crate::model::DocExample {
        slice: slice_iri.clone(),
        logical_path: "examples/demo.ttl".to_string(),
        title: "Demo example".to_string(),
        text: "ex:a a gmeow:Foo .".to_string(),
        terms_referenced: vec!["gmeow:Foo".to_string()],
    });

    let mut example_inferences = std::collections::BTreeMap::new();
    example_inferences.insert(
        crate::exec::example_key(&slice_iri, "examples/demo.ttl"),
        crate::exec::InferenceDiff {
            asserted: vec!["ex:a rdf:type gmeow:Foo".to_string()],
            inferred: vec!["ex:a rdf:type owl:Thing".to_string()],
        },
    );
    let exec = ExecutableDocsData {
        example_inferences,
        cross_example: vec!["ex:shared gmeow:derived ex:x".to_string()],
        playground_trig: b"@prefix ex: <https://e/> . ex:a ex:b ex:c .\n".to_vec(),
        ..Default::default()
    };

    let site = render_site_lang_exec(&model, "english", &exec);

    // Playground page + its assets (incl. the query engine) are present.
    assert!(site.files.contains_key("sparql/index.html"));
    assert!(site.files.contains_key(DOCS_JS_PATH));
    assert!(
        site.files
            .contains_key("assets/query/gmeow_query_wasm_bg.wasm"),
        "the vendored wasm engine is emitted so the playground loads offline"
    );
    assert!(
        site.files
            .contains_key("assets/validate/gmeow_validate_wasm_bg.wasm"),
        "the vendored validator wasm engine is emitted alongside purrdf so the site \
             validates authored RDF client-side"
    );
    let sparql = String::from_utf8(site.files["sparql/index.html"].clone()).unwrap();
    assert!(
        sparql.contains("id=\"gmeow-sparql\""),
        "the query form renders"
    );
    assert!(
        sparql.contains(DOCS_JS_PATH),
        "the playground page loads the controller module"
    );
    assert!(sparql.contains("SPARQL"), "the SPARQL nav entry is present");
    assert!(
        sparql.contains("Cross-example inferences"),
        "the cross-example bucket is surfaced (no silent drop)"
    );

    // No static per-format export files — export runs through the playground.
    assert!(
        !site.files.keys().any(|k| k.starts_with("export/")),
        "export is client-side via the playground; no static export files"
    );
    let slug = slice_slug(model.slices.iter().find(|s| s.iri == slice_iri).unwrap());
    let slice_md =
        String::from_utf8(site.files[&format!("slices/{slug}/index.md")].clone()).unwrap();
    assert!(
        slice_md.contains("## Export"),
        "the slice page has an Export section"
    );
    assert!(
        slice_md.contains("sparql/index.html?q="),
        "the slice export links into the playground"
    );
    assert!(
        slice_md.contains("Try it"),
        "the slice page shows the reasoner try-it inferences"
    );
    assert!(
        slice_md.contains("owl:Thing"),
        "the inferred triple appears in the try-it block"
    );

    // Non-English trees carry NO executable surfaces (bundle-size gate).
    let fr = render_site_lang_exec(&model, "fr", &exec);
    assert!(
        !fr.files.contains_key("sparql/index.html") && !fr.files.contains_key(PLAYGROUND_TRIG_PATH),
        "the executable surfaces live only in the English carrier tree"
    );
}

#[test]
fn okf_doc_reference_matches_the_bundle_scheme() {
    // Class / property / individual terms reference their `gmeow-okf/` document
    // by the SAME {category-dir}/{local-name}.md scheme the OKF projection emits;
    // datatypes / other categories have no per-concept OKF document.
    let mut class = DocTerm {
        iri: format!("{GMEOW_NS}Foo"),
        curie: "gmeow:Foo".to_string(),
        category: DocTermCategory::Class,
        ..Default::default()
    };
    assert_eq!(
        okf_doc_reference(&class).as_deref(),
        Some("gmeow-okf/classes/Foo.md")
    );
    class.category = DocTermCategory::Property;
    assert_eq!(
        okf_doc_reference(&class).as_deref(),
        Some("gmeow-okf/properties/Foo.md")
    );
    class.category = DocTermCategory::Individual;
    assert_eq!(
        okf_doc_reference(&class).as_deref(),
        Some("gmeow-okf/individuals/Foo.md")
    );
    class.category = DocTermCategory::Datatype;
    assert_eq!(okf_doc_reference(&class), None);
}

#[test]
fn term_page_renders_usage_advice_and_alignments() {
    let mut model = tiny_model();
    // Enrich Foo with every advisory field + one consumer profile, and add a
    // documented consumer term so the consumer link resolves internally.
    let foo = model
        .terms
        .iter_mut()
        .find(|t| t.curie == "gmeow:Foo")
        .expect("Foo present");
    foo.scope_notes = vec!["Scope of the foo.".to_string()];
    foo.examples = vec!["ex:x a gmeow:Foo .".to_string()];
    foo.use_when = vec!["Use when foo-ing.".to_string()];
    foo.avoid_when = vec!["Avoid when bar-ing.".to_string()];
    foo.how_to_use = vec!["Reference via gmeow:hasFoo.".to_string()];
    foo.use_for_consumer = vec!["gmeow:Bar".to_string(), "ext:Other".to_string()];

    // One alignment cross-walk on Foo.
    model.linkages.push(crate::model::DocLinkage {
        mapping_set: None,
        subject: format!("{GMEOW_NS}Foo"),
        subject_curie: "gmeow:Foo".to_string(),
        predicate: "http://www.w3.org/2004/02/skos/core#closeMatch".to_string(),
        object: "http://www.wikidata.org/entity/Q42".to_string(),
        justification: None,
        confidence: None,
        owner_slice: format!("{GMEOW_NS}slices/demo"),
    });

    let md = to_markdown(&model, &Page::Term("foo".to_string()));

    // Usage Advice section + every field label, in order.
    assert!(md.contains("## Usage Advice"), "advice heading present");
    for label in [
        "Scope",
        "Example",
        "Use when",
        "Avoid when",
        "How to use",
        "Use for consumers",
    ] {
        assert!(
            md.contains(&format!("**{label}:**")),
            "missing advice label {label}"
        );
    }
    // Documented consumer resolves to an internal link; undocumented stays a CURIE.
    assert!(md.contains("[`gmeow:Bar`]"), "documented consumer linked");
    assert!(
        md.contains("`ext:Other`"),
        "undocumented consumer shown as code"
    );

    // Alignments section uses the short predicate tag and links the object.
    assert!(md.contains("## Alignments"), "alignments heading present");
    assert!(md.contains("`closeMatch`"), "predicate short tag");
    // The external object IRI is md-escaped (`.` → `\.`); match a dot-free tail.
    assert!(md.contains("entity/Q42"), "alignment object linked");
    // The approximate (closeMatch) crosswalk carries an inline lossy caveat and
    // the section cross-links the preservation loss ledger.
    assert!(
        md.contains("approximate match (close)"),
        "inline lossy-projection caveat present"
    );
    assert!(
        md.contains("preservation loss ledger"),
        "loss-ledger cross-link present for an approximate alignment"
    );
    // Any crosswalk also discloses that its EDOAL/FnO lowering is lossy.
    assert!(
        edoal_fno_lowering_is_lossy(),
        "the EDOAL/FnO lowerings are declared lossy in the projection ledger"
    );
    assert!(
        md.contains("lowered to EDOAL"),
        "per-term EDOAL/FnO lowering caveat present on an aligned term"
    );

    // Bar carries no advice/alignments → neither section appears on its page.
    let bar_md = to_markdown(&model, &Page::Term("bar".to_string()));
    assert!(
        !bar_md.contains("## Usage Advice"),
        "empty advice suppressed"
    );
    assert!(
        !bar_md.contains("## Alignments"),
        "empty alignments suppressed"
    );
}

/// The always-present Stability badge must render every `DocTermStability`
/// variant. The `deprecated` arm is otherwise never exercised by the term
/// goldens (no production term is `owl:deprecated` — this project deletes
/// rather than deprecates), so this is the only coverage of that render
/// path. The derivation logic itself is unit-tested separately in
/// `model::tests::stability_resolves_by_precedence`.
#[test]
fn stability_badge_renders_every_state() {
    let mut model = tiny_model();
    model.terms.iter_mut().for_each(|t| {
        t.stability = match t.curie.as_str() {
            "gmeow:Foo" => DocTermStability::Deprecated,
            _ => DocTermStability::Experimental,
        };
    });

    let foo_md = to_markdown(&model, &Page::Term("foo".to_string()));
    assert!(foo_md.contains("## Stability"), "stability heading present");
    assert!(
        foo_md.contains("- **Status:** deprecated"),
        "deprecated badge renders: {foo_md}"
    );

    let bar_md = to_markdown(&model, &Page::Term("bar".to_string()));
    assert!(
        bar_md.contains("- **Status:** experimental"),
        "experimental badge renders: {bar_md}"
    );

    // The default (no override, core-tier) resolves to `stable` and still
    // renders unconditionally — assert via the variant label directly.
    assert_eq!(DocTermStability::Stable.label(), "stable");
}

#[test]
fn finding_category_display_humanizes_finding_classes() {
    assert_eq!(
        finding_category_display("https://blackcatinformatics.ca/logic/FindingPolicyWarning"),
        "Finding: Policy Warning"
    );
    assert_eq!(
        finding_category_display("https://blackcatinformatics.ca/logic/FindingDataShapeViolation"),
        "Finding: Data Shape Violation"
    );
    // A non-Finding IRI degrades to its local name verbatim.
    assert_eq!(
        finding_category_display("https://example.org/vocab/SomethingElse"),
        "Something Else"
    );
}

#[test]
fn constraint_catalog_anchors_rule_by_helpuri_slug() {
    use crate::model::ConstraintRule;

    let mut model = tiny_model();
    // `box-roles.invalid` → slug `box-roles-invalid` (the validator's transform:
    // `.`/`/` → `-`), which is the fragment of the rule's helpUri.
    let rule = ConstraintRule {
        code: "box-roles.invalid".to_string(),
        slug: gmeow_validate::rule_catalog::slugify("box-roles.invalid"),
        category: "https://blackcatinformatics.ca/logic/FindingPolicyWarning".to_string(),
        severity: "binding".to_string(),
        help_uri:
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#box-roles-invalid"
                .to_string(),
        label: Some("box-roles.invalid".to_string()),
        definition: Some("Every box declares exactly one valid box role.".to_string()),
        applies_to_terms: vec![format!("{GMEOW_NS}Foo")],
        formalizes: None,
    };
    // The rendered anchor id MUST equal the helpUri fragment so a finding's
    // help link resolves to its rule entry.
    let fragment = rule.help_uri.rsplit('#').next().unwrap().to_string();
    assert_eq!(rule.slug, fragment, "anchor slug == helpUri fragment");
    model.constraint_rules = vec![rule];

    let md = to_markdown(&model, &Page::ConstraintCatalog);
    assert!(
        md.contains(&format!("<a id=\"{fragment}\"></a>")),
        "explicit anchor emitted: {md}"
    );
    assert!(md.contains("Finding: Policy Warning"), "category heading");
    assert!(md.contains("**binding**"), "severity rendered");
    // The definition is md-escaped (`.` → `\.`), so match a bare substring.
    assert!(
        md.contains("Every box declares exactly one valid box role"),
        "definition rendered: {md}"
    );
    // The applies-to term links internally to the documented gmeow:Foo term.
    assert!(
        md.contains("[`gmeow:Foo`]"),
        "applies-to term is a curie link: {md}"
    );
}

#[test]
fn constraint_catalog_empty_renders_empty_state() {
    let model = tiny_model(); // constraint_rules empty
    let md = to_markdown(&model, &Page::ConstraintCatalog);
    assert!(
        md.contains("No validation rules are declared in the constraint catalog."),
        "empty-state line renders: {md}"
    );
}
