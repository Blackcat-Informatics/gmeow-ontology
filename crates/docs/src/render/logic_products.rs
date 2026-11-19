// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Documentation for the information products of the canonical logic compiler.

use crate::model::DocsModel;

use super::{Page, blank, code_escape, heading, line, md_escape, one_line, push_line, rel};

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
