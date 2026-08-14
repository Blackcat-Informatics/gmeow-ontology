// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ONE canonical term-card renderer (§19 one-path).
//!
//! A GMEOW term card is a compact, link-free, prompt-ready Markdown block: a
//! metadata header, the definition, and every usage-advisory field. Before this
//! module two divergent renderers emitted "the same card" — the docs-site
//! `term_body` (`crates/docs/src/render.rs`) and the folded-snapshot
//! `term_card_lines` (`crates/pipeline/src/stages/export.rs`) — with different
//! conventions (bold vs italic labels, `; ` vs `, ` delimiters, backticks or
//! not). That violated §19 (one-path) and P15 (maximal information flow): the
//! MCP card claimed to be "the live twin" of the site card while rendering it
//! differently.
//!
//! This module is the single source of truth. Both crates build a neutral
//! plain-data [`Card`] (values are PRE-RESOLVED display strings — the caller
//! does local-name/CURIE resolution) and call [`render_card_body`] /
//! [`render_card`]. The canonical convention is the SITE card's: **bold**
//! labels, `; ` value delimiters, NO per-item backticks, and a metadata header.
//!
//! Pure / std-only so it has no I/O or graph dependency (the `serde` derive is
//! data-only, no I/O).

use serde::Serialize;
use serde_json::Value;

/// How much of a [`Card`] to surface — the token-budgeted detail tiers the live
/// `doc_card` MCP tool exposes. The DEFAULT is [`CardDetail::Standard`], whose
/// rendered body is byte-identical to the historical unconditional render (the
/// published docs-site `card.md`); the single-renderer authority is preserved.
///
/// * [`CardDetail::Summary`] — the leanest surface: title + definition ONLY, no
///   metadata header, no advisory fields, no rich panels. The cheapest card.
/// * [`CardDetail::Standard`] — EXACTLY the compact card (metadata header +
///   definition + every advisory field), NONE of the full-tier rich panels.
/// * [`CardDetail::Full`] — the oracle card: `Standard` plus the rich panels
///   (entailments, Do / Don't fixtures, diagnostics, projection loss) appended as
///   clearly-headed sections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CardDetail {
    /// Title + definition only.
    Summary,
    /// The compact card — the default, byte-identical to the historical render.
    #[default]
    Standard,
    /// The compact card plus the full-tier rich panels.
    Full,
}

/// One reasoner entailment documenting the term (full tier): the rule that fires,
/// its conclusion, and every premise the derivation rests on. Field order is fixed
/// for deterministic serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CardEntailment {
    /// The entailment rule name.
    pub rule: String,
    /// The derived conclusion.
    pub conclusion: String,
    /// Every premise the derivation rests on, sorted for determinism.
    pub premises: Vec<String>,
}

/// One conformance fixture documenting the term (full tier): its human title and a
/// short body (the fixture Turtle, capped). Both the well-formed (Do) and the
/// counter-example (Don't) panels are lists of these. Field order is fixed for
/// deterministic serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CardFixture {
    /// The fixture's human label / title.
    pub title: String,
    /// A short body (the fixture body, one-lined and capped) or a reference.
    pub body: String,
}

/// One diagnostic finding the term may hit (full tier): the finding code and a
/// short note. Field order is fixed for deterministic serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CardDiagnostic {
    /// The finding code (the stable identifier of the diagnostic).
    pub code: String,
    /// A short note describing the diagnostic incidence.
    pub note: String,
}

/// One projection-loss row for the term (full tier): the projection target the
/// term degrades into and the preservation judgment for that degradation. Field
/// order is fixed for deterministic serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CardLoss {
    /// The projection target (the lossy projection this term degrades into).
    pub target: String,
    /// The preservation judgment (`logic:preservationKind` local name(s)).
    pub preservation: String,
}

/// The full UNION of fields both the docs-site `DocTerm` and the folded `Term`
/// can carry for one term. Every value is a pre-resolved DISPLAY string (the
/// caller resolves IRIs to local names / CURIEs); the renderer never touches
/// IRIs. Empty `Vec`s / empty `String`s / `None`s are omitted from the output.
///
/// `#[derive(Serialize)]` gives `format=json` a byte-stable projection: the fixed
/// struct field order is the serialization order, and every field is a `String` /
/// `Option` / `Vec` whose contents the caller has already deterministically
/// ordered. Empty optional / vector fields are skipped, so a leaner tier
/// serializes to a strictly smaller object than a richer one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Card {
    /// The vocabulary category, singular and human-cased (`"Class"`,
    /// `"Property"`, `"Individual"`, `"Datatype"`, `"Term"`).
    pub category: String,
    /// The full term IRI.
    pub iri: String,
    /// `rdfs:label`, omitted from the header when it equals the title/CURIE.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The defining slice as a display string (the owning module's local name).
    /// Both sources carry it: the docs side from `DocTerm::owner_slice`, the
    /// folded/MCP side from the documentation graph's `gmeow:docOwnerSlice`.
    /// `None` only when the source genuinely has no slice for this term, and is
    /// rendered as no `slice:` header line — NEVER a blank value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slice: Option<String>,
    /// `gmeow:graphBoxRole` four-boxes role display names (e.g. `boxTBox`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub box_roles: Vec<String>,
    /// `skos:definition` (falling back to `rdfs:comment`), one-lined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    /// `logic:subClassOf`/`logic:subPropertyOf` (canonical) ∪ `rdfs:subClassOf`/
    /// `rdfs:subPropertyOf` (projection) parent display names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
    /// `rdfs:domain` display names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub domain: Vec<String>,
    /// `rdfs:range` display names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub range: Vec<String>,
    /// `gmeow:useWhen` prose.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub use_when: Vec<String>,
    /// `gmeow:avoidWhen` prose.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub avoid_when: Vec<String>,
    /// `gmeow:howToUse` prose.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub how_to_use: Vec<String>,
    /// `skos:scopeNote` prose.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scope_notes: Vec<String>,
    /// `skos:example` prose.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    /// `logic:*` stereotype CURIEs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub logic_stereotypes: Vec<String>,
    /// Related-term display names (`skos:related` ∪ `gmeow:pairsWith` ∪
    /// `rdfs:seeAlso`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_terms: Vec<String>,
    /// `gmeow:useForConsumer` profile display names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub use_for_consumer: Vec<String>,
    /// `gmeow:avoidForConsumer` profile display names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub avoid_for_consumer: Vec<String>,
    /// Alignment facets, each a pre-formatted `predicate=object` display string.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aligns: Vec<String>,
    /// The importable dotted path `gmeow_models.<slice>.<Class>` of the generated
    /// Pydantic model for a MODELED CLASS term — the explicit term→model link
    /// (§19: importing the model IS reading the term). `None` for non-class terms,
    /// which have no generated model. Rides the Standard + Full tiers, not Summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_model: Option<String>,
    /// A COMPACT, deterministic Pydantic snippet for a modeled class term — the
    /// model import plus a `model_validate` of a minimal `@type` payload. `None`
    /// for non-class terms. Token-budgeted (short) so it rides Standard + Full.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_snippet: Option<String>,
    // ── Full-tier rich panels (populated ONLY for [`CardDetail::Full`]) ─────────
    /// The reasoner entailments documenting the term. Empty for a term with no
    /// entailments, and for every tier below `Full` (never queried there).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entailments: Vec<CardEntailment>,
    /// The well-formed conformance exemplars (the "Do" panel).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixtures_do: Vec<CardFixture>,
    /// The counter-example conformance fixtures (the "Don't" panel).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixtures_dont: Vec<CardFixture>,
    /// The diagnostic findings the term may hit.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<CardDiagnostic>,
    /// The projection-loss rows: the targets the term degrades into.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub loss: Vec<CardLoss>,
}

impl Card {
    /// Project this card down to `detail`, clearing the fields a leaner tier does
    /// not carry, so `format=json` serializes strictly less at a lower tier:
    ///
    /// * `Full` — the whole card, unchanged.
    /// * `Standard` — the compact card with the rich panels cleared.
    /// * `Summary` — identity (category / IRI / label / slice) plus the
    ///   definition only; every advisory field and rich panel cleared.
    ///
    /// Deterministic (a pure field projection), so the JSON is byte-stable.
    #[must_use]
    pub fn projected(&self, detail: CardDetail) -> Card {
        match detail {
            CardDetail::Full => self.clone(),
            CardDetail::Standard => Card {
                entailments: Vec::new(),
                fixtures_do: Vec::new(),
                fixtures_dont: Vec::new(),
                diagnostics: Vec::new(),
                loss: Vec::new(),
                ..self.clone()
            },
            CardDetail::Summary => Card {
                category: self.category.clone(),
                iri: self.iri.clone(),
                label: self.label.clone(),
                slice: self.slice.clone(),
                definition: self.definition.clone(),
                ..Card::default()
            },
        }
    }
}

/// Render the card BODY (metadata header + definition + every advisory field, NO
/// heading) at `detail` — the shared core of the per-term `card.md` and the
/// inlined `llms-full.txt` block. Deterministic field order; a field is emitted
/// only when non-empty.
///
/// Tier gating (the SINGLE renderer; a leaner tier is a strict prefix-in-spirit
/// of a richer one):
///
/// * [`CardDetail::Summary`] — the definition ONLY (no metadata header, no
///   advisory fields, no rich panels). The `# {title}` from [`render_card`]
///   supplies the title.
/// * [`CardDetail::Standard`] — EXACTLY the historical body: metadata header +
///   definition + every advisory field. Byte-identical to the pre-tier render, so
///   the published docs-site `card.md` is unchanged.
/// * [`CardDetail::Full`] — `Standard` followed by the rich panels (entailments,
///   Do / Don't fixtures, diagnostics, projection loss), each a clearly-headed
///   `## ` section, emitted only when its panel is non-empty.
pub fn render_card_body(card: &Card, detail: CardDetail) -> String {
    let mut out = String::new();

    // ── Summary: the definition ONLY (title supplied by `render_card`). ───────
    if detail == CardDetail::Summary {
        if let Some(def) = &card.definition
            && !def.is_empty()
        {
            out.push_str(def);
            out.push_str("\n\n");
        }
        return out;
    }

    // ── Metadata header ──────────────────────────────────────────────────────
    out.push_str(&format!(
        "- category: {}\n- iri: {}\n",
        card.category, card.iri
    ));
    if let Some(slice) = &card.slice {
        out.push_str(&format!("- slice: {slice}\n"));
    }
    if let Some(label) = &card.label {
        out.push_str(&format!("- label: {label}\n"));
    }
    if let Some(box_role) = card.box_roles.first() {
        out.push_str(&format!("- box: {box_role}\n"));
    }
    out.push('\n');

    // ── Definition ───────────────────────────────────────────────────────────
    if let Some(def) = &card.definition
        && !def.is_empty()
    {
        out.push_str(def);
        out.push_str("\n\n");
    }

    // ── Advisory fields (one canonical convention: **bold**, `; ` delimiter) ──
    let mut field = |label: &str, values: &[String]| {
        if !values.is_empty() {
            out.push_str(&format!("**{label}:** {}\n\n", values.join("; ")));
        }
    };
    field("Parents", &card.parents);
    field("Domain", &card.domain);
    field("Range", &card.range);
    field("Box roles", &card.box_roles);
    field("Use when", &card.use_when);
    field("Avoid when", &card.avoid_when);
    field("How to use", &card.how_to_use);
    field("Use for consumers", &card.use_for_consumer);
    field("Avoid for consumers", &card.avoid_for_consumer);
    field("Scope notes", &card.scope_notes);
    field("Examples", &card.examples);
    field("Logic", &card.logic_stereotypes);
    field("Related", &card.related_terms);
    field("Aligns", &card.aligns);

    // ── Python model surface — a modeled class links to its generated Pydantic
    //    model (the explicit term→model link) plus a compact construct/validate
    //    snippet. Present only for a class term; rides Standard + Full (Summary
    //    returned early above), so both compact surfaces carry it. ─────────────
    if let Some(model) = &card.python_model {
        out.push_str(&format!("**Python model:** `{model}`\n\n"));
        if let Some(snippet) = &card.python_snippet {
            out.push_str(&format!("```python\n{snippet}\n```\n\n"));
        }
    }

    // ── Full-tier rich panels — appended after the compact body. ─────────────
    if detail == CardDetail::Full {
        render_full_panels(&mut out, card);
    }

    out
}

/// Append the full-tier oracle panels to `out`, each a clearly-headed `## `
/// section emitted only when its panel is non-empty (an empty panel is an honest
/// omission, never a fabricated section).
fn render_full_panels(out: &mut String, card: &Card) {
    if !card.entailments.is_empty() {
        out.push_str("## Entailments\n\n");
        for e in &card.entailments {
            out.push_str(&format!("- **{}** ⊢ {}\n", e.rule, e.conclusion));
            if !e.premises.is_empty() {
                out.push_str(&format!("  - premises: {}\n", e.premises.join("; ")));
            }
        }
        out.push('\n');
    }
    let fixtures = |out: &mut String, heading: &str, items: &[CardFixture]| {
        if !items.is_empty() {
            out.push_str(&format!("## {heading}\n\n"));
            for f in items {
                out.push_str(&format!("- **{}**", f.title));
                if !f.body.is_empty() {
                    out.push_str(&format!(" — {}", f.body));
                }
                out.push('\n');
            }
            out.push('\n');
        }
    };
    fixtures(out, "Do", &card.fixtures_do);
    fixtures(out, "Don't", &card.fixtures_dont);
    if !card.diagnostics.is_empty() {
        out.push_str("## Diagnostics\n\n");
        for d in &card.diagnostics {
            out.push_str(&format!("- **{}** — {}\n", d.code, d.note));
        }
        out.push('\n');
    }
    if !card.loss.is_empty() {
        out.push_str("## Degrades under projection\n\n");
        for l in &card.loss {
            out.push_str(&format!("- {} — {}\n", l.target, l.preservation));
        }
        out.push('\n');
    }
}

/// Render a complete, standalone card: `# {title}\n\n` followed by
/// [`render_card_body`] at `detail`. Used for the per-term `card.md` file and the
/// live MCP `doc_card`.
pub fn render_card(title: &str, card: &Card, detail: CardDetail) -> String {
    format!("# {title}\n\n{}", render_card_body(card, detail))
}

/// The importable dotted path `gmeow_models.<slice>.<Class>` of the generated
/// Pydantic model for a class term — the explicit term→model link ([`Card`]'s
/// `python_model`). Composed from the SAME routing the Pydantic emitter and the
/// docs-site Python example tab use ([`crate::slug::pydantic_module_slug`] +
/// [`crate::slug::pydantic_class_name`]), so the card link can never drift from
/// the generated module. Deterministic; callers gate it on a class term.
#[must_use]
pub fn python_model_path(slice_iri: &str, term_iri: &str) -> String {
    format!(
        "gmeow_models.{}.{}",
        crate::slug::pydantic_module_slug(slice_iri),
        crate::slug::pydantic_class_name(term_iri)
    )
}

/// A COMPACT, deterministic Pydantic construct-and-validate snippet for a class
/// term ([`Card`]'s `python_snippet`): the model import plus a `model_validate`
/// of a minimal `{"@type": "<curie>"}` payload. Short by design — the card is
/// token-budgeted — and derived from the SAME emitter routing as
/// [`python_model_path`], so import + class never drift from the generated model.
#[must_use]
pub fn python_model_snippet(slice_iri: &str, term_iri: &str, curie: &str) -> String {
    let module = crate::slug::pydantic_module_slug(slice_iri);
    let class = crate::slug::pydantic_class_name(term_iri);
    format!(
        "from gmeow_models.{module} import {class}\n\
         obj = {class}.model_validate({{\"@type\": \"{curie}\"}})"
    )
}

/// The hand-authored JSON Schema (draft 2020-12) describing the serialized term
/// [`Card`] — the exact shape of the packed `terms/{slug}/card.json` member and the
/// live MCP `doc_card format=json` payload.
///
/// It is co-located WITH the [`Card`] type on purpose (drift-resistance /
/// dogfooding): the `properties` mirror the struct's serialized fields, the two
/// unconditional fields (`category`, `iri`) are `required`, and every
/// `#[serde(skip_serializing_if)]` field is optional (absent when empty). The rich
/// panels reference `$defs` matching [`CardEntailment`], [`CardFixture`],
/// [`CardDiagnostic`], and [`CardLoss`] (whose subfields carry no skip attribute, so
/// each is `required`). Keeping this beside the type means a field added to `Card`
/// that is not mirrored here is caught by the conformance test that validates a REAL
/// rendered card against this schema.
#[must_use]
pub fn card_json_schema() -> serde_json::Value {
    let string_array = || serde_json::json!({ "type": "array", "items": { "type": "string" } });
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://blackcatinformatics.ca/gmeow/schemas/card.schema.json",
        "title": "GMEOW term card",
        "description": "The neutral, pre-resolved term card the docs `card.json` member \
                        and the live MCP `doc_card format=json` tool serialize.",
        "type": "object",
        "additionalProperties": false,
        "required": ["category", "iri"],
        "properties": {
            "category": {
                "type": "string",
                "description": "The vocabulary category (Class, Property, Individual, Datatype, Term)."
            },
            "iri": { "type": "string", "description": "The full term IRI." },
            "label": { "type": "string" },
            "slice": { "type": "string" },
            "box_roles": string_array(),
            "definition": { "type": "string" },
            "parents": string_array(),
            "domain": string_array(),
            "range": string_array(),
            "use_when": string_array(),
            "avoid_when": string_array(),
            "how_to_use": string_array(),
            "scope_notes": string_array(),
            "examples": string_array(),
            "logic_stereotypes": string_array(),
            "related_terms": string_array(),
            "use_for_consumer": string_array(),
            "avoid_for_consumer": string_array(),
            "aligns": string_array(),
            "python_model": {
                "type": "string",
                "description": "The importable dotted path gmeow_models.<slice>.<Class> of the \
                                generated Pydantic model for a class term (the term→model link)."
            },
            "python_snippet": {
                "type": "string",
                "description": "A compact Pydantic import + model_validate snippet for a class term."
            },
            "entailments": { "type": "array", "items": { "$ref": "#/$defs/entailment" } },
            "fixtures_do": { "type": "array", "items": { "$ref": "#/$defs/fixture" } },
            "fixtures_dont": { "type": "array", "items": { "$ref": "#/$defs/fixture" } },
            "diagnostics": { "type": "array", "items": { "$ref": "#/$defs/diagnostic" } },
            "loss": { "type": "array", "items": { "$ref": "#/$defs/loss" } }
        },
        "$defs": {
            "entailment": {
                "type": "object",
                "additionalProperties": false,
                "required": ["rule", "conclusion", "premises"],
                "properties": {
                    "rule": { "type": "string" },
                    "conclusion": { "type": "string" },
                    "premises": { "type": "array", "items": { "type": "string" } }
                }
            },
            "fixture": {
                "type": "object",
                "additionalProperties": false,
                "required": ["title", "body"],
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string" }
                }
            },
            "diagnostic": {
                "type": "object",
                "additionalProperties": false,
                "required": ["code", "note"],
                "properties": {
                    "code": { "type": "string" },
                    "note": { "type": "string" }
                }
            },
            "loss": {
                "type": "object",
                "additionalProperties": false,
                "required": ["target", "preservation"],
                "properties": {
                    "target": { "type": "string" },
                    "preservation": { "type": "string" }
                }
            }
        }
    })
}

/// The output serialization for a term [`Card`] on the `describe` surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardFormat {
    /// Human-facing Markdown prose (the default) — the canonical `render_card`.
    #[default]
    Prose,
    /// Pretty JSON of the serialized card (the [`card_json_schema`] shape).
    Json,
    /// TOON (Token-Oriented Object Notation) — a compact, token-efficient,
    /// indentation-based serialization for LLM/agent consumption.
    Toon,
}

/// Serialize a card as pretty JSON — the exact field shape of the packed
/// `card.json` member and the `doc_card format=json` payload (see
/// [`card_json_schema`]). Byte-stable: struct-declaration field order, empty
/// optionals/vectors skipped.
#[must_use]
pub fn card_json(card: &Card) -> String {
    serde_json::to_string_pretty(card)
        .expect("a Card is pure String/Vec data and always serializes to JSON")
}

/// Serialize a card as TOON — the compact, token-oriented notation. Encodes the
/// card's neutral serde JSON model, so it is reusable for any [`serde::Serialize`]
/// value that maps to the same JSON object/array/scalar shape.
///
/// The encoding: object fields as `key: value` lines (nested objects indented by
/// two spaces); scalar arrays inline and length-tagged (`key[N]: a,b,c`); arrays
/// of uniform scalar-valued objects as a length-and-header-tagged table
/// (`key[N]{f1,f2}:` then one comma-row per element); any other array as a
/// length-tagged `-`-marked list. A string is quoted only when it would otherwise
/// be ambiguous (empty, padded, or containing a structural delimiter / a
/// number/bool/null lexeme).
#[must_use]
pub fn card_toon(card: &Card) -> String {
    let value = serde_json::to_value(card)
        .expect("a Card is pure String/Vec data and always serializes to a JSON value");
    let mut out = String::new();
    toon_encode(&value, 0, &mut out);
    out
}

/// Render a JSON value at the root of a TOON document (or as a `-`-list element).
fn toon_encode(value: &Value, indent: usize, out: &mut String) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                toon_field(k, v, indent, out);
            }
        }
        Value::Array(arr) => toon_array("", arr, indent, out),
        scalar => {
            let pad = "  ".repeat(indent);
            out.push_str(&pad);
            out.push_str(&toon_scalar(scalar));
            out.push('\n');
        }
    }
}

/// Render one `key`/`value` object field.
fn toon_field(key: &str, value: &Value, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    let k = toon_scalar_str(key);
    match value {
        Value::Object(map) => {
            out.push_str(&format!("{pad}{k}:\n"));
            for (ck, cv) in map {
                toon_field(ck, cv, indent + 1, out);
            }
        }
        Value::Array(arr) => toon_array(&k, arr, indent, out),
        scalar => out.push_str(&format!("{pad}{k}: {}\n", toon_scalar(scalar))),
    }
}

/// Render an array field (`key` already escaped; empty for a root array).
fn toon_array(key: &str, arr: &[Value], indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    let n = arr.len();
    if arr.is_empty() {
        out.push_str(&format!("{pad}{key}[0]:\n"));
        return;
    }
    // Scalar array → inline comma list.
    if arr.iter().all(is_scalar) {
        let items: Vec<String> = arr.iter().map(toon_scalar).collect();
        out.push_str(&format!("{pad}{key}[{n}]: {}\n", items.join(",")));
        return;
    }
    // Uniform scalar-valued objects → tabular.
    if let Some(fields) = uniform_scalar_object_fields(arr) {
        let header: Vec<String> = fields.iter().map(|f| toon_scalar_str(f)).collect();
        out.push_str(&format!("{pad}{key}[{n}]{{{}}}:\n", header.join(",")));
        let rowpad = "  ".repeat(indent + 1);
        for elem in arr {
            let obj = elem
                .as_object()
                .expect("uniform check guarantees an object");
            let cells: Vec<String> = fields.iter().map(|f| toon_scalar(&obj[f])).collect();
            out.push_str(&format!("{rowpad}{}\n", cells.join(",")));
        }
        return;
    }
    // Otherwise → a `-`-marked list of nested elements.
    out.push_str(&format!("{pad}{key}[{n}]:\n"));
    let elem_indent = indent + 1;
    let dashpad = "  ".repeat(elem_indent);
    for elem in arr {
        if is_scalar(elem) {
            out.push_str(&format!("{dashpad}- {}\n", toon_scalar(elem)));
        } else {
            let mut buf = String::new();
            toon_encode(elem, elem_indent + 1, &mut buf);
            splice_dash(&buf, elem_indent, out);
        }
    }
}

/// Splice a `- ` element marker onto the first line of a rendered nested element,
/// keeping the remaining lines' deeper indent aligned under it.
fn splice_dash(buf: &str, elem_indent: usize, out: &mut String) {
    let inner_pad = "  ".repeat(elem_indent + 1);
    let dash_pad = "  ".repeat(elem_indent);
    let mut first = true;
    for line in buf.lines() {
        if first {
            let content = line.strip_prefix(&inner_pad).unwrap_or(line);
            out.push_str(&format!("{dash_pad}- {content}\n"));
            first = false;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
}

/// A JSON scalar (not an object or array).
fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Object(_) | Value::Array(_))
}

/// The shared field list when every array element is a non-empty object with the
/// same keys and only scalar values — the precondition for the tabular form.
fn uniform_scalar_object_fields(arr: &[Value]) -> Option<Vec<String>> {
    let first = arr.first()?.as_object()?;
    if first.is_empty() {
        return None;
    }
    let fields: Vec<String> = first.keys().cloned().collect();
    for elem in arr {
        let obj = elem.as_object()?;
        if obj.len() != fields.len() {
            return None;
        }
        for f in &fields {
            match obj.get(f) {
                Some(v) if is_scalar(v) => {}
                _ => return None,
            }
        }
    }
    Some(fields)
}

/// A TOON scalar token: `null`/`true`/`false`/number bare, string minimally quoted.
fn toon_scalar(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => toon_scalar_str(s),
        // Objects/arrays are never rendered as an inline scalar.
        other => other.to_string(),
    }
}

/// A string as a TOON token — bare when unambiguous, else double-quoted with the
/// structural escapes.
fn toon_scalar_str(s: &str) -> String {
    let needs_quote = s.is_empty()
        || s != s.trim()
        || s.contains([',', ':', '"', '\n', '\t', '\r', '\\', '[', ']', '{', '}'])
        || matches!(s, "true" | "false" | "null")
        || s.parse::<f64>().is_ok();
    if needs_quote {
        let escaped = s
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\r', "\\r");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Card {
        Card {
            category: "Class".to_string(),
            iri: "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
            label: Some("Foo".to_string()),
            slice: Some("demo".to_string()),
            box_roles: vec!["boxTBox".to_string()],
            definition: Some("A foo.".to_string()),
            parents: vec!["Bar".to_string(), "Baz".to_string()],
            use_when: vec!["When you have a foo.".to_string()],
            use_for_consumer: vec!["gmeow:profileMemory".to_string()],
            aligns: vec!["exactMatch=ex:Foo".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn body_emits_metadata_header_then_definition_then_advisories() {
        let body = render_card_body(&sample(), CardDetail::Standard);
        // Header lines, in canonical order.
        assert!(body.starts_with(
            "- category: Class\n- iri: https://blackcatinformatics.ca/gmeow/Foo\n- slice: demo\n- label: Foo\n- box: boxTBox\n\n"
        ));
        // Definition.
        assert!(body.contains("\nA foo.\n\n"));
        // Bold labels + `; ` delimiter, NO backticks.
        assert!(body.contains("**Parents:** Bar; Baz\n\n"));
        assert!(body.contains("**Use when:** When you have a foo.\n\n"));
        assert!(body.contains("**Use for consumers:** gmeow:profileMemory\n\n"));
        assert!(body.contains("**Aligns:** exactMatch=ex:Foo\n\n"));
        assert!(
            !body.contains('`'),
            "canonical card uses no per-item backticks"
        );
        // Labels are bold (`**`), never the folded side's single-asterisk italic
        // (`*Use when:* …`). A bold `**Use when:** ` is fine; assert no italic
        // label survives — i.e. no `\n*Use when:* ` (single asterisk after a newline).
        assert!(
            !body.contains("\n*Use when:* "),
            "labels must be bold (**), never single-italic"
        );
    }

    #[test]
    fn empty_fields_are_omitted() {
        let card = Card {
            category: "Individual".to_string(),
            iri: "https://blackcatinformatics.ca/gmeow/Bar".to_string(),
            ..Default::default()
        };
        let body = render_card_body(&card, CardDetail::Standard);
        assert_eq!(
            body,
            "- category: Individual\n- iri: https://blackcatinformatics.ca/gmeow/Bar\n\n"
        );
        // No slice / label / box header line when those are absent.
        assert!(!body.contains("- slice:"));
        assert!(!body.contains("- label:"));
        assert!(!body.contains("- box:"));
    }

    #[test]
    fn slice_none_never_emits_blank_slice() {
        let card = Card {
            category: "Class".to_string(),
            iri: "x".to_string(),
            slice: None,
            ..Default::default()
        };
        let body = render_card_body(&card, CardDetail::Standard);
        assert!(
            !body.contains("- slice:"),
            "None slice must omit the line, not blank it"
        );
    }

    #[test]
    fn render_card_prepends_h1_title() {
        let card = sample();
        let full = render_card("gmeow:Foo (⊑ Bar)", &card, CardDetail::Standard);
        assert!(full.starts_with("# gmeow:Foo (⊑ Bar)\n\n"));
        assert!(full.contains("- category: Class\n"));
        assert_eq!(
            full,
            format!(
                "# gmeow:Foo (⊑ Bar)\n\n{}",
                render_card_body(&card, CardDetail::Standard)
            )
        );
    }

    /// A card carrying every rich panel, for the tier-gating tests.
    fn full_sample() -> Card {
        Card {
            entailments: vec![CardEntailment {
                rule: "subClassOf-transitivity".to_string(),
                conclusion: "Foo ⊑ Qux".to_string(),
                premises: vec!["Foo ⊑ Bar".to_string(), "Bar ⊑ Qux".to_string()],
            }],
            fixtures_do: vec![CardFixture {
                title: "Well-formed Foo".to_string(),
                body: "a valid foo shape".to_string(),
            }],
            fixtures_dont: vec![CardFixture {
                title: "Foo missing range".to_string(),
                body: "violates the range requirement".to_string(),
            }],
            diagnostics: vec![CardDiagnostic {
                code: "gmeow-range-missing".to_string(),
                note: "has 1 diagnostic finding(s)".to_string(),
            }],
            loss: vec![CardLoss {
                target: "owl".to_string(),
                preservation: "Weakened".to_string(),
            }],
            ..sample()
        }
    }

    #[test]
    fn summary_is_definition_only_and_smaller_than_standard() {
        let card = full_sample();
        let summary = render_card_body(&card, CardDetail::Summary);
        // Definition present…
        assert!(summary.contains("A foo."));
        // …but NONE of the header / advisory / panel surface.
        assert!(!summary.contains("- category:"));
        assert!(!summary.contains("**Parents:**"));
        assert!(!summary.contains("## Entailments"));
        let standard = render_card_body(&card, CardDetail::Standard);
        assert!(summary.len() < standard.len());
    }

    #[test]
    fn standard_carries_no_full_panels_but_full_does() {
        let card = full_sample();
        let standard = render_card_body(&card, CardDetail::Standard);
        // Standard is EXACTLY the compact card: no rich-panel headers.
        assert!(!standard.contains("## Entailments"));
        assert!(!standard.contains("## Do"));
        assert!(!standard.contains("## Don't"));
        assert!(!standard.contains("## Diagnostics"));
        assert!(!standard.contains("## Degrades under projection"));

        let full = render_card_body(&card, CardDetail::Full);
        // Full is a superset: the whole compact body PLUS every panel.
        assert!(full.starts_with(&standard));
        assert!(full.len() > standard.len());
        assert!(full.contains("## Entailments\n\n- **subClassOf-transitivity** ⊢ Foo ⊑ Qux\n"));
        assert!(full.contains("  - premises: Foo ⊑ Bar; Bar ⊑ Qux\n"));
        assert!(full.contains("## Do\n\n- **Well-formed Foo** — a valid foo shape\n"));
        assert!(full.contains("## Don't\n\n- **Foo missing range**"));
        assert!(full.contains("## Diagnostics\n\n- **gmeow-range-missing**"));
        assert!(full.contains("## Degrades under projection\n\n- owl — Weakened\n"));
    }

    #[test]
    fn empty_panels_are_omitted_at_full_tier() {
        // A card with NO rich panels renders identically at Full and Standard —
        // honest empty sections are omitted, never fabricated.
        let card = sample();
        assert_eq!(
            render_card_body(&card, CardDetail::Full),
            render_card_body(&card, CardDetail::Standard)
        );
    }

    #[test]
    fn projected_json_tiers_are_strictly_nested() {
        let card = full_sample();
        let summary = serde_json::to_string(&card.projected(CardDetail::Summary)).unwrap();
        let standard = serde_json::to_string(&card.projected(CardDetail::Standard)).unwrap();
        let full = serde_json::to_string(&card.projected(CardDetail::Full)).unwrap();
        // Standard JSON MUST NOT carry any full-tier rich key.
        assert!(!standard.contains("entailments"));
        assert!(!standard.contains("fixtures_do"));
        assert!(!standard.contains("diagnostics"));
        // Summary carries identity + definition but no advisory field.
        assert!(summary.contains("\"definition\":\"A foo.\""));
        assert!(!summary.contains("use_when"));
        // Full carries the rich panels.
        assert!(full.contains("\"entailments\""));
        assert!(full.contains("\"loss\""));
        // Byte-stable across two serializations.
        assert_eq!(
            full,
            serde_json::to_string(&card.projected(CardDetail::Full)).unwrap()
        );
        // Monotone by size.
        assert!(summary.len() <= standard.len());
        assert!(standard.len() < full.len());
    }

    #[test]
    fn python_model_path_and_snippet_route_through_the_emitter() {
        let slice = "https://blackcatinformatics.ca/gmeow/slices/lifecycle";
        let term = "https://blackcatinformatics.ca/gmeow/Foo";
        assert_eq!(
            python_model_path(slice, term),
            "gmeow_models.lifecycle.Foo",
            "the dotted path is gmeow_models.<slice>.<Class>"
        );
        let snippet = python_model_snippet(slice, term, "gmeow:Foo");
        assert_eq!(
            snippet,
            "from gmeow_models.lifecycle import Foo\n\
             obj = Foo.model_validate({\"@type\": \"gmeow:Foo\"})"
        );
    }

    #[test]
    fn class_card_carries_python_model_link_and_snippet() {
        let slice = "https://blackcatinformatics.ca/gmeow/slices/lifecycle";
        let term = "https://blackcatinformatics.ca/gmeow/Foo";
        let card = Card {
            python_model: Some(python_model_path(slice, term)),
            python_snippet: Some(python_model_snippet(slice, term, "gmeow:Foo")),
            ..sample()
        };

        // Standard body renders the explicit link + the fenced snippet.
        let standard = render_card_body(&card, CardDetail::Standard);
        assert!(standard.contains("**Python model:** `gmeow_models.lifecycle.Foo`\n\n"));
        assert!(
            standard.contains("```python\nfrom gmeow_models.lifecycle import Foo\n"),
            "the compact card carries the fenced Pydantic snippet"
        );
        // Full carries it too (it is a superset of Standard).
        let full = render_card_body(&card, CardDetail::Full);
        assert!(full.contains("**Python model:** `gmeow_models.lifecycle.Foo`"));

        // Summary drops the model surface entirely.
        let summary = render_card_body(&card, CardDetail::Summary);
        assert!(!summary.contains("Python model"));

        // JSON: Standard carries both fields; Summary drops them.
        let standard_json = serde_json::to_string(&card.projected(CardDetail::Standard)).unwrap();
        assert!(standard_json.contains("\"python_model\":\"gmeow_models.lifecycle.Foo\""));
        assert!(standard_json.contains("\"python_snippet\":"));
        let summary_json = serde_json::to_string(&card.projected(CardDetail::Summary)).unwrap();
        assert!(!summary_json.contains("python_model"));
        assert!(!summary_json.contains("python_snippet"));

        // A non-class card carries neither field, so no Python section renders.
        let plain = Card {
            category: "Property".to_string(),
            ..sample()
        };
        assert!(!render_card_body(&plain, CardDetail::Standard).contains("Python model"));
    }

    #[test]
    fn toon_scalar_str_escapes_carriage_return_and_backslash() {
        // A lone `\r` and a `\` must both force quoting AND be escaped —
        // otherwise TOON's line-oriented, indentation-based format would
        // either emit a raw CR inside a bare token or leave a backslash
        // ambiguous with the escape sequences that follow it.
        assert_eq!(
            toon_scalar_str("line1\rline2\\tail"),
            "\"line1\\rline2\\\\tail\""
        );
    }
}
