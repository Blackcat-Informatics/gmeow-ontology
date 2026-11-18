// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_docs_model::slug::provenance_chain;

/// The RDF-to-developer glossary: a fixed mapping from the RDF / OWL / SHACL
/// vocabulary this site uses to the everyday software concepts they correspond
/// to, so a reader who does not know RDF has a bridge into the rest of the site.
/// A pure, authored table (no model data), deterministic.
pub(super) fn md_glossary(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_glossary"));
    line(
        &mut out,
        "If you build software but have not worked with RDF, these are the \
         everyday concepts behind the vocabulary this site uses. The mapping is \
         approximate — a bridge into the model, not an exact equivalence.",
    );
    push_line(
        &mut out,
        "| RDF / OWL / SHACL | Developer concept | Notes |",
    );
    push_line(&mut out, "| --- | --- | --- |");
    // (rdf-term, developer-concept, note) — authored, sorted-by-intent.
    const ROWS: &[(&str, &str, &str)] = &[
        (
            "Class",
            "Type",
            "A named category of thing — like a struct/record type or an interface.",
        ),
        (
            "Property",
            "Field",
            "A typed attribute or relation on a thing — like a struct field or a foreign key.",
        ),
        (
            "Individual",
            "Instance / value",
            "A concrete thing of some class — like a row or an object instance.",
        ),
        (
            "IRI",
            "Typed global ID",
            "A globally-unique identifier for a thing — like a URL-shaped primary key.",
        ),
        (
            "Triple",
            "Row (subject, predicate, object)",
            "One fact: `subject predicate object` — like a single (id, column, value) cell.",
        ),
        (
            "SHACL shape",
            "Schema validation",
            "Constraints a graph must satisfy — like a JSON Schema or a DB `CHECK`/`NOT NULL`.",
        ),
        (
            "Ontology",
            "Schema + rules",
            "The class/property vocabulary plus its axioms — a schema that also entails new facts.",
        ),
        (
            "rdfs:subClassOf",
            "Subtype / `extends`",
            "One class specialises another — like inheritance between types.",
        ),
        (
            "Domain / range",
            "Field's owner type / value type",
            "Which class a property applies to, and what type its values take.",
        ),
        (
            "Reasoner",
            "Inference engine",
            "Derives entailed facts and detects contradictions from the axioms.",
        ),
    ];
    for (rdf, dev, note) in ROWS {
        push_line(
            &mut out,
            &format!(
                "| `{}` | {} | {} |",
                code_escape(rdf),
                md_escape(dev),
                md_escape(note)
            ),
        );
    }
    blank(&mut out);
    out
}

/// A link from a grounding-slice IRI (a `gmeow:seamFromSlice`/`seamToSlice`
/// object) to its slice page, falling back to the IRI's local name in a code
/// span when the slice is unresolvable (never happens for a well-formed
/// registry, but this is a documentation READ, not a re-validation).
fn seam_slice_link(model: &DocsModel, from: &str, slice_iri: &str) -> String {
    if let Some(slice) = model.slices.iter().find(|s| s.iri == slice_iri) {
        let href = rel(from, &Page::Slice(slice_slug(slice)).dir());
        return format!("[{}]({}index.md)", md_escape(&slice_display(slice)), href);
    }
    md_escape(local_name(slice_iri))
}

/// The grounding seam registry: every sanctioned cross-grounding `gmeow:Seam`
/// — the closed channel set that authorizes a term reference to cross a
/// peered grounding-slice pair (Principle 19). Rendered directly from
/// [`crate::model::DocsModel::seams`] — the canonical governance data authored
/// in the grounding slices' `manifest.ttl` files — so this page is always a
/// faithful projection of that data, never a hand-maintained duplicate (see
/// `gmeow_validate`'s seam-registry drift gate). Deterministic: `model.seams`
/// is already IRI-sorted, and every per-seam collection is sorted/deduped.
pub(super) fn md_seam_registry(model: &DocsModel) -> String {
    let from = Page::SeamRegistry.dir();
    let mut out = String::new();
    heading(&mut out, 1, "Grounding seams");
    line(
        &mut out,
        &format!(
            "The closed set of **{}** sanctioned cross-grounding reference channels among the \
             `logic:`, `lang:`, and `math:` grounding slices (Principle 19). Every peered \
             cross-slice term reference must land on one of these seams rather than riding free \
             on the `gmeow:sliceCoFoundationalWith` peerage grant. Each row is a `gmeow:Seam` \
             individual authored as canonical data in a grounding slice's `manifest.ttl`.",
            model.seams.len(),
        ),
    );

    if model.seams.is_empty() {
        line(&mut out, "No grounding seams are declared in this model.");
        return out;
    }

    push_line(
        &mut out,
        "| Seam | Direction | Carrying terms | Owning doc |",
    );
    push_line(&mut out, "| --- | --- | --- | --- |");
    for seam in &model.seams {
        let name = seam.label.clone().unwrap_or_else(|| to_curie(&seam.iri));
        let directions = seam
            .directions
            .iter()
            .map(|d| {
                format!(
                    "{} → {}",
                    seam_slice_link(model, &from, &d.from),
                    seam_slice_link(model, &from, &d.to)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let carrying = seam
            .carrying_terms
            .iter()
            .map(|iri| curie_link(model, &from, &to_curie(iri)))
            .collect::<Vec<_>>()
            .join(", ");
        let owning = seam
            .owning_docs
            .iter()
            .map(|d| format!("`{}`", code_escape(d)))
            .collect::<Vec<_>>()
            .join(", ");
        push_line(
            &mut out,
            &format!(
                "| **{}** | {} | {} | {} |",
                md_escape(&name),
                directions,
                carrying,
                owning,
            ),
        );
    }
    blank(&mut out);

    // A definitions section: the full prose each seam's `skos:definition`
    // carries, keyed to the same seam name the table above uses.
    heading(&mut out, 2, "Definitions");
    for seam in &model.seams {
        let name = seam.label.clone().unwrap_or_else(|| to_curie(&seam.iri));
        heading(&mut out, 3, &name);
        if let Some(definition) = &seam.definition {
            line(&mut out, &md_escape(definition));
        }
    }
    out
}

/// The short four-boxes label for a `gmeow:box*` role CURIE (`gmeow:boxTBox` →
/// `TBox`); the CURIE unchanged when it does not match the expected shape.
pub(super) fn box_role_label(role: &str) -> String {
    role.strip_prefix("gmeow:box")
        .filter(|s| !s.is_empty())
        .unwrap_or(role)
        .to_string()
}

/// The logic-stereotypes index: every term grouped by its `logic:` stereotype.
/// Resolves the `nav_logic` chrome string to a real page.
pub(super) fn md_logic_index(model: &DocsModel) -> String {
    let from = Page::Logic.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_logic_and_reasoning"));
    line(
        &mut out,
        "Terms grouped by their lowered OntoUML/UFO stereotype (the `logic:` discipline \
         each term carries). See the slice doctrine under `slices/grounding/logic` for the \
         stereotype semantics.",
    );

    // Cross-link to the four `logic:` compiler-product pages. The compiler turns
    // one canonical IR into every projection target; these pages document the four
    // information products that fall out of that compile — the IR itself, the
    // preservation loss ledger, the derivation-graph explanations, and the
    // diagnostics. Links use the same `rel` helper the rest of the page does, so
    // they resolve cleanly under the anchor gate.
    heading(&mut out, 2, model.ui("body_compiler_products"));
    line(
        &mut out,
        "The `logic:` compiler emits four information products from one canonical \
         program. Each has a dedicated page:",
    );
    let ir_href = rel(&from, &Page::LogicCanonicalIr.dir());
    let ledger_href = rel(&from, &Page::LogicLossLedger.dir());
    let deriv_href = rel(&from, &Page::LogicDerivationGraph.dir());
    let diag_href = rel(&from, &Page::LogicDiagnostics.dir());
    push_line(
        &mut out,
        &format!(
            "- [Canonical IR]({ir_href}index.md) — the one AST / RDF 1.2 identity \
             serialization the compiler produces."
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Preservation loss ledger]({ledger_href}index.md) — per projection \
             target: preservation kind, complexity class, and structural lossy drops."
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Derivation graph]({deriv_href}index.md) — the reasoning provenance: \
             per-axiom proof skeletons."
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Compiler diagnostics]({diag_href}index.md) — parse findings plus \
             lossy-drop notes, surfaced as SARIF."
        ),
    );
    blank(&mut out);

    // Cross-link to the grounding seam registry — the closed set of sanctioned
    // cross-grounding reference channels among `logic:`/`lang:`/`math:`.
    let seams_href = rel(&from, &Page::SeamRegistry.dir());
    push_line(
        &mut out,
        &format!(
            "See also the [grounding seam registry]({seams_href}index.md) — the sanctioned \
             cross-grounding reference channels among the `logic:`/`lang:`/`math:` grounding \
             slices."
        ),
    );
    blank(&mut out);

    // Group terms by each stereotype they carry (a term may carry several).
    let mut by_stereotype: BTreeMap<String, Vec<&DocTerm>> = BTreeMap::new();
    for term in &model.terms {
        for stereotype in &term.logic_stereotypes {
            by_stereotype
                .entry(stereotype.clone())
                .or_default()
                .push(term);
        }
    }

    if by_stereotype.is_empty() {
        line(&mut out, model.ui("body_no_logic_stereotypes"));
        return out;
    }

    for (stereotype, mut terms) in by_stereotype {
        terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
        heading(
            &mut out,
            2,
            &format!("{stereotype} ({} term(s))", terms.len()),
        );
        for term in terms {
            push_line(
                &mut out,
                &format!("- {}", term_link(model, &from, &term.iri)),
            );
        }
        blank(&mut out);
    }
    out
}

// ── Logic compiler products ───────────────────────────────────────────────────
//
// The `logic:` compiler turns one canonical program into every projection
// target; four information products fall out of that compile. Each gets a
// dedicated page under the logic area. The loss-ledger page reads the public
// `gmeow_logic_compile::projections::projection_ledger_rows` accessor (the
// crate-private `target_meta` table is the source of truth); the other three are
// high-level prose pages — they describe products, not fabricated APIs.

/// The canonical-IR product page: the one AST / RDF 1.2 identity serialization
/// the compiler produces, and the projection targets it feeds.
pub(super) fn md_logic_canonical_ir(model: &DocsModel) -> String {
    let from = Page::LogicCanonicalIr.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_canonical_ir"));
    line(
        &mut out,
        "The `logic:` compiler parses every source surface into a single canonical \
         intermediate representation — **the one AST**. The IR is a frozen value \
         hierarchy with an order-independent canonicalization contract: every \
         ordering and hash the downstream artifacts depend on bottoms out in the \
         IR's stable sort keys, so the same program always compiles byte-for-byte \
         identically.",
    );
    line(
        &mut out,
        "The IR's own faithful serialization is the **canonical RDF 1.2** projection: \
         a lossless, identity round-trip (`ExactPreservation`). Re-parsing those \
         triples reconstructs the IR exactly — it is the AST written down, not a \
         lossy view of it.",
    );

    heading(&mut out, 2, model.ui("body_projection_surface"));
    line(
        &mut out,
        "From the one IR the compiler runs every projection back-end. The standard \
         whole-program targets are:",
    );
    for row in gmeow_logic_compile::projections::projection_ledger_rows() {
        push_line(&mut out, &format!("- `{}`", code_escape(&row.target)));
    }
    blank(&mut out);
    let ledger_href = rel(&from, &Page::LogicLossLedger.dir());
    line(
        &mut out,
        &format!(
            "Each target declares how faithfully it preserves the IR; the \
             [preservation loss ledger]({ledger_href}index.md) records the \
             preservation kind, complexity class, and structural drops per target."
        ),
    );
    out
}

/// The preservation loss-ledger product page: a table built from the public
/// `projection_ledger_rows` accessor — one row per standard projection target.
pub(super) fn md_logic_loss_ledger(model: &DocsModel) -> String {
    let from = Page::LogicLossLedger.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_preservation_loss_ledger"));
    line(
        &mut out,
        "Every projection of the canonical IR declares **how much it preserves**. A \
         lossy down-projection (OWL, gUFO, Datalog, …) is sound but drops structure \
         the target format cannot carry; exact targets such as canonical RDF 1.2 \
         drop nothing. This ledger is the per-target record — the compiler's \
         overclaim gate turns the build red if a target claims exact preservation \
         yet drops anything.",
    );

    let ir_href = rel(&from, &Page::LogicCanonicalIr.dir());
    line(
        &mut out,
        &format!(
            "The rows below are the static, whole-program targets the \
             [canonical IR]({ir_href}index.md) feeds; a concrete program may add a \
             per-shape property-path row for each declared path shape."
        ),
    );

    push_line(
        &mut out,
        "| Target | Preservation kind | Complexity class | Lossy drops |",
    );
    push_line(&mut out, "| --- | --- | --- | --- |");
    for row in gmeow_logic_compile::projections::projection_ledger_rows() {
        // The drops are a structured list; render them as a `<br>`-separated cell so
        // the four-column grid stays intact (a multi-line cell would break it). An
        // exact target carries no drops — show an em dash rather than an empty cell.
        let drops = if row.lossy_drops.is_empty() {
            "—".to_string()
        } else {
            row.lossy_drops
                .iter()
                .map(|d| md_escape(&one_line(d)))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        push_line(
            &mut out,
            &format!(
                "| `{}` | `{}` | `{}` | {} |",
                code_escape(&row.target),
                code_escape(&row.preservation_kind),
                code_escape(&row.complexity),
                drops,
            ),
        );
    }
    blank(&mut out);

    // ── Worked, authored examples ────────────────────────────────────────────
    // Distinct from the static whole-program rows above: these come from
    // concrete AUTHORED artifacts (`examples/*.ttl`, any slice — not just the
    // logic slice's `projection-loss-ledger.ttl`) applying the SAME
    // preservation-kind vocabulary to a specific report, not a whole target
    // class. Discovered generically by `gmeow_docs::model::extract_loss_targets`:
    // any example subject carrying both `logic:preservationKind` and
    // `logic:complexityClass` becomes a row here.
    heading(&mut out, 2, model.ui("body_worked_preservation_examples"));
    line(
        &mut out,
        "The compiler ledger above covers the static, whole-program targets; \
         these are worked, authored examples of the SAME preservation-kind \
         vocabulary applied to concrete artifacts — a projection report, a \
         bridge view, a closed-world SHACL-to-JSON-Schema compile — each one an \
         instance, not a target class.",
    );
    if model.loss_targets.is_empty() {
        line(&mut out, model.ui("body_no_worked_preservation_examples"));
    } else {
        push_line(
            &mut out,
            "| Target | Label | Preservation kind | Complexity class |",
        );
        push_line(&mut out, "| --- | --- | --- | --- |");
        for row in &model.loss_targets {
            let label = row
                .label
                .as_deref()
                .map(|l| md_escape(&one_line(l)))
                .unwrap_or_else(|| "—".to_string());
            push_line(
                &mut out,
                &format!(
                    "| `{}` | {} | `{}` | `{}` |",
                    code_escape(&row.target),
                    label,
                    code_escape(&row.preservation_kind),
                    code_escape(&row.complexity_class),
                ),
            );
        }
        blank(&mut out);
    }
    out
}

/// The derivation-graph product page: the reasoning-provenance explanation
/// skeletons (per-axiom proof skeletons) the reasoning channel ships.
pub(super) fn md_logic_derivation_graph(model: &DocsModel) -> String {
    let from = Page::LogicDerivationGraph.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_derivation_graph"));
    line(
        &mut out,
        "Beyond the projected programs, the reasoning channel produces a **derivation \
         graph** — the provenance of every entailed fact. Each derived statement \
         carries a proof skeleton: the axiom(s) and the body bindings that justified \
         it, chained back to the source facts. The graph is the content-addressed \
         explanation surface — it is what makes a conclusion auditable rather than \
         opaque.",
    );
    line(
        &mut out,
        "The skeletons double as the truth-maintenance justification: when a premise \
         is retracted, the derivation graph identifies exactly which conclusions \
         lose their support. Because the explanation is keyed by content, two runs \
         over the same program yield the same justification graph — the basis for \
         the explanation goldens.",
    );
    let ir_href = rel(&from, &Page::LogicCanonicalIr.dir());
    line(
        &mut out,
        &format!(
            "This product ships in the bundle's `reasoning-archive` channel as \
             `generated/logic/reasoning-explanations.rdf12.ttl`, folded alongside the \
             compiled programs the [canonical IR]({ir_href}index.md) projects, so a \
             consumer can carry the conclusions and their proofs together."
        ),
    );
    out
}

/// The compiler-diagnostics product page: parse findings + lossy-drop notes,
/// surfaced as SARIF.
pub(super) fn md_logic_diagnostics(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_compiler_diagnostics"));
    line(
        &mut out,
        "Compiling a `logic:` program is also a **diagnostic pass**. The compiler \
         emits two kinds of finding:",
    );
    push_line(
        &mut out,
        "- **Parse findings** — malformed or ill-typed source surfaces flagged at \
         the point they are read.",
    );
    push_line(
        &mut out,
        "- **`preservation.rung.structural` / `preservation.rung.actual` notes** — one \
         finding per structural limitation a lossy projection carries and per concrete \
         item it had to drop, so the loss is never silent. Each actual drop carries its \
         causing structural limitation as an antecedent, and these line up with the \
         per-target rows in the preservation loss ledger.",
    );
    blank(&mut out);
    line(
        &mut out,
        "The findings are surfaced as **SARIF**, the standard static-analysis result \
         format, so they flow into the same code-scanning surface as the rest of the \
         repository's diagnostics — no bespoke reader required.",
    );
    out
}

pub(super) fn md_slice_index(model: &DocsModel) -> String {
    let from = Page::SliceIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_slices"));
    line(
        &mut out,
        &format!("{} compilation unit(s).", model.slices.len()),
    );

    heading(&mut out, 2, model.ui("body_dependency_graph"));
    push_line(
        &mut out,
        &format!(
            "![Slice dependency graph]({}diagrams/slices.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    push_line(&mut out, "| Slice | Tier | IRI |");
    push_line(&mut out, "| --- | --- | --- |");
    // model.slices is already sorted by IRI.
    for slice in &model.slices {
        let href = rel(&from, &Page::Slice(slice_slug(slice)).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | `{}` |",
                md_escape(&slice_display(slice)),
                href,
                md_escape(&tier_name(slice)),
                code_escape(&slice.iri)
            ),
        );
    }
    blank(&mut out);
    out
}

pub(super) fn md_slice(
    model: &DocsModel,
    slug: &str,
    page_map: &SourceToPageMap,
) -> Result<String> {
    let Some(slice) = model.slices.iter().find(|s| slice_slug(s) == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_slice_not_found"));
        return Ok(out);
    };
    let from = Page::Slice(slug.to_string()).dir();

    let mut out = String::new();
    heading(&mut out, 1, &slice_display(slice));
    line(&mut out, &format!("`{}`", code_escape(&slice.iri)));

    // Manifest identity.
    push_line(&mut out, "| Field | Value |");
    push_line(&mut out, "| --- | --- |");
    push_line(
        &mut out,
        &format!("| Tier | {} |", md_escape(&tier_name(slice))),
    );
    if let Some(id) = &slice.identifier {
        push_line(&mut out, &format!("| Identifier | `{}` |", code_escape(id)));
    }
    if !slice.creators.is_empty() {
        // Show supplied language and direction beside annotated names. Plain
        // proper names retain their lexical display; every source value is shown.
        let creators = slice
            .creators
            .iter()
            .map(|creator| {
                let literal = creator.object();
                if literal.language.is_some() || literal.direction.is_some() {
                    purrdf::emit_term(&purrdf::RdfTerm::Literal(literal.clone()))
                } else {
                    literal.lexical_form.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        push_line(
            &mut out,
            &format!("| Creators | {} |", md_escape(&creators)),
        );
    }
    if !slice.consumers.is_empty() {
        push_line(
            &mut out,
            &format!("| Consumers | {} |", md_escape(&slice.consumers.join(", "))),
        );
    }
    blank(&mut out);

    // `page_map` is the single link-rewrite authority (a pure function of the model's
    // already-validated document set), built ONCE per render pass and threaded in — it
    // drives the grafted `docs.md` prose, the child-document index, and every internal
    // link those surfaces rewrite.

    // Graft the slice's top-level `docs.md` narrative directly onto the slice page,
    // between the manifest metadata and the generated artifact/term/linkage sections.
    // Every heading is demoted one level (H1→H2 … H5→H6) so the generated slice H1
    // stays the unique page H1, and every internal link is rewritten through the
    // page map. The thesis / realized-state FACTS are still read separately from
    // `docs.md` (see `DocSlice::from_record`); this additionally RENDERS its prose.
    if let Some(docs_md) = slice
        .documents
        .iter()
        .find(|d| d.source_path == SLICE_PAGE_SOURCE)
    {
        let grafted = rewrite_doc_body(
            &docs_md.source_text,
            page_map,
            &slice.iri,
            SLICE_PAGE_SOURCE,
            &from,
            HeadingDemotion::GraftOne,
            true,
        )?;
        out.push_str(&grafted);
        blank(&mut out);
    }

    // The slice's child documents (every non-`docs.md` markdown), by title, sorted
    // by logical path, each linking to its own generated page and retaining its
    // source path + raw digest as visible provenance.
    let children = page_map.slice_children(&slice.iri);
    if !children.is_empty() {
        heading(&mut out, 2, "Documents");
        for entry in &children {
            let target_dir = entry.page.strip_suffix('/').unwrap_or(&entry.page);
            let href = rel(&from, target_dir);
            push_line(
                &mut out,
                &format!(
                    "- [{}]({href}index.md) — `{}` (`{}`)",
                    md_escape(&entry.title),
                    code_escape(&entry.source_path),
                    code_escape(&short_digest(&entry.raw_digest)),
                ),
            );
        }
        blank(&mut out);
    }

    // Artifact inventory grouped by role (by reference: path + media type + digest).
    if !slice.artifacts.is_empty() {
        heading(&mut out, 2, model.ui("body_artifacts"));
        // Group by role-name; both the group order and the within-group order are
        // sorted for determinism.
        let mut by_role: BTreeMap<String, Vec<&crate::model::DocArtifact>> = BTreeMap::new();
        for artifact in &slice.artifacts {
            by_role
                .entry(role_name(&artifact.role))
                .or_default()
                .push(artifact);
        }
        for (role, mut artifacts) in by_role {
            artifacts.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
            heading(&mut out, 3, &role);
            push_line(&mut out, "| Path | Media type | Digest |");
            push_line(&mut out, "| --- | --- | --- |");
            for artifact in artifacts {
                push_line(
                    &mut out,
                    &format!(
                        "| `{}` | `{}` | `{}` |",
                        code_escape(&artifact.logical_path),
                        code_escape(&artifact.media_type),
                        code_escape(&short_digest(&artifact.raw_digest))
                    ),
                );
            }
            blank(&mut out);
        }
    }

    // Terms owned by this slice.
    let mut owned: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|t| t.owner_slice == slice.iri)
        .collect();
    owned.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    if !owned.is_empty() {
        heading(&mut out, 2, &format!("Terms ({})", owned.len()));
        for term in owned {
            let href = rel(&from, &Page::Term(term_slug(term)).dir());
            push_line(
                &mut out,
                &format!(
                    "- [`{}`]({}index.md) — {}",
                    code_escape(&term.curie),
                    href,
                    md_escape(category_singular(term.category))
                ),
            );
        }
        blank(&mut out);
    }

    // Linkages whose subject is owned by this slice.
    let mut slice_links: Vec<&crate::model::DocLinkage> = model
        .linkages
        .iter()
        .filter(|l| l.owner_slice == slice.iri)
        .collect();
    slice_links.sort_by(|a, b| {
        a.subject_curie
            .cmp(&b.subject_curie)
            .then_with(|| a.object.cmp(&b.object))
    });
    if !slice_links.is_empty() {
        heading(&mut out, 2, &format!("Linkages ({})", slice_links.len()));
        push_line(
            &mut out,
            "| Subject | Predicate | External object | Conf. |",
        );
        push_line(&mut out, "| --- | --- | --- | --- |");
        for link in slice_links {
            push_line(
                &mut out,
                &format!(
                    "| {} | `{}` | [{}]({}) | {} |",
                    subject_link(model, &from, link),
                    code_escape(&link.predicate),
                    md_escape(&link.object),
                    md_escape(&link.object),
                    confidence_cell(link.confidence),
                ),
            );
        }
        blank(&mut out);
    }

    // Worked examples owned by this slice — rendered IN FULL.
    let mut slice_examples: Vec<&crate::model::DocExample> = model
        .examples
        .iter()
        .filter(|e| e.slice == slice.iri)
        .collect();
    slice_examples.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
    if !slice_examples.is_empty() {
        heading(&mut out, 2, &format!("Examples ({})", slice_examples.len()));
        for example in slice_examples {
            heading(&mut out, 3, &example.title);
            line(
                &mut out,
                &format!("`{}`", code_escape(&example.logical_path)),
            );
            fenced(&mut out, "turtle", &example.text);
        }
    }

    Ok(out)
}

/// Render a slice child document (`Page::SliceDocument`) as its own page: the
/// document's `source_text` Markdown IS the page body — its authored H1 stays the
/// page H1 (NO heading demotion) — with every internal markdown→markdown link and
/// fragment rewritten through the single [`SourceToPageMap`] authority. A dangling
/// WITHIN-slice link hard-fails (never renders silently); off-corpus references
/// (another slice, a repo `docs/` file, a non-markdown asset) are absolutized to
/// the published site. List provenance (source path + digest) lives on the SLICE
/// page's document index, not on the child body itself.
pub(super) fn md_slice_document(
    model: &DocsModel,
    slice_iri: &str,
    path: &str,
    page_map: &SourceToPageMap,
) -> Result<String> {
    let page_dir = Page::SliceDocument {
        slice: slice_iri.to_string(),
        path: path.to_string(),
    }
    .dir();
    let doc = model
        .slices
        .iter()
        .find(|s| s.iri == slice_iri)
        .and_then(|s| s.documents.iter().find(|d| d.source_path == path));
    let Some(doc) = doc else {
        let mut out = String::new();
        heading(&mut out, 1, path);
        line(&mut out, model.ui("body_slice_not_found"));
        return Ok(out);
    };
    rewrite_doc_body(
        &doc.source_text,
        page_map,
        slice_iri,
        path,
        &page_dir,
        HeadingDemotion::Keep,
        true,
    )
}

// ── First-class Markdown document rewrite (graft + child page + llms corpus) ──

/// Re-emit an authored slice Markdown document (`docs.md` or a child `*.md`) with
/// every internal link resolved through the single [`SourceToPageMap`] authority
/// and, when `demote` is set, every ATX heading demoted one level.
///
/// The body is processed line by line so every Markdown construct is preserved
/// verbatim except the two deliberate rewrites. Fenced code blocks (```` ``` ````
/// / `~~~`) and inline code spans are passed through untouched, so a `#` or a
/// `](…)` inside a code sample is never mistaken for a heading or a link.
///
/// * `from_slice` / `from_path` locate the document in source space (the resolver
///   joins relative links against `from_path`).
/// * `page_dir` is the site directory of the page the body is rendered ONTO (the
///   slice page for a graft, the child page for a child render, `""` for the
///   root-level llms corpus) — relative hrefs are computed from it.
/// * `demote` selects the heading transform (see [`HeadingDemotion`]).
/// * `inject_anchors` — when set, emit an explicit `<a id="{slug}"></a>` before
///   each heading using the map's page-scoped, text-derived slugs (matched in
///   source order). pulldown-cmark emits no heading `id`, so these anchors are what
///   the rewritten `#slug` / `…index.md#slug` cross-links resolve to on the HTML
///   page. Off for the plain-text `llms-full` corpus (no HTML anchors there).
pub(super) fn rewrite_doc_body(
    source: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
    demote: HeadingDemotion,
    inject_anchors: bool,
) -> Result<String> {
    // The page-scoped heading anchors, in source order, for the id injection. Empty
    // when not injecting or when the document has no known page.
    let anchors: &[crate::source_map::HeadingAnchor] = if inject_anchors {
        map.page_of(from_slice, from_path)
            .map(|page| map.heading_anchors(page))
            .unwrap_or(&[])
    } else {
        &[]
    };
    let mut anchor_idx = 0usize;

    let mut out = String::with_capacity(source.len() + 64);
    let mut in_fence = false;
    let mut fence_marker = "";
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        // Toggle fenced-code state; fenced lines pass through verbatim.
        if let Some(marker) = fence_open(trimmed) {
            if in_fence {
                if trimmed.starts_with(fence_marker) {
                    in_fence = false;
                    fence_marker = "";
                }
            } else {
                in_fence = true;
                fence_marker = marker;
            }
            out.push_str(line);
            continue;
        }
        if in_fence {
            // Fenced code is emitted verbatim and its links are NOT rewritten (a
            // `](x.md)` in a code sample is not a document link). The one exception
            // is the `CorpusFloor` flatten, which additionally demotes any leading
            // `#`-run so an inlined Turtle/shell comment (`# note`) never surfaces
            // as a spurious H1/section in the flat `llms-full` corpus — harmless,
            // since `#`-to-end-of-line comment syntaxes stay comments at any depth.
            if demote == HeadingDemotion::CorpusFloor {
                out.push_str(&demote_heading_line(line, demote, from_slice, from_path)?);
            } else {
                out.push_str(line);
            }
            continue;
        }
        // Before an ATX heading (in source order), emit its explicit HTML anchor so
        // the resolved `#slug` cross-links have a matching `id` on the page.
        if inject_anchors && atx_level(line).is_some() {
            if let Some(anchor) = anchors.get(anchor_idx) {
                out.push_str(&format!("<a id=\"{}\"></a>\n\n", anchor.slug));
            }
            anchor_idx += 1;
        }
        // Heading demotion is applied to the raw line before link rewriting; the
        // added `#`s never affect link scanning.
        let demoted = demote_heading_line(line, demote, from_slice, from_path)?;
        rewrite_doc_line(&demoted, map, from_slice, from_path, page_dir, &mut out)?;
    }
    Ok(out)
}

/// The heading transform [`rewrite_doc_body`] applies to a re-emitted document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeadingDemotion {
    /// Keep every heading level (a child page: its authored H1 is the page H1).
    Keep,
    /// Demote one level (H1→H2 … H5→H6) so a grafted `docs.md` sits under the
    /// generated slice H1. A source H6 is the H6-overflow guard: H6→H7 is illegal,
    /// so it is a hard failure naming the document.
    GraftOne,
    /// Floor headings two levels down, clamped at H6, so the inlined `llms-full`
    /// document corpus sits below both the single `# ` document H1 and the
    /// `## Documents` section header (every heading becomes H3+, never a bare `## `
    /// that the flat llmstxt structure would read as a new section). Clamps rather
    /// than overflowing — the corpus is a lossy flattened surface, not the graft.
    CorpusFloor,
}

/// Apply the [`HeadingDemotion`] transform to one line, preserving leading
/// whitespace and the trailing newline. Non-heading lines are returned unchanged.
fn demote_heading_line(
    line: &str,
    demote: HeadingDemotion,
    from_slice: &str,
    from_path: &str,
) -> Result<String> {
    if demote == HeadingDemotion::Keep {
        return Ok(line.to_string());
    }
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return Ok(line.to_string());
    }
    let after = &rest[hashes..];
    // A heading needs whitespace (or end of line) after the hash run; otherwise
    // it is not an ATX heading (e.g. `#hashtag`) and must not be demoted.
    if !after.is_empty() && !after.starts_with([' ', '\t', '\n']) {
        return Ok(line.to_string());
    }
    let extra = match demote {
        HeadingDemotion::Keep => unreachable!("handled above"),
        HeadingDemotion::GraftOne => {
            if hashes == 6 {
                return Err(Diag::of_kind(crate::render_error::HeadingOverflow::new(
                    format!(
                        "markdown source `{from_path}` in slice {from_slice} carries a level-6 heading \
                     that cannot be demoted (H6→H7 is illegal): {:?}",
                        line.trim_end()
                    ),
                )));
            }
            1
        }
        // Floor at H6: every heading lands at level+2 but never past 6.
        HeadingDemotion::CorpusFloor => (6 - hashes).min(2),
    };
    let prefix = "#".repeat(hashes + extra);
    Ok(format!("{indent}{prefix}{after}"))
}

/// Rewrite the link targets on a single (non-fenced) source line, appending the
/// result to `out`. Scans for inline-link / image destinations `](target)`,
/// skipping inline code spans so a `](…)` inside backticks is left verbatim.
fn rewrite_doc_line(
    line: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
    out: &mut String,
) -> Result<()> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut span_start = 0usize;
    while i < bytes.len() {
        // Skip an inline code span: a backtick run of length N is closed by the
        // next run of exactly N backticks (CommonMark). Everything between is
        // literal and never rewritten.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let close_from = i + run;
            if let Some(rel_close) = find_backtick_run(&bytes[close_from..], run) {
                i = close_from + rel_close + run;
                continue;
            }
            // No closing run on this line: the rest is not a code span, keep scanning.
            i += run;
            continue;
        }
        // An inline link/image destination opens at `](`.
        if bytes[i] == b']' && i + 1 < bytes.len() && bytes[i + 1] == b'(' {
            let open = i + 2;
            // Destinations in authored slice markdown never contain a raw `)`.
            if let Some(rel_close) = line[open..].find(')') {
                let close = open + rel_close;
                out.push_str(&line[span_start..open]);
                out.push_str(&rewrite_doc_target(
                    &line[open..close],
                    map,
                    from_slice,
                    from_path,
                    page_dir,
                )?);
                span_start = close;
                i = close;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&line[span_start..]);
    Ok(())
}

/// The ATX heading level (1–6) of a non-fenced line, or `None` when it is not an
/// ATX heading. Mirrors the `source_map` heading detector so this walk's heading
/// order aligns with the map's page-scoped anchor order.
fn atx_level(line: &str) -> Option<u8> {
    let rest = line.trim_start();
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if after.is_empty() || after.starts_with([' ', '\t', '\n']) {
        Some(hashes as u8)
    } else {
        None
    }
}

/// The fence marker (` ``` ` or `~~~`) a trimmed line opens/closes a fenced code
/// block with (three or more of one marker char), else `None`.
/// Find the byte offset of the next run of EXACTLY `run` backticks in `bytes`,
/// else `None`. Used to skip inline code spans in [`rewrite_doc_line`].
fn find_backtick_run(bytes: &[u8], run: usize) -> Option<usize> {
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let here = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            if here == run {
                return Some(i);
            }
            i += here;
        } else {
            i += 1;
        }
    }
    None
}

/// Rewrite a single authored link/image destination (the text between `](` and
/// `)`), routing every internal reference through the single [`SourceToPageMap`]
/// authority.
///
/// * Absolute / scheme-qualified / `mailto:` / site-absolute targets are external
///   and pass through verbatim.
/// * A pure `#fragment` addresses a heading in THIS document; a missing anchor is
///   a hard failure (a broken same-document link must not render silently).
/// * A relative `*.md` reference that stays WITHIN the slice is resolved to its
///   generated page (relative href, with any `#anchor`); a dangling within-slice
///   reference is a hard failure the slice's markdown must fix.
/// * A reference that escapes the slice root (another slice, a repo `docs/` file)
///   or targets a non-markdown asset is off-corpus and is absolutized to the
///   published site, so the rendered page never dangles.
fn rewrite_doc_target(
    target: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
) -> Result<String> {
    if target.is_empty() {
        return Ok(target.to_string());
    }
    // The path portion (before any `#fragment`) classifies the target; the resolver
    // re-splits and validates the fragment itself.
    let path_part = target.split_once('#').map_or(target, |(p, _)| p);

    // A pure fragment: a heading in this same document.
    if path_part.is_empty() {
        return match map.resolve_link(from_slice, from_path, target) {
            LinkResolution::Resolved(loc) => Ok(match loc.anchor {
                Some(anchor) => format!("#{anchor}"),
                // A bare `#` with no anchor — leave as authored.
                None => target.to_string(),
            }),
            LinkResolution::Dangling { .. } => Err(Diag::of_kind(
                crate::render_error::DanglingLink::new(format!(
                    "markdown source `{from_path}` in slice {from_slice} has a dangling same-document \
                 anchor link `{target}` (no such heading)"
                )),
            )),
        };
    }

    // Everything with a path portion is CLASSIFIED by the single authority
    // (`SourceToPageMap::classify_doc_link`) — the site renderer never re-derives
    // "internal vs. off-corpus vs. external"; it only formats the classification for
    // its own (site-relative) output.
    match map.classify_doc_link(from_slice, from_path, target) {
        DocLinkResolution::External => Ok(target.to_string()),
        DocLinkResolution::OffCorpus => Ok(absolutize_offsite(target)),
        DocLinkResolution::Corpus(loc) => {
            let target_dir = loc.page.strip_suffix('/').unwrap_or(&loc.page);
            let href = rel(page_dir, target_dir);
            Ok(match loc.anchor {
                Some(anchor) => format!("{href}index.md#{anchor}"),
                None => format!("{href}index.md"),
            })
        }
        DocLinkResolution::Dangling { .. } => Err(Diag::of_kind(
            crate::render_error::DanglingLink::new(format!(
                "markdown source `{from_path}` in slice {from_slice} has a dangling internal document \
             link `{target}` — fix the link in the slice's markdown (it names no document in the \
             slice)"
            )),
        )),
    }
}

/// Absolutize an off-corpus relative reference to the published documentation site,
/// so a rendered page never carries a dangling relative link into a document this
/// render does not emit. Leading `./` / `../` segments are dropped and the
/// remainder is appended to [`crate::mdbook::PUBLISHED_SITE_BASE`] — the SAME
/// published-site base the mdbook renderer externalizes dropped surfaces to.
fn absolutize_offsite(target: &str) -> String {
    let mut rest = target;
    loop {
        if let Some(r) = rest.strip_prefix("../") {
            rest = r;
        } else if let Some(r) = rest.strip_prefix("./") {
            rest = r;
        } else {
            break;
        }
    }
    format!("{}{rest}", crate::mdbook::PUBLISHED_SITE_BASE)
}

// ── Linkage / example / concern / external / integrity pages ──────────────────

pub(super) fn md_linkage_index(model: &DocsModel) -> String {
    let from = Page::LinkageIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_linkages"));
    line(
        &mut out,
        &format!(
            "**{}** term equivalences across **{}** mapping set(s), cross-walking GMEOW terms to \
             external vocabularies.",
            model.linkages.len(),
            model.mapping_sets.len()
        ),
    );

    for set in &model.mapping_sets {
        heading(&mut out, 2, &set_display(set));
        push_line(&mut out, "| Field | Value |");
        push_line(&mut out, "| --- | --- |");
        push_line(
            &mut out,
            &format!("| CURIE | `{}` |", code_escape(&set.curie)),
        );
        if let Some(id) = &set.set_id {
            push_line(&mut out, &format!("| Set ID | `{}` |", code_escape(id)));
        }
        if let Some(file) = &set.sssom_file {
            push_line(
                &mut out,
                &format!("| SSSOM file | `{}` |", code_escape(file)),
            );
        }
        if let Some(license) = &set.license {
            push_line(
                &mut out,
                &format!(
                    "| License | [{}]({}) |",
                    md_escape(license),
                    md_escape(license)
                ),
            );
        }
        push_line(
            &mut out,
            &format!(
                "| Owner slice | {} |",
                slice_link(model, &from, &set.owner_slice)
            ),
        );
        push_line(
            &mut out,
            &format!("| Equivalences | {} |", set.equivalence_count),
        );
        blank(&mut out);

        if let Some(comment) = &set.comment {
            line(&mut out, &md_escape(&one_line(comment)));
        }

        let mut links: Vec<&crate::model::DocLinkage> = model
            .linkages
            .iter()
            .filter(|l| l.mapping_set.as_deref() == Some(set.iri.as_str()))
            .collect();
        links.sort_by(|a, b| {
            a.subject_curie
                .cmp(&b.subject_curie)
                .then_with(|| a.object.cmp(&b.object))
        });
        if !links.is_empty() {
            push_line(
                &mut out,
                "| Subject | Predicate | External object | Justification | Conf. |",
            );
            push_line(&mut out, "| --- | --- | --- | --- | --- |");
            for link in links {
                push_line(
                    &mut out,
                    &format!(
                        "| {} | `{}` | [{}]({}) | {} | {} |",
                        subject_link(model, &from, link),
                        code_escape(&link.predicate),
                        md_escape(&link.object),
                        md_escape(&link.object),
                        link.justification
                            .as_deref()
                            .map(|j| format!("`{}`", code_escape(j)))
                            .unwrap_or_default(),
                        confidence_cell(link.confidence),
                    ),
                );
            }
            blank(&mut out);
        }
    }

    // Any linkages not attached to a known mapping set (defensive completeness).
    let mut orphans: Vec<&crate::model::DocLinkage> = model
        .linkages
        .iter()
        .filter(|l| {
            l.mapping_set
                .as_deref()
                .map(|m| !model.mapping_sets.iter().any(|s| s.iri == m))
                .unwrap_or(true)
        })
        .collect();
    orphans.sort_by(|a, b| {
        a.subject_curie
            .cmp(&b.subject_curie)
            .then_with(|| a.object.cmp(&b.object))
    });
    if !orphans.is_empty() {
        heading(&mut out, 2, model.ui("body_other_equivalences"));
        push_line(
            &mut out,
            "| Subject | Predicate | External object | Conf. |",
        );
        push_line(&mut out, "| --- | --- | --- | --- |");
        for link in orphans {
            push_line(
                &mut out,
                &format!(
                    "| {} | `{}` | [{}]({}) | {} |",
                    subject_link(model, &from, link),
                    code_escape(&link.predicate),
                    md_escape(&link.object),
                    md_escape(&link.object),
                    confidence_cell(link.confidence),
                ),
            );
        }
        blank(&mut out);
    }

    out
}

pub(super) fn md_example_index(model: &DocsModel) -> String {
    let from = Page::ExampleIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_examples"));
    line(
        &mut out,
        &format!(
            "**{}** worked example(s). Each example's Turtle source is shown in full on its \
             owning slice page.",
            model.examples.len()
        ),
    );

    // Group examples by slice (model.examples is slice/path-sorted).
    let mut by_slice: BTreeMap<String, Vec<&crate::model::DocExample>> = BTreeMap::new();
    for example in &model.examples {
        by_slice
            .entry(example.slice.clone())
            .or_default()
            .push(example);
    }
    for (slice_iri, mut examples) in by_slice {
        examples.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
        heading(&mut out, 2, &slice_name(model, &slice_iri));
        let slice_href = model
            .slices
            .iter()
            .find(|s| s.iri == slice_iri)
            .map(|s| rel(&from, &Page::Slice(slice_slug(s)).dir()));
        for example in examples {
            match &slice_href {
                Some(href) => push_line(
                    &mut out,
                    &format!(
                        "- [{}]({}index.md) — `{}`",
                        md_escape(&example.title),
                        href,
                        code_escape(&example.logical_path)
                    ),
                ),
                None => push_line(
                    &mut out,
                    &format!(
                        "- {} — `{}`",
                        md_escape(&example.title),
                        code_escape(&example.logical_path)
                    ),
                ),
            }
        }
        blank(&mut out);
    }

    // ── Worked math instances (ℚ⁷ SI-dimension worked examples) ─────────────
    // Distinct from the example listing above: every example subject (in any
    // slice) carrying `math:hasDimension`, resolved generically by
    // `gmeow_docs::model::extract_worked_instances` — not special-cased to
    // `measure-and-dimension.ttl` (today's only author). Placed on THIS page
    // (rather than a new dedicated `Page` variant) because a worked instance
    // IS a worked example — the same `examples/*.ttl` scan the loss-row
    // section reuses, and `Page::ExampleIndex` is already the page readers
    // reach for "show me a concrete instance", so it stays their next stop
    // rather than a fourth example-adjacent page.
    heading(&mut out, 2, model.ui("body_worked_instances"));
    line(
        &mut out,
        "Concrete quantities/measures/functions carrying `math:hasDimension`, resolved to their \
         ℚ⁷ SI base-dimension exponent vector (mass · length · time · electric current · \
         temperature · amount of substance · luminous intensity) when the dimension object \
         breaks one down — a dimensionless subject (e.g. `math:dimensionless`) honestly renders \
         with no exponent rows, not a fabricated breakdown.",
    );
    if model.worked_instances.is_empty() {
        line(&mut out, model.ui("body_no_worked_instances"));
    } else {
        for instance in &model.worked_instances {
            let type_suffix = if instance.types.is_empty() {
                String::new()
            } else {
                format!(" ({})", instance.types.join(", "))
            };
            heading(
                &mut out,
                3,
                &format!(
                    "{}{}",
                    md_escape(&instance.subject),
                    md_escape(&type_suffix)
                ),
            );
            line(
                &mut out,
                &format!(
                    "`{}` — `{}`",
                    code_escape(&slice_name(model, &instance.slice)),
                    code_escape(&instance.logical_path)
                ),
            );
            if let Some(label) = &instance.label {
                line(&mut out, &md_escape(label));
            }
            if let Some(dimension_label) = &instance.dimension_label {
                line(
                    &mut out,
                    &format!("Dimension: {}", md_escape(dimension_label)),
                );
            }
            if instance.dimension_exponents.is_empty() {
                line(
                    &mut out,
                    "Dimensionless — no base-dimension exponent breakdown.",
                );
            } else {
                push_line(&mut out, "| Base dimension | Numerator | Denominator |");
                push_line(&mut out, "| --- | --- | --- |");
                for exponent in &instance.dimension_exponents {
                    push_line(
                        &mut out,
                        &format!(
                            "| `{}` | {} | {} |",
                            code_escape(&exponent.base_dimension),
                            exponent.numerator,
                            exponent.denominator
                        ),
                    );
                }
                blank(&mut out);
            }
            fenced(&mut out, "turtle", &instance.turtle);
        }
    }

    out
}

/// The filename stem of a fixture's slice-relative logical path (no directory,
/// no `.ttl` extension) — the basis for the best-effort Do/Don't pairing.
fn fixture_stem(logical_path: &str) -> &str {
    logical_path
        .rsplit('/')
        .next()
        .unwrap_or(logical_path)
        .trim_end_matches(".ttl")
}

/// The number of leading `-`-separated tokens two fixture stems share (e.g.
/// `plan-wellformed` / `plan-missing-successmode` share 1: `plan`). Used to
/// pick, for a counter-example, the best-matching well-formed fixture in the
/// same slice — the repo's fixture-naming convention puts the shared concept
/// stem first and the wellformed/violating descriptor last.
fn shared_prefix_tokens(a: &str, b: &str) -> usize {
    a.split('-')
        .zip(b.split('-'))
        .take_while(|(x, y)| x == y)
        .count()
}

/// Render a fixture's expected outcome as a Markdown link into the constraint
/// catalog when [`DocFixture::catalog_slug`] resolved a genuine match, else a
/// plain code span (never a dead link).
fn fixture_violation_code_display(
    from: &str,
    fixture: &crate::model::DocFixture,
) -> Option<String> {
    let code = fixture.violation_code.as_ref()?;
    Some(match &fixture.catalog_slug {
        Some(slug) => {
            let catalog_href = rel(from, &Page::ConstraintCatalog.dir());
            format!("[`{}`]({catalog_href}index.md#{slug})", code_escape(code))
        }
        None => format!("`{}`", code_escape(code)),
    })
}

/// Push the Do/Don't detail bullets shared by the fixture index and the
/// per-term conformance-examples section: the violation code (catalog-linked
/// when resolved) and the rationale, when the fixture carries a binding.
pub(super) fn push_fixture_binding_bullets(
    out: &mut String,
    model: &DocsModel,
    from: &str,
    fixture: &crate::model::DocFixture,
) {
    if let Some(code_display) = fixture_violation_code_display(from, fixture) {
        push_line(
            out,
            &format!(
                "  - **{}:** {code_display}",
                model.ui("body_label_violation_code")
            ),
        );
    }
    for rationale in &fixture.rationales {
        push_line(
            out,
            &format!("  - {}", md_escape(&one_line(&rationale.lexical_form))),
        );
    }
}

/// The conformance Do/Don't fixtures index, grouped by slice. Each
/// counter-example is paired with its best-matching well-formed fixture in the
/// same slice (by shared filename-stem prefix tokens) when one exists; a
/// well-formed fixture never picked as a pair partner is listed once on its
/// own so none are lost.
pub(super) fn md_fixture_index(model: &DocsModel) -> String {
    let from = Page::FixtureIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_conformance_fixtures"));
    line(
        &mut out,
        &format!(
            "**{}** conformance fixture(s) — well-formed instances and deliberately \
             malformed counter-examples used to pin per-slice validation behaviour. Each \
             counter-example isolates ONE violation; its \"{}\" entry names the expected \
             `sh:*ConstraintComponent` code and rationale from the owning slice's \
             `tests/example-conformance.ttl` binding, when one is authored.",
            model.fixtures.len(),
            model.ui("body_label_dont"),
        ),
    );
    if model.fixtures.is_empty() {
        line(&mut out, model.ui("body_no_conformance_fixtures"));
        return out;
    }

    let mut by_slice: BTreeMap<&str, Vec<&crate::model::DocFixture>> = BTreeMap::new();
    for fixture in &model.fixtures {
        by_slice
            .entry(fixture.slice.as_str())
            .or_default()
            .push(fixture);
    }

    for (slice_iri, fixtures) in by_slice {
        heading(&mut out, 2, &slice_name(model, slice_iri));

        let wellformed: Vec<&crate::model::DocFixture> = fixtures
            .iter()
            .copied()
            .filter(|f| f.kind == crate::model::DocFixtureKind::Wellformed)
            .collect();
        let counter_examples: Vec<&crate::model::DocFixture> = fixtures
            .iter()
            .copied()
            .filter(|f| f.kind == crate::model::DocFixtureKind::CounterExample)
            .collect();

        let mut paired_wellformed: std::collections::BTreeSet<&str> =
            std::collections::BTreeSet::new();

        for ce in &counter_examples {
            let ce_stem = fixture_stem(&ce.logical_path);
            let best = wellformed
                .iter()
                .copied()
                .filter_map(|wf| {
                    let n = shared_prefix_tokens(fixture_stem(&wf.logical_path), ce_stem);
                    (n > 0).then_some((n, wf))
                })
                .max_by(|(n1, wf1), (n2, wf2)| {
                    n1.cmp(n2)
                        .then_with(|| wf2.logical_path.cmp(&wf1.logical_path))
                });

            heading(&mut out, 3, &ce.title);
            if let Some((_, wf)) = best {
                paired_wellformed.insert(wf.logical_path.as_str());
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:** `{}`",
                        model.ui("body_label_do"),
                        code_escape(&wf.logical_path)
                    ),
                );
            }
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_label_dont"),
                    code_escape(&ce.logical_path)
                ),
            );
            push_fixture_binding_bullets(&mut out, model, &from, ce);
            blank(&mut out);
        }

        // Well-formed fixtures never picked as a pair partner (e.g. a standalone
        // flagship-scenario fixture with no counter-example twin) — listed once
        // so none are silently dropped from the index.
        for wf in &wellformed {
            if paired_wellformed.contains(wf.logical_path.as_str()) {
                continue;
            }
            heading(&mut out, 3, &wf.title);
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_label_do"),
                    code_escape(&wf.logical_path)
                ),
            );
            push_fixture_binding_bullets(&mut out, model, &from, wf);
            blank(&mut out);
        }
    }
    out
}

/// The competency-questions index, grouped by slice. Each question renders its
/// rationale, the terms it exercises (linked), the full copy-paste-runnable
/// SPARQL query, and its expected result — either an enumerated row table or a
/// pinned row count.
pub(super) fn md_competency_index(model: &DocsModel) -> String {
    let from = Page::CompetencyIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_competency_questions"));
    line(
        &mut out,
        &format!(
            "**{}** competency question(s) — declarative SPARQL questions the ontology must \
             answer, each pinning an expected result over the asserted merged ontology (or its \
             RDFS/native-reasoned closure, when the question opts in).",
            model.competencies.len(),
        ),
    );
    if model.competencies.is_empty() {
        line(&mut out, model.ui("body_no_competency_questions"));
        return out;
    }

    let mut by_slice: BTreeMap<&str, Vec<&crate::model::DocCompetency>> = BTreeMap::new();
    for cq in &model.competencies {
        by_slice
            .entry(cq.owner_slice.as_str())
            .or_default()
            .push(cq);
    }

    for (slice_iri, cqs) in by_slice {
        heading(&mut out, 2, &slice_name(model, slice_iri));
        for cq in cqs {
            heading(&mut out, 3, local_name(&cq.iri));

            for rationale in &cq.rationales {
                let text = md_escape(&one_line(&rationale.lexical_form));
                match &cq.query_file {
                    Some(qf) => push_line(&mut out, &format!("- {text} (`{}`)", code_escape(qf))),
                    None => push_line(&mut out, &format!("- {text}")),
                }
            }

            if !cq.exercises.is_empty() {
                let links: Vec<String> = cq
                    .exercises
                    .iter()
                    .map(|t| term_link(model, &from, t))
                    .collect();
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:** {}",
                        model.ui("body_terms_used"),
                        links.join(", ")
                    ),
                );
            }

            match (cq.exact_rows, cq.expected_row_count) {
                (Some(true), _) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** exactly {} row(s) (closed set)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (Some(false), _) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** at least {} row(s) (subset)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (None, Some(n)) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** exactly {n} row(s)",
                        model.ui("body_expected_rows")
                    ),
                ),
                (None, None) if !cq.expected_rows.is_empty() => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** {} row(s)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (None, None) => {}
            }
            blank(&mut out);

            push_line(&mut out, &format!("**{}:**", model.ui("body_query")));
            blank(&mut out);
            match &cq.query_text {
                Some(text) => fenced(&mut out, "sparql", text),
                None => line(&mut out, model.ui("body_no_query_text")),
            }

            if !cq.expected_rows.is_empty() {
                let mut vars: Vec<&str> = cq
                    .expected_rows
                    .iter()
                    .flat_map(|row| row.cells.iter().filter_map(|c| c.var.as_deref()))
                    .collect();
                vars.sort();
                vars.dedup();
                if !vars.is_empty() {
                    push_line(&mut out, &format!("| {} |", vars.join(" | ")));
                    push_line(
                        &mut out,
                        &format!(
                            "| {} |",
                            vars.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
                        ),
                    );
                    for row in &cq.expected_rows {
                        let cells: Vec<String> = vars
                            .iter()
                            .map(|v| {
                                let Some(cell) =
                                    row.cells.iter().find(|c| c.var.as_deref() == Some(*v))
                                else {
                                    return String::new();
                                };
                                if let Some(iri) = &cell.value_iri {
                                    term_link(model, &from, iri)
                                } else if let Some(lit) = &cell.value_literal {
                                    format!("`{}`", code_escape(lit))
                                } else {
                                    String::new()
                                }
                            })
                            .collect();
                        push_line(&mut out, &format!("| {} |", cells.join(" | ")));
                    }
                    blank(&mut out);
                }
            }
        }
    }
    out
}

/// The notation-grammars index — every first-class `lang:Grammar` rendering
/// (GMN / GTS / Turtle) authored as plain W3C EBNF text under
/// `slices/grounding/lang/grammars/*.ebnf`, listed with its title and
/// license, each linking to the full source on its own [`Page::Grammar`] page.
pub(super) fn md_notation_index(model: &DocsModel) -> String {
    let from = Page::NotationIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_notation_grammars"));
    line(
        &mut out,
        &format!(
            "**{}** notation grammar(s) — first-class W3C EBNF renderings of the sign systems \
             this project's own serialization notations use (the GMN compact record notation, \
             the GTS textual surface, and the RDF 1.1 Turtle grammar the native codec \
             interprets). Each is a RENDERING of the corresponding `lang:Grammar` object, never \
             a second parser: the grammar object itself carries the normative claims.",
            model.grammars.len()
        ),
    );
    if model.grammars.is_empty() {
        line(&mut out, model.ui("body_no_notation_grammars"));
        return out;
    }
    push_line(&mut out, "| Grammar | License |");
    push_line(&mut out, "| --- | --- |");
    for grammar in &model.grammars {
        let href = rel(&from, &Page::Grammar(grammar.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | `{}` |",
                md_escape(&grammar.title),
                href,
                code_escape(&grammar.license),
            ),
        );
    }
    blank(&mut out);
    out
}

/// A single notation-grammar detail page: title, slug, license, and the full
/// W3C EBNF source in a fenced code block.
pub(super) fn md_grammar(model: &DocsModel, slug: &str) -> String {
    let Some(grammar) = model.grammars.iter().find(|g| g.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_grammar_not_found"));
        return out;
    };
    let mut out = String::new();
    heading(&mut out, 1, &grammar.title);
    line(
        &mut out,
        &format!(
            "`{}` · notation grammar · License: `{}`",
            code_escape(&grammar.slug),
            code_escape(&grammar.license)
        ),
    );
    heading(&mut out, 2, model.ui("body_grammar_source"));
    fenced(&mut out, "ebnf", &grammar.source);
    out
}

/// The build-pipeline DAG page: the deterministic SVG plus a stage table (impl,
/// capabilities, resources, consumes count) and the plan's goal + success mode.
/// A pure render of `model.pipeline` (the source-lane build graph).
pub(super) fn md_pipeline_dag(model: &DocsModel) -> String {
    let from = Page::PipelineDag.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_build_pipeline"));
    let Some(pipeline) = &model.pipeline else {
        line(&mut out, model.ui("body_no_pipeline"));
        return out;
    };
    line(
        &mut out,
        &format!(
            "The dogfooded GMEOW build DAG, authored as data in \
             `slices/core/pipeline/module.ttl` and read back by the `gmeow-pipeline` \
             executor: **{}** `gmeow:PipelineStage` node(s) wired by **{}** dataflow \
             edge(s). Exactly one stage holds `gmeow:sinkCapability` — the single \
             serialization exit (the gts narrow waist), highlighted in the diagram.",
            pipeline.stages.len(),
            pipeline.edges.len(),
        ),
    );

    // The plan facets (a `gmeow:Pipeline` IS a `logic:Plan`).
    if pipeline.goal.is_some() || pipeline.success_mode.is_some() {
        heading(&mut out, 2, model.ui("body_goal"));
        if let Some(goal) = &pipeline.goal {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** {}",
                    model.ui("body_goal"),
                    curie_link(model, &from, goal)
                ),
            );
        }
        if let Some(mode) = &pipeline.success_mode {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_pipeline_success_mode"),
                    code_escape(mode)
                ),
            );
        }
        blank(&mut out);
    }

    // The deterministic DAG SVG (emitted in `render_site_lang_exec`).
    heading(&mut out, 2, model.ui("body_pipeline_diagram"));
    push_line(
        &mut out,
        &format!(
            "![Build pipeline DAG]({}diagrams/pipeline.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    // The stage table.
    heading(&mut out, 2, model.ui("body_pipeline_stages"));
    push_line(
        &mut out,
        &format!(
            "| Stage | {} | {} | {} | {} |",
            model.ui("body_pipeline_implementation"),
            model.ui("body_pipeline_capabilities"),
            model.ui("body_pipeline_consumes"),
            model.ui("body_box_role"),
        ),
    );
    push_line(&mut out, "| --- | --- | --- | --- | --- |");
    for stage in &pipeline.stages {
        let name = curie_link(model, &from, &to_curie(&stage.iri));
        let impl_cell = match &stage.stage_impl {
            Some(module) => format!("`crates/pipeline/src/stages/{}.rs`", code_escape(module)),
            None => "—".to_string(),
        };
        let caps = pipeline_curie_cell(&stage.capabilities, &stage.resources);
        let box_role = stage
            .box_role
            .as_deref()
            .map(|r| format!("`{}`", code_escape(r)))
            .unwrap_or_else(|| "—".to_string());
        push_line(
            &mut out,
            &format!(
                "| {} | {} | {} | {} | {} |",
                name,
                impl_cell,
                caps,
                stage.consumes.len(),
                box_role,
            ),
        );
    }
    blank(&mut out);
    out
}

/// Render a stage's capability + resource CURIEs into one compact table cell
/// (each a code span), or an em-dash when the stage is a plain transform leaf.
fn pipeline_curie_cell(capabilities: &[String], resources: &[String]) -> String {
    let all: Vec<String> = capabilities
        .iter()
        .chain(resources.iter())
        .map(|c| format!("`{}`", code_escape(c)))
        .collect();
    if all.is_empty() {
        "—".to_string()
    } else {
        all.join(" ")
    }
}

/// Append the enriched build-pipeline stage section to a term page when the term
/// IS a `gmeow:PipelineStage` (its IRI matches a stage in `model.pipeline`). Renders
/// the Rust module binding, the consumes / consumed-by dataflow tables (from
/// `model.pipeline.edges`), the flowing named graphs (reified `gmeow:flowEntity`
/// where authored — honest absence otherwise), and the capabilities/resources.
///
/// No per-stage gate-verdict chip is rendered: `gmeow_errors::grade::GateVerdict`
/// is finding-scoped, and no genuine substrate attributes findings to one of the
/// 36 pipeline STAGE IRIs, so a per-stage verdict would be fabricated. Per the
/// proof-carrying doctrine, the honest static facts are rendered and the chip is
/// omitted (an honest computed-absence, not a scope gap).
pub(super) fn append_stage_section(
    out: &mut String,
    model: &DocsModel,
    term: &DocTerm,
    from: &str,
) {
    let Some(pipeline) = &model.pipeline else {
        return;
    };
    let Some(stage) = pipeline.stages.iter().find(|s| s.iri == term.iri) else {
        return;
    };

    heading(out, 2, model.ui("body_pipeline_stage"));
    line(
        out,
        &format!(
            "This term is a stage of the [build pipeline]({}index.md) — a typed unit of \
             build work the `gmeow-pipeline` executor runs single-pass over the in-memory \
             dataset.",
            rel(from, &Page::PipelineDag.dir()),
        ),
    );

    if let Some(module) = &stage.stage_impl {
        push_line(
            out,
            &format!(
                "- **{}:** `crates/pipeline/src/stages/{}.rs`",
                model.ui("body_pipeline_implementation"),
                code_escape(module)
            ),
        );
    }
    if !stage.capabilities.is_empty() || !stage.resources.is_empty() {
        push_line(
            out,
            &format!(
                "- **{}:** {}",
                model.ui("body_pipeline_capabilities"),
                pipeline_curie_cell(&stage.capabilities, &stage.resources)
            ),
        );
    }
    if let Some(role) = &stage.box_role {
        push_line(
            out,
            &format!(
                "- **{}:** `{}`",
                model.ui("body_box_role"),
                code_escape(role)
            ),
        );
    }
    blank(out);

    // Consumes: the upstream producer stages this stage reads (each a term page).
    if !stage.consumes.is_empty() {
        heading(out, 3, model.ui("body_pipeline_consumes"));
        for producer in &stage.consumes {
            push_line(
                out,
                &format!("- {}", curie_link(model, from, &to_curie(producer))),
            );
        }
        blank(out);
    }

    // Attaches: the named graphs / blob-rep lanes this stage contributes to the carrier
    // as its delta (gmeow:attachesGraph / gmeow:attachesBlobRep) — the run-verified
    // declaration of what the stage produced, so `gmeow docs-on <stage>` self-explains.
    if !stage.attaches_graphs.is_empty() || !stage.attaches_blob_reps.is_empty() {
        heading(out, 3, model.ui("body_pipeline_attaches"));
        for graph in &stage.attaches_graphs {
            push_line(out, &format!("- `{}`", code_escape(graph)));
        }
        for rep in &stage.attaches_blob_reps {
            push_line(
                out,
                &format!(
                    "- {} `{}`",
                    model.ui("body_pipeline_attaches_blob"),
                    code_escape(rep)
                ),
            );
        }
        blank(out);
    }

    // Consumed by: the downstream stages that read THIS stage's product (the edge
    // reverse — `edges.from == this stage`).
    let mut consumed_by: Vec<&str> = pipeline
        .edges
        .iter()
        .filter(|e| e.from == stage.iri)
        .map(|e| e.to.as_str())
        .collect();
    consumed_by.sort_unstable();
    consumed_by.dedup();
    if !consumed_by.is_empty() {
        heading(out, 3, model.ui("body_pipeline_consumed_by"));
        for consumer in consumed_by {
            push_line(
                out,
                &format!("- {}", curie_link(model, from, &to_curie(consumer))),
            );
        }
        blank(out);
    }

    // Flowing graphs: the reified `gmeow:flowEntity` named graphs on any edge
    // touching this stage (sorted/deduped). Absent unless a `gmeow:BuildDataFlow`
    // authors them — honest computed-absence, so the heading is emitted only when
    // at least one flowing graph exists.
    let mut flowing: Vec<&str> = pipeline
        .edges
        .iter()
        .filter(|e| e.from == stage.iri || e.to == stage.iri)
        .flat_map(|e| e.flow_entities.iter().map(String::as_str))
        .collect();
    flowing.sort_unstable();
    flowing.dedup();
    if !flowing.is_empty() {
        heading(out, 3, model.ui("body_pipeline_flowing_graphs"));
        for graph in flowing {
            push_line(out, &format!("- `{}`", code_escape(graph)));
        }
        blank(out);
    }
}

/// Append the per-page provenance footer: the producing-stage chain, the
/// build-grain projection of the single `gmeow:docGroundedBy` provenance relation.
/// A no-op when the model carries no pipeline (a bare unit-test model) — honest
/// absence, so the source-model goldens without a pipeline are unaffected.
pub(super) fn append_provenance_footer(out: &mut String, model: &DocsModel) {
    let Some(pipeline) = &model.pipeline else {
        return;
    };
    let chain = provenance_chain(pipeline, "stage-docs-render");
    if chain.is_empty() {
        return;
    }
    let rendered = chain
        .iter()
        .map(|s| format!("`{}`", code_escape(s)))
        .collect::<Vec<_>>()
        .join(" ← ");
    blank(out);
    push_line(out, "---");
    blank(out);
    push_line(
        out,
        &format!(
            "**{}:** this page ← {}",
            model.ui("body_provenance"),
            rendered
        ),
    );
    push_line(
        out,
        "Rendered from `gmeow.gts` by the dogfooded build pipeline.",
    );
    blank(out);
}

pub(super) fn md_concern_index(model: &DocsModel) -> String {
    let from = Page::ConcernIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_concerns"));
    line(
        &mut out,
        &format!(
            "**{}** cross-cutting documentation concern(s). Concerns group vocabulary terms by \
             the design question they answer, across slice boundaries.",
            model.concerns.len()
        ),
    );

    heading(&mut out, 2, model.ui("body_by_term_count"));
    push_line(
        &mut out,
        &format!(
            "![Concerns by term count]({}diagrams/concerns.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    push_line(&mut out, "| Concern | Terms | Slices |");
    push_line(&mut out, "| --- | --- | --- |");
    for concern in &model.concerns {
        let href = rel(&from, &Page::Concern(concern_slug(concern)).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | {} |",
                md_escape(&concern_display(concern)),
                href,
                concern.terms.len(),
                concern.slices.len(),
            ),
        );
    }
    blank(&mut out);
    out
}

pub(super) fn md_concern(model: &DocsModel, slug: &str) -> String {
    let Some(concern) = model.concerns.iter().find(|c| concern_slug(c) == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_concern_not_found"));
        return out;
    };
    let from = Page::Concern(slug.to_string()).dir();

    let mut out = String::new();
    heading(&mut out, 1, &concern_display(concern));
    line(&mut out, &format!("`{}`", code_escape(&concern.curie)));

    if let Some(def) = &concern.definition {
        heading(&mut out, 2, model.ui("body_definition"));
        line(&mut out, &md_escape(def));
    }

    if !concern.terms.is_empty() {
        heading(&mut out, 2, &format!("Terms ({})", concern.terms.len()));
        for curie in &concern.terms {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !concern.slices.is_empty() {
        heading(&mut out, 2, &format!("Slices ({})", concern.slices.len()));
        for slice_iri in &concern.slices {
            push_line(
                &mut out,
                &format!("- {}", slice_link(model, &from, slice_iri)),
            );
        }
        blank(&mut out);
    }
    out
}

pub(super) fn md_external_index(model: &DocsModel) -> String {
    let from = Page::ExternalIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_external_ontologies"));
    line(
        &mut out,
        &format!(
            "**{}** external term(s) referenced across **{}** namespace(s). These are the \
             non-GMEOW IRIs GMEOW terms link to (via mappings) or inherit from / constrain (via \
             domain, range, super-class edges).",
            model.external_terms.len(),
            namespace_count(model),
        ),
    );

    // Group by namespace (deterministic: external_terms is IRI-sorted).
    let mut by_ns: BTreeMap<String, Vec<&crate::model::DocExternalTerm>> = BTreeMap::new();
    for term in &model.external_terms {
        by_ns.entry(term.namespace.clone()).or_default().push(term);
    }
    for (namespace, terms) in by_ns {
        heading(&mut out, 2, &namespace);
        push_line(&mut out, "| External term | Referenced by | Via |");
        push_line(&mut out, "| --- | --- | --- |");
        for term in terms {
            let referencers = term
                .referenced_by
                .iter()
                .map(|c| curie_link(model, &from, c))
                .collect::<Vec<_>>()
                .join(", ");
            push_line(
                &mut out,
                &format!(
                    "| [{}]({}) | {} | {} |",
                    md_escape(&term.iri),
                    md_escape(&term.iri),
                    referencers,
                    md_escape(&term.via_predicate.join(", ")),
                ),
            );
        }
        blank(&mut out);
    }
    out
}

pub(super) fn md_integrity_index(model: &DocsModel) -> String {
    let from = Page::IntegrityIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_integrity_constraints"));
    line(
        &mut out,
        "Per-slice SPARQL verification queries. Each query asserts an integrity constraint the \
         slice's data must satisfy; a non-empty result is a violation.",
    );

    let mut any = false;
    for slice in &model.slices {
        let mut queries: Vec<&crate::model::DocArtifact> = slice
            .artifacts
            .iter()
            .filter(|a| a.role == purrdf::slice::ArtifactRole::VerifyQuery)
            .collect();
        queries.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
        if queries.is_empty() {
            continue;
        }
        any = true;
        heading(&mut out, 2, &slice_display(slice));
        let href = rel(&from, &Page::Slice(slice_slug(slice)).dir());
        line(&mut out, &format!("[Slice page]({href}index.md)"));
        for query in queries {
            heading(&mut out, 3, &query.logical_path);
            // The query text is not carried on DocArtifact (by-reference); note the
            // path and digest so the constraint is locatable in the slice tree.
            push_line(
                &mut out,
                &format!(
                    "- Path: `{}`  ·  digest `{}`",
                    code_escape(&query.logical_path),
                    code_escape(&short_digest(&query.raw_digest)),
                ),
            );
            blank(&mut out);
        }
    }
    if !any {
        line(&mut out, model.ui("body_no_verify_queries"));
    }
    out
}

/// The "What GMEOW enforces" page: the constraint catalog — every
/// `gmeow:ValidationRule` the toolchain enforces — grouped by finding category
/// and, within each category, sorted by rule code. Each rule entry carries an
/// explicit `<a id="{slug}">` anchor whose id is
/// `gmeow_validate::rule_catalog::slugify(code)` — the identical slug the
/// validator stamps into a finding's `helpUri` fragment — so a finding's help
/// link deep-links straight to its rule. Deterministic: categories, rules, and
/// applies-to terms are all sorted.
pub(super) fn md_constraint_catalog(model: &DocsModel) -> String {
    let from = Page::ConstraintCatalog.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_enforced_constraints"));
    line(
        &mut out,
        &format!(
            "The **{}** validation rules the GMEOW toolchain enforces, grouped by finding \
             category. Each rule is a `gmeow:ValidationRule` individual from the constraint \
             catalog; a finding's `helpUri` deep-links to its entry here. **Binding** rules fail \
             the gate; **advisory** rules report without failing. The distinct **{}** section \
             below documents the non-gating *recommendations* (`avoidWhen` / `useWhen` / \
             `howToUse`) that advisory `advice.*` findings resolve to.",
            model.constraint_rules.len(),
            model.ui("body_usage_advice"),
        ),
    );

    if model.constraint_rules.is_empty() {
        line(&mut out, model.ui("body_no_enforced_constraints"));
        return out;
    }

    // Group by category IRI (rules are already sorted by code); the category map
    // is a BTreeMap so category headings emit in sorted IRI order. The `advice.`
    // family rule is EXCLUDED here — it heads the distinct Advice section below and
    // carries the single `#advice-` anchor, which must appear exactly once.
    let mut by_category: std::collections::BTreeMap<&str, Vec<&crate::model::ConstraintRule>> =
        std::collections::BTreeMap::new();
    for rule in &model.constraint_rules {
        if rule.code == gmeow_validate::codes::ADVICE_FAMILY {
            continue;
        }
        by_category
            .entry(rule.category.as_str())
            .or_default()
            .push(rule);
    }

    for (category, rules) in &by_category {
        heading(&mut out, 2, &finding_category_display(category));
        for rule in rules {
            // Explicit anchor so the finding helpUri fragment (`#{slug}`) resolves
            // regardless of the Markdown heading-id derivation. The slug is
            // `[a-z0-9-]` only (the validator's transform), so it is inert inside the
            // HTML `id` attribute — never md-escape it, or the id would gain
            // backslashes and stop matching the helpUri fragment.
            push_line(&mut out, &format!("<a id=\"{}\"></a>", rule.slug));
            blank(&mut out);
            heading(&mut out, 3, &rule.code);

            let severity = if rule.severity.is_empty() {
                "—".to_string()
            } else {
                rule.severity.clone()
            };
            push_line(
                &mut out,
                &format!(
                    "- {}: **{}**",
                    model.ui("body_label_severity"),
                    md_escape(&severity)
                ),
            );
            push_line(
                &mut out,
                &format!(
                    "- {}: `{}`",
                    model.ui("body_label_rule_code"),
                    code_escape(&rule.code)
                ),
            );
            if !rule.help_uri.is_empty() {
                // The display text is md-escaped; the link *target* is the raw URL
                // (escaping a target corrupts the href).
                push_line(
                    &mut out,
                    &format!(
                        "- {}: [{}]({})",
                        model.ui("body_label_help_link"),
                        md_escape(&rule.help_uri),
                        rule.help_uri,
                    ),
                );
            }
            if let Some(formalizes) = &rule.formalizes {
                push_line(
                    &mut out,
                    &format!(
                        "- {}: {}",
                        model.ui("body_formalized_by"),
                        curie_link(model, &from, &to_curie(formalizes)),
                    ),
                );
            }
            blank(&mut out);

            if let Some(definition) = &rule.definition {
                line(&mut out, &md_escape(definition));
            } else if let Some(label) = &rule.label {
                line(&mut out, &md_escape(label));
            }

            if !rule.applies_to_terms.is_empty() {
                push_line(&mut out, &format!("**{}**", model.ui("body_applies_to")));
                blank(&mut out);
                // applies_to_terms is already sorted by the model reader.
                for term in &rule.applies_to_terms {
                    push_line(
                        &mut out,
                        &format!("- {}", curie_link(model, &from, &to_curie(term))),
                    );
                }
                blank(&mut out);
            }
        }
    }

    // ── The Advice section: the recommendation tier, a distinct top-level
    // section headed by the `advice.` family rule's `#advice-` anchor. ────────────
    md_advice_section(&mut out, model, &from);
    out
}

/// Render the distinct "Advice" section: the non-gating recommendation tier. Headed
/// by the `advice.` family rule's `#advice-` anchor (the single static target every
/// advisory `advice.*` finding code resolves to), then one sub-entry per
/// [`crate::model::AdviceEntry`] carrying the term's verbatim avoid/use/how-to prose.
fn md_advice_section(out: &mut String, model: &DocsModel, from: &str) {
    // The `#advice-` anchor is the advice family rule's own slug — the guaranteed
    // resolution target of every advice.* helpUri fragment.
    let anchor = model
        .constraint_rules
        .iter()
        .find(|r| r.code == gmeow_validate::codes::ADVICE_FAMILY)
        .map(|r| r.slug.clone())
        .unwrap_or_else(|| {
            gmeow_validate::rule_catalog::slugify(gmeow_validate::codes::ADVICE_FAMILY)
        });
    heading(out, 2, model.ui("body_usage_advice"));
    push_line(out, &format!("<a id=\"{anchor}\"></a>"));
    blank(out);
    line(
        out,
        "Non-gating **recommendations** harvested verbatim from each term's usage prose \
         (`gmeow:avoidWhen` / `gmeow:useWhen` / `gmeow:howToUse`) and made machine-active as \
         realized advice carriers. Unlike the compliance rules above, advice NEVER fails the \
         gate — an advisory `advice.*` finding resolves to this section.",
    );

    if model.advice_entries.is_empty() {
        // Honest empty state: no realized advice carrier exists yet.
        line(out, model.ui("body_no_enforced_constraints"));
        return;
    }

    for entry in &model.advice_entries {
        // Per-term navigation sub-anchor (advice-<term-slug>); the guaranteed static
        // resolution target remains the section-head `#advice-` above.
        push_line(out, &format!("<a id=\"{}\"></a>", entry.slug));
        blank(out);
        let title = entry.label.clone().unwrap_or_else(|| to_curie(&entry.term));
        heading(out, 3, &md_escape(&title));
        push_line(
            out,
            &format!("- {}", curie_link(model, from, &to_curie(&entry.term))),
        );
        blank(out);
        if let Some(definition) = &entry.definition {
            line(out, &md_escape(definition));
        }
        // The three deontic-modality legs; each rendered only when present.
        for (label, values) in [
            (model.ui("body_advice_avoid_when"), &entry.avoid_when),
            (model.ui("body_advice_use_when"), &entry.use_when),
            (model.ui("body_advice_how_to_use"), &entry.how_to_use),
        ] {
            for value in values {
                push_line(out, &format!("- **{label}:** {}", md_escape(value)));
            }
        }
        blank(out);
    }
}

/// A human-readable display name for a `logic:FindingCategory` IRI (e.g.
/// `…/logic/FindingPolicyWarning` → `Finding: Policy Warning`). Falls back to the
/// raw IRI's local name when it is not a recognized `Finding…` class.
pub(super) fn finding_category_display(iri: &str) -> String {
    let local = iri.rsplit(['/', '#']).next().unwrap_or(iri);
    let stem = local.strip_prefix("Finding").unwrap_or(local);
    // Split the CamelCase stem into spaced words.
    let mut words = String::new();
    for (i, ch) in stem.char_indices() {
        if i > 0 && ch.is_ascii_uppercase() {
            words.push(' ');
        }
        words.push(ch);
    }
    if stem == local {
        // Not a `Finding…` class — surface the local name verbatim.
        words
    } else {
        format!("Finding: {words}")
    }
}

// ── Guides: recipes / learning paths / four boxes ──────────────────────────────

pub(super) fn md_recipe_index(model: &DocsModel) -> String {
    let from = Page::RecipeIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_recipes"));
    line(
        &mut out,
        &format!(
            "**{}** task-oriented adoption recipes — short, goal-named guides that show how to \
             model one recurring task in GMEOW, each backed by canonical example files and the \
             vocabulary terms it exercises.",
            model.recipes.len()
        ),
    );
    if model.recipes.is_empty() {
        line(&mut out, model.ui("body_no_recipes"));
        return out;
    }
    push_line(&mut out, "| Recipe | Goal |");
    push_line(&mut out, "| --- | --- |");
    // model.recipes is already sorted by slug.
    for recipe in &model.recipes {
        let href = rel(&from, &Page::Recipe(recipe.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} |",
                md_escape(&recipe.title),
                href,
                md_escape(&one_line(&recipe.goal)),
            ),
        );
    }
    blank(&mut out);
    out
}

pub(super) fn md_recipe(model: &DocsModel, slug: &str) -> String {
    let Some(recipe) = model.recipes.iter().find(|r| r.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_recipe_not_found"));
        return out;
    };
    let from = Page::Recipe(slug.to_string()).dir();
    let mut out = String::new();
    heading(&mut out, 1, &recipe.title);
    line(
        &mut out,
        &format!("`{}` · recipe", code_escape(&recipe.slug)),
    );

    heading(&mut out, 2, model.ui("body_goal"));
    line(&mut out, &md_escape(&recipe.goal));

    // ── Quickstart (composed over every member term, so a promoted recipe is
    // copy-paste-runnable as a whole, not just per-term) ────────────────────────
    if !recipe.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_composed_quickstart(model, &recipe.term_curies);
        fenced(&mut out, "turtle", skeleton.trim_end());
        blank(&mut out);
    }

    if !recipe.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_terms_used"));
        for curie in &recipe.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !recipe.example_paths.is_empty() {
        heading(&mut out, 2, model.ui("body_example_files"));
        for path in &recipe.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(path)));
        }
        blank(&mut out);
    }

    if !recipe.follow_pages.is_empty() {
        heading(&mut out, 2, model.ui("body_read_next"));
        for page in &recipe.follow_pages {
            push_line(&mut out, &format!("- `{}`", code_escape(page)));
        }
        blank(&mut out);
    }

    // Learning paths that fold this recipe in.
    let mut hosting: Vec<&crate::model::DocLearningPath> = model
        .learning_paths
        .iter()
        .filter(|p| p.recipe_slugs.iter().any(|s| s == slug))
        .collect();
    hosting.sort_by(|a, b| a.slug.cmp(&b.slug));
    if !hosting.is_empty() {
        heading(&mut out, 2, model.ui("body_part_of"));
        for path in hosting {
            let href = rel(&from, &Page::LearningPath(path.slug.clone()).dir());
            push_line(
                &mut out,
                &format!("- [{}]({}index.md)", md_escape(&path.title), href),
            );
        }
        blank(&mut out);
    }
    out
}

pub(super) fn md_learning_path_index(model: &DocsModel) -> String {
    let from = Page::LearningPathIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_learning_paths"));
    line(
        &mut out,
        &format!(
            "**{}** curated adoption journeys — ordered itineraries that sequence recipes, \
             example files, and vocabulary terms so a developer learns to model a whole area end \
             to end.",
            model.learning_paths.len()
        ),
    );
    if model.learning_paths.is_empty() {
        line(&mut out, model.ui("body_no_learning_paths"));
        return out;
    }
    push_line(&mut out, "| Learning path | Audience | Goal |");
    push_line(&mut out, "| --- | --- | --- |");
    // model.learning_paths is already sorted by slug.
    for path in &model.learning_paths {
        let href = rel(&from, &Page::LearningPath(path.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | {} |",
                md_escape(&path.title),
                href,
                md_escape(&one_line(&path.audience)),
                md_escape(&one_line(&path.goal)),
            ),
        );
    }
    blank(&mut out);
    out
}

pub(super) fn md_learning_path(model: &DocsModel, slug: &str) -> String {
    let Some(path) = model.learning_paths.iter().find(|p| p.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_learning_path_not_found"));
        return out;
    };
    let from = Page::LearningPath(slug.to_string()).dir();
    let mut out = String::new();
    heading(&mut out, 1, &path.title);
    line(
        &mut out,
        &format!("`{}` · learning path", code_escape(&path.slug)),
    );

    push_line(&mut out, "| Field | Value |");
    push_line(&mut out, "| --- | --- |");
    push_line(
        &mut out,
        &format!("| Audience | {} |", md_escape(&one_line(&path.audience))),
    );
    blank(&mut out);

    heading(&mut out, 2, model.ui("body_goal"));
    line(&mut out, &md_escape(&path.goal));

    // ── Quickstart (composed over every member term, so a promoted learning
    // path is copy-paste-runnable as a whole, not just per-term) ───────────────
    if !path.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_composed_quickstart(model, &path.term_curies);
        fenced(&mut out, "turtle", skeleton.trim_end());
        blank(&mut out);
    }

    if !path.recipe_slugs.is_empty() {
        heading(&mut out, 2, model.ui("body_recipes"));
        for recipe_slug in &path.recipe_slugs {
            let title = model
                .recipes
                .iter()
                .find(|r| &r.slug == recipe_slug)
                .map(|r| r.title.clone())
                .unwrap_or_else(|| recipe_slug.clone());
            // Link only when the recipe page exists in the model.
            if model.recipes.iter().any(|r| &r.slug == recipe_slug) {
                let href = rel(&from, &Page::Recipe(recipe_slug.clone()).dir());
                push_line(
                    &mut out,
                    &format!("- [{}]({}index.md)", md_escape(&title), href),
                );
            } else {
                push_line(&mut out, &format!("- {}", md_escape(&title)));
            }
        }
        blank(&mut out);
    }

    if !path.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_terms_used"));
        for curie in &path.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !path.example_paths.is_empty() {
        heading(&mut out, 2, model.ui("body_example_files"));
        for p in &path.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(p)));
        }
        blank(&mut out);
    }

    if !path.adoption_targets.is_empty() {
        heading(&mut out, 2, model.ui("body_projects_toward"));
        let cells: Vec<String> = path
            .adoption_targets
            .iter()
            .map(|t| format!("`{}`", code_escape(t)))
            .collect();
        line(&mut out, &cells.join(", "));
    }
    out
}

pub(super) fn md_four_boxes(model: &DocsModel) -> String {
    let mut out = String::new();
    // The curated prose is authored Markdown; emit it verbatim (its SPDX comment
    // header is an HTML comment and renders inertly). Fall back to a stub when
    // absent so the page is never empty.
    match &model.four_boxes {
        Some(prose) => {
            out.push_str(prose);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        None => {
            heading(&mut out, 1, model.ui("body_what_is_this"));
            line(
                &mut out,
                "The four-boxes doctrine prose is not available in this build.",
            );
        }
    }
    out
}
