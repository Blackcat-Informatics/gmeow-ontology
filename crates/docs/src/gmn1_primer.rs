// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-1 **teachability primer** — the ~500-token, graph-derived instruction card a
//! fresh model reads to emit conformant GMN.
//!
//! The EPIC scenario 7 defines teachability verbatim: "a fresh model given ONLY the
//! generated ~500-token primer achieves a GATED AST-validity rate on HELD-OUT emission
//! tasks." This module builds that primer as a PROJECTION of the folded ontology graph —
//! never hand-authored prose. Every content row traces to one of exactly three
//! graph-derived sources:
//!
//! * the **record-sigil table** — the `gmeow:GmnSigilRole` individuals (each carries its
//!   concrete `gmeow:gmnSigilGlyph` and `rdfs:label`), which open every GMN record;
//! * the **operator glyph table** — [`gmeow_lang_bridge::resolve_operator_forms`] over the
//!   carrier glyph registry (glyph, `gmeow:gmnFixity`, ASCII alias, denoted-term CURIE),
//!   the exact registry the writer/reader/EBNF share; and
//! * the **repair-loop cards** — the `gmeow:GmnErr` / `gmeow:GmnPatch` / `gmeow:GmnRetract`
//!   term definitions and `gmeow:howToUse` advice, which TEACH the
//!   NL → GMN → `gmn_validate` → `@err`/`@patch` repair workflow (enhancement item 10).
//!
//! The card is assembled in a deterministic CURIE order and truncated at
//! [`crate::llms::GMN1_PRIMER_TOKEN_BUDGET`] via [`crate::llms::estimate_tokens`], DISCLOSING
//! any elided remainder ("N of M … elided", never silent). Because the sigil and repair rows
//! are emitted before the operator rows and always fit, a fresh model always sees the record
//! grammar and the repair loop whole; only the tail of the operator glyph table can elide.
//!
//! ## Where it flows
//!
//! The primer routes through the ONE shared `llms.txt`-family builder ([`crate::llms`]) so
//! all three llms surfaces pick it up without re-authoring it: it appends to the flat
//! `dist/llms.txt` export and to the complete `llms-full.txt` (both the dist tarball and the
//! MCP `llms_full` twin), and it is exposed as a standalone MCP resource
//! (`gmeow://ontology/gmn1-primer`). All of these call [`build_primer`] over the same folded
//! carrier dataset, so the primer cannot silently diverge across surfaces.

use std::collections::BTreeMap;

use gmeow_lang_bridge::{GmnDictionary, resolve_operator_forms};
use gmeow_logic_compile::ingest::{ns_to_prefix, sssom_id};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

use crate::llms::{self, LlmsSection, estimate_tokens};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
const GMEOW_ENGLISH: &str = "x-gmeow-english";

/// The heading the primer section carries in every surface — the anchor export/MCP tests
/// assert on so a dropped primer reds loudly.
pub const PRIMER_HEADING: &str = "GMN-1 emission primer";

/// A defect building the primer: the carrier does not resolve the GMN-1 codebook (dictionary
/// / glyph registry), or the operator-form resolution failed. A missing codebook is a HARD
/// FAIL, never a silently-empty primer (no-optionality: the primer is a required projection of
/// a carrier that ships GMN).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimerError(pub String);

impl std::fmt::Display for PrimerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PrimerError {}

/// The kind of graph source a primer row traces to — the falsifiable "no hand-authored prose"
/// witness the graph-derived test reads: every emitted content line carries one of these, so a
/// line that traced to nothing (invented prose) could not be constructed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimerRowSource {
    /// A `gmeow:GmnSigilRole` individual — keyed by its CURIE.
    Sigil { curie: String },
    /// An operator glyph binding — keyed by its denoted-term CURIE.
    Operator { curie: String, glyph: String },
    /// A repair-loop term card (`gmeow:GmnErr` / `GmnPatch` / `GmnRetract`) — keyed by CURIE.
    Repair { curie: String },
}

/// One graph-derived primer row: the rendered one-line body plus its source provenance and its
/// deterministic sort key (the CURIE). Rows are ordered by `(group, sort_curie)` so the whole
/// card is byte-stable regardless of dataset iteration order.
#[derive(Clone, Debug)]
struct PrimerRow {
    /// The CURIE sort key within the group (sigils and the repair loop are emitted before
    /// operators so they always fit the budget whole; within each group rows sort by this key).
    sort_curie: String,
    /// The rendered markdown bullet body (without the leading `- `).
    body: String,
    /// The graph source this row traces to.
    source: PrimerRowSource,
}

/// The built GMN-1 primer: the graph-derived rows that fit the token budget, the rendered
/// section for the `llms.txt`-family surfaces, and the elision accounting.
#[derive(Clone, Debug)]
pub struct Gmn1Primer {
    /// Every graph-derived row, in emission order, that fit the budget.
    rows: Vec<PrimerRow>,
    /// The intro line (the `gmeow:Gmn1` notation's own first-sentence definition) — traces to
    /// the graph term, rendered as the section's lead bullet.
    intro: String,
    /// The number of operator rows elided to fit the budget (sigils/repair never elide).
    elided_operators: usize,
    /// The total operator rows the graph offered (the elision denominator).
    total_operators: usize,
}

impl Gmn1Primer {
    /// The compact `LlmsSection` for the `llms.txt` / `llms-full.txt` append: the primer heading
    /// and one bullet per surviving row (intro first, then the disclosure line when rows elided).
    #[must_use]
    pub fn section(&self) -> LlmsSection {
        let mut bullets = Vec::with_capacity(self.rows.len() + 2);
        bullets.push(bullet(&self.intro));
        for row in &self.rows {
            bullets.push(bullet(&row.body));
        }
        if self.elided_operators > 0 {
            bullets.push(bullet(&self.elision_line()));
        }
        LlmsSection {
            heading: PRIMER_HEADING.to_string(),
            bullets,
        }
    }

    /// The standalone primer document body for the MCP `gmeow://ontology/gmn1-primer` resource:
    /// the shared llmstxt.org header + the primer section, so the resource is a self-contained,
    /// prompt-ready card.
    #[must_use]
    pub fn resource_text(&self) -> String {
        llms::render_index(
            PRIMER_HEADING,
            &[format!(
                "The ~{}-token teachability primer — a graph-derived projection of the GMN-1 \
                 record sigils, operator glyph table, and repair loop.",
                llms::GMN1_PRIMER_TOKEN_BUDGET
            )],
            &[self.section()],
        )
    }

    /// The elision disclosure line (empty when nothing elided) — the disclose-don't-truncate
    /// leg: the omitted operators remain reachable via the `gmn_explain` MCP tool and the docs
    /// site.
    #[must_use]
    pub fn elision_line(&self) -> String {
        if self.elided_operators == 0 {
            return String::new();
        }
        format!(
            "{} of {} operator glyphs elided to fit the {}-token primer budget; \
             resolve any omitted operator via the MCP `gmn_explain` tool or the glyph table.",
            self.elided_operators,
            self.total_operators,
            llms::GMN1_PRIMER_TOKEN_BUDGET
        )
    }

    /// The rendered primer text (the section rendered through the shared bullet path) — the
    /// exact bytes appended to a surface, for the budget/teaching tests.
    #[must_use]
    pub fn rendered(&self) -> String {
        llms::render_section(&self.section())
    }

    /// The number of tokens the rendered primer costs under [`estimate_tokens`].
    #[must_use]
    pub fn token_count(&self) -> usize {
        estimate_tokens(&self.rendered())
    }

    /// Whether the rendered primer fits the [`crate::llms::GMN1_PRIMER_TOKEN_BUDGET`] — the
    /// SEPARATE budget-compliance assertion (distinct from teachability).
    #[must_use]
    pub fn fits_budget(&self) -> bool {
        self.token_count() <= llms::GMN1_PRIMER_TOKEN_BUDGET
    }

    /// Every operator glyph the primer teaches, joined to its `(fixity_local_name, alias)` — the
    /// completeness gate's lookup: a held-out task's operator is TAUGHT iff it appears here with
    /// a non-empty fixity and alias.
    #[must_use]
    pub fn operator_index(&self) -> BTreeMap<String, (String, String)> {
        self.rows
            .iter()
            .filter_map(|r| match &r.source {
                PrimerRowSource::Operator { glyph, .. } => parse_operator_body(&r.body)
                    .map(|(fixity, alias)| (glyph.clone(), (fixity, alias))),
                _ => None,
            })
            .collect()
    }

    /// Every record sigil glyph the primer teaches (e.g. `@c`, `@ℒ`) — the completeness gate's
    /// sigil lookup.
    #[must_use]
    pub fn sigil_glyphs(&self) -> std::collections::BTreeSet<String> {
        self.rows
            .iter()
            .filter_map(|r| match &r.source {
                PrimerRowSource::Sigil { .. } => sigil_glyph_of(&r.body),
                _ => None,
            })
            .collect()
    }

    /// Every content line of the rendered primer that must trace to a graph source — the
    /// falsifiable "no hand-authored prose" surface: the set is exactly the intro line, the
    /// per-row bodies, and the (structural) elision disclosure. A hand-authored line would not
    /// be in any row and would fail the graph-derived test's subset check.
    #[must_use]
    pub fn graph_line_bodies(&self) -> std::collections::BTreeSet<String> {
        let mut set = std::collections::BTreeSet::new();
        set.insert(self.intro.clone());
        for row in &self.rows {
            set.insert(row.body.clone());
        }
        if self.elided_operators > 0 {
            set.insert(self.elision_line());
        }
        set
    }

    /// The CURIEs of every graph term/individual the primer cites — the graph-derived test's
    /// positive core-coverage check (`gmeow:GmnErr`, the sigil individuals, the operator
    /// targets).
    #[must_use]
    pub fn cited_curies(&self) -> std::collections::BTreeSet<String> {
        self.rows
            .iter()
            .map(|r| match &r.source {
                PrimerRowSource::Sigil { curie }
                | PrimerRowSource::Operator { curie, .. }
                | PrimerRowSource::Repair { curie } => curie.clone(),
            })
            .collect()
    }
}

/// A linkless llms bullet carrying a whole pre-rendered body in its `text` field (no signature,
/// no note) — the primer builds each row's markdown itself so provenance stays row-addressable.
fn bullet(text: &str) -> llms::LlmsBullet {
    llms::LlmsBullet {
        text: text.to_string(),
        url: None,
        signature: String::new(),
        note: String::new(),
    }
}

/// The CURIE of an IRI via the canonical prefix registry (the same `sssom_id` the describe
/// cards and canonical-Turtle renderer use), so a primer CURIE and a projection agree.
fn curie(iri: &str) -> String {
    sssom_id(iri, ns_to_prefix())
}

/// The local name of a fixity individual IRI (`…/gmnFixityInfix` → `infix`) — the human column
/// of the operator table, matching `gmn_explain`'s `fixity_local_name`.
fn fixity_local(iri: &str) -> String {
    iri.rsplit(['/', '#'])
        .next()
        .unwrap_or(iri)
        .strip_prefix("gmnFixity")
        .map(|s| {
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_lowercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string())
}

/// The one-line rendering of an operator row body: `⊑ (infix, alias subClassOf) — rdfs:subClassOf`.
/// The completeness gate parses it back through [`parse_operator_body`].
fn operator_body(glyph: &str, fixity: &str, alias: &str, term_curie: &str) -> String {
    format!("{glyph} ({fixity}, alias {alias}) — {term_curie}")
}

/// The inverse of [`operator_body`]: recover `(fixity_local, alias)` from a rendered operator
/// row. Returns `None` for a body that is not operator-shaped.
fn parse_operator_body(body: &str) -> Option<(String, String)> {
    let open = body.find(" (")?;
    let close = body[open..].find(')')? + open;
    let inside = &body[open + 2..close];
    let (fixity, rest) = inside.split_once(", alias ")?;
    Some((fixity.trim().to_string(), rest.trim().to_string()))
}

/// The sigil glyph a sigil row body opens with (`@c — claim sigil` → `@c`).
fn sigil_glyph_of(body: &str) -> Option<String> {
    body.split_once(" — ").map(|(g, _)| g.trim().to_string())
}

/// Collapse a multi-sentence graph literal to its first sentence (up to and including the first
/// period-space or trailing period), trimmed — the terse card body that keeps the primer within
/// budget while staying a verbatim graph prefix.
fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(idx) = trimmed.find(". ") {
        return trimmed[..=idx].trim().to_string();
    }
    trimmed.to_string()
}

/// A small graph reader over the carrier's DEFAULT graph — the same scope the describe cards
/// read (the GTS default graph carries the authored, import-free ontology).
struct Reader<'a> {
    ds: &'a RdfDataset,
}

impl<'a> Reader<'a> {
    fn new(ds: &'a RdfDataset) -> Self {
        Self { ds }
    }

    fn iri_id(&self, iri: &str) -> Option<TermId> {
        self.ds.term_id_by_value(&TermValue::iri(iri))
    }

    /// Every default-graph subject IRI of `?s rdf:type <class>`.
    fn instances_of(&self, class_iri: &str) -> Vec<String> {
        let (Some(p), Some(o)) = (self.iri_id(RDF_TYPE), self.iri_id(class_iri)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(None, Some(p), Some(o), GraphMatch::Default)
            .filter_map(|q| match self.ds.resolve(q.s) {
                TermRef::Iri(iri) => Some(iri.to_owned()),
                _ => None,
            })
            .collect()
    }

    /// The preferred literal object of `<subject> <pred> ?o` — the `x-gmeow-english` literal
    /// when present, else the lexically-least literal (deterministic). `None` when absent.
    fn literal(&self, subject_iri: &str, pred: &str) -> Option<String> {
        let (Some(s), Some(p)) = (self.iri_id(subject_iri), self.iri_id(pred)) else {
            return None;
        };
        let mut best: Option<(bool, String)> = None;
        for q in self
            .ds
            .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
        {
            if let TermRef::Literal {
                lexical, language, ..
            } = self.ds.resolve(q.o)
            {
                let is_en = language == Some(GMEOW_ENGLISH);
                let cand = (is_en, lexical.to_owned());
                // Prefer english; among a tie, prefer the lexically-least form (stable).
                let take = match &best {
                    None => true,
                    Some((be, bl)) => {
                        (cand.0, std::cmp::Reverse(cand.1.clone()))
                            > (*be, std::cmp::Reverse(bl.clone()))
                    }
                };
                if take {
                    best = Some(cand);
                }
            }
        }
        best.map(|(_, l)| l)
    }

    /// Every default-graph `rdfs:label` literal keyed by subject IRI (english-preferred) — the
    /// label map [`resolve_operator_forms`] joins operators to.
    fn labels(&self) -> BTreeMap<String, String> {
        let Some(p) = self.iri_id(RDFS_LABEL) else {
            return BTreeMap::new();
        };
        let mut best: BTreeMap<String, (bool, String)> = BTreeMap::new();
        for q in self
            .ds
            .quads_for_pattern(None, Some(p), None, GraphMatch::Default)
        {
            let (
                TermRef::Iri(subject),
                TermRef::Literal {
                    lexical, language, ..
                },
            ) = (self.ds.resolve(q.s), self.ds.resolve(q.o))
            else {
                continue;
            };
            let is_en = language == Some(GMEOW_ENGLISH);
            let cand = (is_en, lexical.to_owned());
            let take = match best.get(subject) {
                None => true,
                Some((be, bl)) => {
                    (cand.0, std::cmp::Reverse(cand.1.clone()))
                        > (*be, std::cmp::Reverse(bl.clone()))
                }
            };
            if take {
                best.insert(subject.to_owned(), cand);
            }
        }
        best.into_iter().map(|(k, (_, v))| (k, v)).collect()
    }
}

/// Build the graph-derived GMN-1 teachability primer over the folded carrier `ds`.
///
/// HARD-FAILS (never a silently-empty primer) when the carrier does not resolve the GMN-1
/// codebook or its operator forms — a carrier that ships GMN must ship the primer's sources.
pub fn build_primer(ds: &RdfDataset) -> Result<Gmn1Primer, PrimerError> {
    let reader = Reader::new(ds);

    // ── The record-sigil table (group 0) — the GmnSigilRole individuals ──────────────────
    let mut sigil_rows: Vec<PrimerRow> = Vec::new();
    for iri in reader.instances_of(&format!("{NAMESPACE}GmnSigilRole")) {
        let (Some(glyph), Some(label)) = (
            reader.literal(&iri, &format!("{NAMESPACE}gmnSigilGlyph")),
            reader.literal(&iri, RDFS_LABEL),
        ) else {
            continue;
        };
        let c = curie(&iri);
        sigil_rows.push(PrimerRow {
            sort_curie: c.clone(),
            body: format!("{glyph} — {label}"),
            source: PrimerRowSource::Sigil { curie: c },
        });
    }
    sigil_rows.sort_by(|a, b| a.sort_curie.cmp(&b.sort_curie));

    // ── The repair-loop cards (group 1) — GmnErr / GmnPatch / GmnRetract ─────────────────
    let mut repair_rows: Vec<PrimerRow> = Vec::new();
    for local in ["GmnErr", "GmnPatch", "GmnRetract"] {
        let iri = format!("{NAMESPACE}{local}");
        let Some(def) = reader.literal(&iri, SKOS_DEFINITION) else {
            continue;
        };
        let how = reader
            .literal(&iri, &format!("{NAMESPACE}howToUse"))
            .map(|h| format!(" {}", first_sentence(&h)))
            .unwrap_or_default();
        let c = curie(&iri);
        repair_rows.push(PrimerRow {
            sort_curie: c.clone(),
            body: format!("{c} — {}{how}", first_sentence(&def)),
            source: PrimerRowSource::Repair { curie: c },
        });
    }
    repair_rows.sort_by(|a, b| a.sort_curie.cmp(&b.sort_curie));

    // ── The operator glyph table (group 2) — the carrier glyph registry ──────────────────
    let dict = GmnDictionary::from_dataset(ds).map_err(|e| {
        PrimerError(format!(
            "resolve the GMN-1 codebook from the carrier: {}",
            e.0
        ))
    })?;
    let labels = reader.labels();
    let forms = resolve_operator_forms(dict.glyph_registry(), &labels)
        .map_err(|e| PrimerError(format!("resolve GMN operator forms: {e}")))?;
    let registry = dict.glyph_registry();
    let mut operator_rows: Vec<PrimerRow> = Vec::new();
    for form in &forms {
        let term_curie = curie(&form.term_iri);
        let fixity = fixity_local(&form.fixity);
        // The ASCII alias (read fallback) for this operator's glyph, from the registry — the
        // typable spelling. When the registry authors none, the alias column is the CURIE local
        // name (still a graph-traced, typable form), never invented.
        let alias = registry
            .fallbacks_for_term(&form.term_iri)
            .into_iter()
            .find(|(_, fb)| !fb.is_empty())
            .map(|(_, fb)| fb.to_string())
            .unwrap_or_else(|| {
                term_curie
                    .rsplit(':')
                    .next()
                    .unwrap_or(&term_curie)
                    .to_string()
            });
        operator_rows.push(PrimerRow {
            sort_curie: term_curie.clone(),
            body: operator_body(&form.gmn_glyph, &fixity, &alias, &term_curie),
            source: PrimerRowSource::Operator {
                curie: term_curie,
                glyph: form.gmn_glyph.clone(),
            },
        });
    }
    operator_rows.sort_by(|a, b| {
        a.sort_curie
            .cmp(&b.sort_curie)
            .then_with(|| a.body.cmp(&b.body))
    });

    // The notation intro — the gmeow:gmnModelNotation (GMN-1) term's own first-sentence
    // definition (graph-traced).
    let intro = reader
        .literal(&format!("{NAMESPACE}gmnModelNotation"), SKOS_DEFINITION)
        .map(|d| first_sentence(&d))
        .ok_or_else(|| {
            PrimerError(
                "carrier has no gmeow:gmnModelNotation (GMN-1) definition to lead the primer"
                    .into(),
            )
        })?;

    // ── Budget-bounded emission: sigils + repair first (always fit), then operators until the
    // running estimate would exceed the budget; disclose the elided operator tail. ──────────
    let total_operators = operator_rows.len();
    let budget = llms::GMN1_PRIMER_TOKEN_BUDGET;

    let mut primer = Gmn1Primer {
        rows: Vec::new(),
        intro,
        elided_operators: 0,
        total_operators,
    };
    // The always-emitted head (sigils + repair) — these define the record grammar and the
    // repair loop; a carrier whose head alone overflows the budget is a HARD FAIL (the primer
    // could not teach the loop), never a silent drop.
    primer.rows.extend(sigil_rows);
    primer.rows.extend(repair_rows);
    if primer.token_count() > budget {
        return Err(PrimerError(format!(
            "the GMN-1 record sigils + repair loop alone cost {} tokens, over the {budget}-token \
             primer budget; the teachability head cannot be truncated",
            primer.token_count()
        )));
    }
    // Greedily add operator rows while the running rendered estimate stays within budget.
    let mut emitted_operators = 0usize;
    for row in operator_rows {
        let mut trial = primer.clone();
        trial.rows.push(row.clone());
        // Reserve room for the disclosure line whenever any operator would remain elided.
        trial.elided_operators = total_operators - (emitted_operators + 1);
        if trial.token_count() > budget {
            break;
        }
        primer.rows.push(row);
        emitted_operators += 1;
    }
    primer.elided_operators = total_operators - emitted_operators;

    Ok(primer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixity_local_strips_the_individual_prefix() {
        assert_eq!(
            fixity_local("https://blackcatinformatics.ca/gmeow/gmnFixityInfix"),
            "infix"
        );
        assert_eq!(
            fixity_local("https://blackcatinformatics.ca/gmeow/gmnFixityPrefix"),
            "prefix"
        );
    }

    #[test]
    fn operator_body_round_trips_through_its_parser() {
        let body = operator_body("⊑", "infix", "subClassOf", "rdfs:subClassOf");
        assert_eq!(
            parse_operator_body(&body),
            Some(("infix".to_string(), "subClassOf".to_string()))
        );
        assert_eq!(sigil_glyph_of("@c — claim sigil"), Some("@c".to_string()));
    }

    #[test]
    fn first_sentence_keeps_a_verbatim_prefix() {
        assert_eq!(
            first_sentence("A GMN @err record: a typed report. And more."),
            "A GMN @err record: a typed report."
        );
        assert_eq!(
            first_sentence("No trailing period marker"),
            "No trailing period marker"
        );
    }
}
