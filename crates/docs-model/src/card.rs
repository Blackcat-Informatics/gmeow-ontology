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
//! The renderers have no I/O or graph dependency. Native JSON records and writers
//! own the machine representations; callers supply resolved plain data.

use purrdf_lex::json::record::ToJson;
use purrdf_lex::json::{self, Object, Value};

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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardFixture {
    /// The fixture's human label / title.
    pub title: String,
    /// A short body (the fixture body, one-lined and capped) or a reference.
    pub body: String,
}

/// One diagnostic finding the term may hit (full tier): the finding code and a
/// short note. Field order is fixed for deterministic serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardDiagnostic {
    /// The finding code (the stable identifier of the diagnostic).
    pub code: String,
    /// A short note describing the diagnostic incidence.
    pub note: String,
}

/// One projection-loss row for the term (full tier): the projection target the
/// term degrades into and the preservation judgment for that degradation. Field
/// order is fixed for deterministic serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardLoss {
    /// The projection target (the lossy projection this term degrades into).
    pub target: String,
    /// The preservation judgment (`logic:preservationKind` local name(s)).
    pub preservation: String,
}

/// The full UNION of fields both the docs-site `DocTerm` and the folded `Term`
/// can carry for one term. Every value is a pre-resolved DISPLAY string (the
/// caller resolves IRIs to local names / CURIEs); the renderer never touches
/// IRIs. Empty vectors and absent optionals are omitted from JSON. Required strings
/// and present empty optional strings remain present.
///
/// The native record gives `format=json` a byte-stable projection: the fixed
/// struct field order is the serialization order, and every field is a `String` /
/// `Option` / `Vec` whose contents the caller has already deterministically
/// ordered. Absent optionals and empty vectors are skipped, so removing populated
/// fields at a leaner tier produces a smaller object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Card {
    /// The vocabulary category, singular and human-cased (`"Class"`,
    /// `"Property"`, `"Individual"`, `"Datatype"`, `"Term"`).
    pub category: String,
    /// The full term IRI.
    pub iri: String,
    /// `rdfs:label`, omitted from the header when it equals the title/CURIE.
    pub label: Option<String>,
    /// The defining slice as a display string (the owning module's local name).
    /// Both sources carry it: the docs side from `DocTerm::owner_slice`, the
    /// folded/MCP side from the documentation graph's `gmeow:docOwnerSlice`.
    /// `None` only when the source genuinely has no slice for this term, and is
    /// rendered as no `slice:` header line — NEVER a blank value.
    pub slice: Option<String>,
    /// `gmeow:graphBoxRole` four-boxes role display names (e.g. `boxTBox`).
    pub box_roles: Vec<String>,
    /// `skos:definition` (falling back to `rdfs:comment`), one-lined.
    pub definition: Option<String>,
    /// `logic:subClassOf`/`logic:subPropertyOf` (canonical) ∪ `rdfs:subClassOf`/
    /// `rdfs:subPropertyOf` (projection) parent display names.
    pub parents: Vec<String>,
    /// `rdfs:domain` display names.
    pub domain: Vec<String>,
    /// `rdfs:range` display names.
    pub range: Vec<String>,
    /// `gmeow:useWhen` prose.
    pub use_when: Vec<String>,
    /// `gmeow:avoidWhen` prose.
    pub avoid_when: Vec<String>,
    /// `gmeow:howToUse` prose.
    pub how_to_use: Vec<String>,
    /// `skos:scopeNote` prose.
    pub scope_notes: Vec<String>,
    /// `skos:example` prose.
    pub examples: Vec<String>,
    /// `logic:*` stereotype CURIEs.
    pub logic_stereotypes: Vec<String>,
    /// Related-term display names (`skos:related` ∪ `gmeow:pairsWith` ∪
    /// `rdfs:seeAlso`).
    pub related_terms: Vec<String>,
    /// `gmeow:useForConsumer` profile display names.
    pub use_for_consumer: Vec<String>,
    /// `gmeow:avoidForConsumer` profile display names.
    pub avoid_for_consumer: Vec<String>,
    /// Alignment facets, each a pre-formatted `predicate=object` display string.
    pub aligns: Vec<String>,
    /// The importable dotted path `gmeow_models.<slice>.<Class>` of the generated
    /// Pydantic model for a MODELED CLASS term — the explicit term→model link
    /// (§19: importing the model IS reading the term). `None` for non-class terms,
    /// which have no generated model. Rides the Standard + Full tiers, not Summary.
    pub python_model: Option<String>,
    /// A COMPACT, deterministic Pydantic snippet for a modeled class term — the
    /// model import plus a `model_validate` of a minimal `@type` payload. `None`
    /// for non-class terms. Token-budgeted (short) so it rides Standard + Full.
    pub python_snippet: Option<String>,
    // ── Full-tier rich panels (populated ONLY for [`CardDetail::Full`]) ─────────
    /// The reasoner entailments documenting the term. Empty for a term with no
    /// entailments, and for every tier below `Full` (never queried there).
    pub entailments: Vec<CardEntailment>,
    /// The well-formed conformance exemplars (the "Do" panel).
    pub fixtures_do: Vec<CardFixture>,
    /// The counter-example conformance fixtures (the "Don't" panel).
    pub fixtures_dont: Vec<CardFixture>,
    /// The diagnostic findings the term may hit.
    pub diagnostics: Vec<CardDiagnostic>,
    /// The projection-loss rows: the targets the term degrades into.
    pub loss: Vec<CardLoss>,
}

impl ToJson for CardEntailment {
    /// Emit every rich-panel member in its declared order.
    fn to_json(&self) -> Value {
        Value::from(
            Object::new()
                .with("rule", self.rule.to_json())
                .with("conclusion", self.conclusion.to_json())
                .with("premises", self.premises.to_json()),
        )
    }
}

impl ToJson for CardFixture {
    /// Emit every rich-panel member in its declared order.
    fn to_json(&self) -> Value {
        Value::from(
            Object::new()
                .with("title", self.title.to_json())
                .with("body", self.body.to_json()),
        )
    }
}

impl ToJson for CardDiagnostic {
    /// Emit every rich-panel member in its declared order.
    fn to_json(&self) -> Value {
        Value::from(
            Object::new()
                .with("code", self.code.to_json())
                .with("note", self.note.to_json()),
        )
    }
}

impl ToJson for CardLoss {
    /// Emit every rich-panel member in its declared order.
    fn to_json(&self) -> Value {
        Value::from(
            Object::new()
                .with("target", self.target.to_json())
                .with("preservation", self.preservation.to_json()),
        )
    }
}

impl ToJson for Card {
    /// Preserve declaration order and omit only absent optionals and empty vectors.
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("category", self.category.to_json());
        object.insert("iri", self.iri.to_json());
        if let Some(value) = &self.label {
            object.insert("label", value.to_json());
        }
        if let Some(value) = &self.slice {
            object.insert("slice", value.to_json());
        }
        if !self.box_roles.is_empty() {
            object.insert("box_roles", self.box_roles.to_json());
        }
        if let Some(value) = &self.definition {
            object.insert("definition", value.to_json());
        }
        if !self.parents.is_empty() {
            object.insert("parents", self.parents.to_json());
        }
        if !self.domain.is_empty() {
            object.insert("domain", self.domain.to_json());
        }
        if !self.range.is_empty() {
            object.insert("range", self.range.to_json());
        }
        if !self.use_when.is_empty() {
            object.insert("use_when", self.use_when.to_json());
        }
        if !self.avoid_when.is_empty() {
            object.insert("avoid_when", self.avoid_when.to_json());
        }
        if !self.how_to_use.is_empty() {
            object.insert("how_to_use", self.how_to_use.to_json());
        }
        if !self.scope_notes.is_empty() {
            object.insert("scope_notes", self.scope_notes.to_json());
        }
        if !self.examples.is_empty() {
            object.insert("examples", self.examples.to_json());
        }
        if !self.logic_stereotypes.is_empty() {
            object.insert("logic_stereotypes", self.logic_stereotypes.to_json());
        }
        if !self.related_terms.is_empty() {
            object.insert("related_terms", self.related_terms.to_json());
        }
        if !self.use_for_consumer.is_empty() {
            object.insert("use_for_consumer", self.use_for_consumer.to_json());
        }
        if !self.avoid_for_consumer.is_empty() {
            object.insert("avoid_for_consumer", self.avoid_for_consumer.to_json());
        }
        if !self.aligns.is_empty() {
            object.insert("aligns", self.aligns.to_json());
        }
        if let Some(value) = &self.python_model {
            object.insert("python_model", value.to_json());
        }
        if let Some(value) = &self.python_snippet {
            object.insert("python_snippet", value.to_json());
        }
        if !self.entailments.is_empty() {
            object.insert("entailments", self.entailments.to_json());
        }
        if !self.fixtures_do.is_empty() {
            object.insert("fixtures_do", self.fixtures_do.to_json());
        }
        if !self.fixtures_dont.is_empty() {
            object.insert("fixtures_dont", self.fixtures_dont.to_json());
        }
        if !self.diagnostics.is_empty() {
            object.insert("diagnostics", self.diagnostics.to_json());
        }
        if !self.loss.is_empty() {
            object.insert("loss", self.loss.to_json());
        }
        Value::from(object)
    }
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
/// conditionally emitted field is optional (absent under its record policy). The rich
/// panels reference `$defs` matching [`CardEntailment`], [`CardFixture`],
/// [`CardDiagnostic`], and [`CardLoss`] (whose subfields are never omitted, so
/// each is `required`). Keeping this beside the type means a field added to `Card`
/// that is not mirrored here is caught by the conformance test that validates a REAL
/// rendered card against this schema.
#[must_use]
pub fn card_json_schema() -> Value {
    let string_array = || {
        Object::new()
            .with("type", "array")
            .with("items", Object::new().with("type", "string"))
    };
    let properties = Object::new()
        .with(
            "category",
            Object::new().with("type", "string").with(
                "description",
                "The vocabulary category (Class, Property, Individual, Datatype, Term).",
            ),
        )
        .with(
            "iri",
            Object::new()
                .with("type", "string")
                .with("description", "The full term IRI."),
        )
        .with("label", Object::new().with("type", "string"))
        .with("slice", Object::new().with("type", "string"))
        .with("box_roles", string_array())
        .with("definition", Object::new().with("type", "string"))
        .with("parents", string_array())
        .with("domain", string_array())
        .with("range", string_array())
        .with("use_when", string_array())
        .with("avoid_when", string_array())
        .with("how_to_use", string_array())
        .with("scope_notes", string_array())
        .with("examples", string_array())
        .with("logic_stereotypes", string_array())
        .with("related_terms", string_array())
        .with("use_for_consumer", string_array())
        .with("avoid_for_consumer", string_array())
        .with("aligns", string_array())
        .with(
            "python_model",
            Object::new().with("type", "string").with(
                "description",
                "The importable dotted path gmeow_models.<slice>.<Class> of the \
                                generated Pydantic model for a class term (the term→model link).",
            ),
        )
        .with(
            "python_snippet",
            Object::new().with("type", "string").with(
                "description",
                "A compact Pydantic import + model_validate snippet for a class term.",
            ),
        )
        .with(
            "entailments",
            Object::new()
                .with("type", "array")
                .with("items", Object::new().with("$ref", "#/$defs/entailment")),
        )
        .with(
            "fixtures_do",
            Object::new()
                .with("type", "array")
                .with("items", Object::new().with("$ref", "#/$defs/fixture")),
        )
        .with(
            "fixtures_dont",
            Object::new()
                .with("type", "array")
                .with("items", Object::new().with("$ref", "#/$defs/fixture")),
        )
        .with(
            "diagnostics",
            Object::new()
                .with("type", "array")
                .with("items", Object::new().with("$ref", "#/$defs/diagnostic")),
        )
        .with(
            "loss",
            Object::new()
                .with("type", "array")
                .with("items", Object::new().with("$ref", "#/$defs/loss")),
        );
    let entailment = Object::new()
        .with("type", "object")
        .with("additionalProperties", false)
        .with(
            "required",
            Value::Array(vec!["rule".into(), "conclusion".into(), "premises".into()]),
        )
        .with(
            "properties",
            Object::new()
                .with("rule", Object::new().with("type", "string"))
                .with("conclusion", Object::new().with("type", "string"))
                .with("premises", string_array()),
        );
    let fixture = Object::new()
        .with("type", "object")
        .with("additionalProperties", false)
        .with(
            "required",
            Value::Array(vec!["title".into(), "body".into()]),
        )
        .with(
            "properties",
            Object::new()
                .with("title", Object::new().with("type", "string"))
                .with("body", Object::new().with("type", "string")),
        );
    let diagnostic = Object::new()
        .with("type", "object")
        .with("additionalProperties", false)
        .with("required", Value::Array(vec!["code".into(), "note".into()]))
        .with(
            "properties",
            Object::new()
                .with("code", Object::new().with("type", "string"))
                .with("note", Object::new().with("type", "string")),
        );
    let loss = Object::new()
        .with("type", "object")
        .with("additionalProperties", false)
        .with(
            "required",
            Value::Array(vec!["target".into(), "preservation".into()]),
        )
        .with(
            "properties",
            Object::new()
                .with("target", Object::new().with("type", "string"))
                .with("preservation", Object::new().with("type", "string")),
        );
    let definitions = Object::new()
        .with("entailment", entailment)
        .with("fixture", fixture)
        .with("diagnostic", diagnostic)
        .with("loss", loss);
    let mut schema = Value::from(
        Object::new()
            .with("$schema", "https://json-schema.org/draft/2020-12/schema")
            .with(
                "$id",
                "https://blackcatinformatics.ca/gmeow/schemas/card.schema.json",
            )
            .with("title", "GMEOW term card")
            .with(
                "description",
                "The neutral, pre-resolved term card the docs `card.json` member \
                        and the live MCP `doc_card format=json` tool serialize.",
            )
            .with("type", "object")
            .with("additionalProperties", false)
            .with(
                "required",
                Value::Array(vec!["category".into(), "iri".into()]),
            )
            .with("properties", properties)
            .with("$defs", definitions),
    );
    schema.sort_keys();
    schema
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
    json::write_pretty(&card.to_json())
}

/// Serialize a card as TOON — the compact, token-oriented notation. Encodes the
/// card's native JSON model with recursively sorted member names, retaining the
/// established root and tabular column order.
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
    let mut value = card.to_json();
    value.sort_keys();
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
            let cells: Vec<String> = fields
                .iter()
                .map(|f| toon_scalar(obj.get(f).expect("uniform check guarantees each field")))
                .collect();
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

#[path = "card.tests.rs"]
#[cfg(test)]
mod tests;

#[path = "card.json.tests.rs"]
#[cfg(test)]
mod json_tests;
