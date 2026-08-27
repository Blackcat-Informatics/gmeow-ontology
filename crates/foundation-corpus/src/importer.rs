// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `FoundationImporter` — one corpus → one RDF dataset + one budget report.
//!
//! A port of the Python `FoundationImporter`. It builds the dataset over the
//! `purrdf` IR (`RdfDatasetBuilder`) rather than rdflib, and records the
//! flat-vs-reified split in a [`BudgetReport`]. The IRI/term shapes follow the
//! Python so the projections (which navigate the graph) are byte-deterministic
//! against the committed goldens.
//!
//! One deliberate divergence from the Python output: the discourse frame's
//! coordinate axis is typed `gmeow:Axis` (see `import_chapters`), which the
//! Python omitted. An untyped axis passes only fixture-alone SHACL but fails the
//! whole-ontology `FrameProfileShape` (`gmeow:NarrativeTimeFrame ⊑ ReferenceFrame`,
//! `sh:class gmeow:Axis`), so `foundation.ttl` is an intentional correctness
//! improvement over the Python, not a byte-for-byte reproduction of it.

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use purrdf::ir::{RdfDatasetBuilder, TermId};
use purrdf::model::RdfLiteral;
use purrdf::prelude::RdfDataset;

use crate::budget::BudgetReport;
use crate::model::{Record, value_to_index, value_to_iri_component};
use crate::slug::{char_prefix, char_suffix, slug};

// ---------------------------------------------------------------------------
// Namespace + well-known IRI constants.
// ---------------------------------------------------------------------------

/// The GMEOW namespace (mirrors `gmeow_tools.config.NAMESPACE`).
pub const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// The foundation-corpus sub-namespace prefix.
pub const CORP_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/corpus/foundation/";
/// The English-gloss language tag used throughout.
pub const LANG: &str = "x-gmeow-english";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_NNINT: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// `GM.<term>` IRI.
fn gm(term: &str) -> String {
    format!("{NAMESPACE}{term}")
}
/// `CORP[<path>]` IRI.
fn corp(path: &str) -> String {
    format!("{CORP_PREFIX}{path}")
}

/// Corpus role strings → narrative-role seed IRIs (the open vocabulary, P9).
fn role_seed(role: &str) -> Option<String> {
    let term = match role {
        "protagonist" => "roleProtagonist",
        "antagonist" => "roleAntagonist",
        "mentor" => "roleMentor",
        "foil" => "roleFoil",
        "narrator" => "roleNarratingVoice",
        "confidant" => "roleConfidant",
        "love interest" => "roleLoveInterest",
        "trickster" => "roleTrickster",
        _ => return None,
    };
    Some(gm(term))
}

/// Render a decimal in canonical form for the graph value space.
///
/// The Python builds `f"{float(score):.4f}"` then rdflib canonicalizes on output.
/// We store the canonical form directly: strip trailing zeros and a trailing dot.
/// `0.9000 -> "0.9"`, `0.4000 -> "0.4"`, `0.0000 -> "0"`, `-0.0000 -> "0"`.
///
/// The `-0` guard matters because `format!("{:.4}", -0.0)` yields `"-0.0000"`,
/// which trims to `"-0"`; XSD-decimal canonical form has no signed zero.
fn canonical_decimal(value: f64) -> String {
    let s = format!("{value:.4}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Faithful port of Python `float(score)` for a corpus goal-score value.
///
/// Accepts:
/// - `serde_json::Value::Number` → its f64 (Python int/float literal).
/// - `serde_json::Value::String` → trims and parses as f64 (Python accepts
///   numeric strings like "0.9").
///
/// Rejects everything else (bool / null / object / array) and unparsable
/// strings with `ErrorKind::InvalidData`, naming the `goal_id` and raw value.
/// Never silently coerces to 0.0 — zeros are real scores in this corpus.
fn parse_score(goal_id: &str, score: &serde_json::Value) -> io::Result<f64> {
    match score {
        serde_json::Value::Number(n) => n.as_f64().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("goal {goal_id}: score number out of f64 range: {n}"),
            )
        }),
        serde_json::Value::String(s) => s.trim().parse::<f64>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("goal {goal_id}: score string is not a valid number: {s:?}"),
            )
        }),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("goal {goal_id}: score must be a number or numeric string, got: {other}"),
        )),
    }
}

/// The importer: builds the dataset and records the budget.
pub struct FoundationImporter {
    builder: RdfDatasetBuilder,
    /// The budget report (flat vs reified vs skipped).
    pub budget: BudgetReport,
    pipeline_iri: String,
    rubric_iri: String,
    /// Criterion IRIs already minted, keyed by goal id.
    criteria: BTreeMap<String, String>,
    /// Membership index: which `(s,p,o)` triples already exist (for the
    /// `_narrated_event_type` idempotence check the Python does with `in graph`).
    narrated_event_type_minted: bool,
}

impl Default for FoundationImporter {
    fn default() -> Self {
        Self::new()
    }
}

impl FoundationImporter {
    /// Create an empty importer with the corpus-pipeline scaffolding ids.
    pub fn new() -> Self {
        Self {
            builder: RdfDatasetBuilder::new(),
            budget: BudgetReport::new(),
            pipeline_iri: corp("agent/corpus-pipeline"),
            rubric_iri: corp("rubric/principia-goals"),
            criteria: BTreeMap::new(),
            narrated_event_type_minted: false,
        }
    }

    // -- low-level interning helpers ------------------------------------- //

    fn iri(&mut self, iri: &str) -> TermId {
        self.builder.intern_iri(iri)
    }
    fn lang_lit(&mut self, text: &str) -> TermId {
        self.builder
            .intern_literal(RdfLiteral::language_tagged(text, LANG))
    }
    fn plain_lit(&mut self, text: &str) -> TermId {
        self.builder.intern_literal(RdfLiteral::simple(text))
    }
    fn typed_lit(&mut self, lexical: &str, datatype: &str) -> TermId {
        self.builder
            .intern_literal(RdfLiteral::typed(lexical, datatype))
    }

    /// `g.add((s, rdf:type, o))`.
    fn add_type(&mut self, s: &str, ty: &str) {
        let s = self.iri(s);
        let p = self.iri(RDF_TYPE);
        let o = self.iri(ty);
        self.builder.push_quad(s, p, o, None);
    }
    /// `g.add((s, rdfs:label, Literal(text, lang=LANG)))`.
    fn add_label(&mut self, s: &str, text: &str) {
        let s = self.iri(s);
        let p = self.iri(RDFS_LABEL);
        let o = self.lang_lit(text);
        self.builder.push_quad(s, p, o, None);
    }
    /// `g.add((s, p, o))` with `o` an IRI.
    fn add_iri(&mut self, s: &str, p: &str, o: &str) {
        let s = self.iri(s);
        let p = self.iri(p);
        let o = self.iri(o);
        self.builder.push_quad(s, p, o, None);
    }
    /// `g.add((s, p, lang-literal))`.
    fn add_lang(&mut self, s: &str, p: &str, text: &str) {
        let s = self.iri(s);
        let p = self.iri(p);
        let o = self.lang_lit(text);
        self.builder.push_quad(s, p, o, None);
    }

    // -- scaffolding ----------------------------------------------------- //

    fn scaffold(&mut self, source_path: &str) {
        let pipeline = self.pipeline_iri.clone();
        let rubric = self.rubric_iri.clone();
        self.add_type(&pipeline, &gm("SoftwareAgent"));
        self.add_label(&pipeline, "foundation corpus pipeline");

        let activity = corp("activity/import");
        self.add_type(&activity, &gm("ImportActivity"));
        self.add_iri(&activity, &gm("wasAssociatedWith"), &pipeline);
        // Basename only: raw local paths leak usernames/layout (review).
        let basename = if source_path.is_empty() {
            String::new()
        } else {
            std::path::Path::new(source_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        {
            let s = self.iri(&activity);
            let p = self.iri(&gm("sourceLocation"));
            let o = self.plain_lit(&basename);
            self.builder.push_quad(s, p, o, None);
        }

        self.add_type(&rubric, &gm("Rubric"));
        self.add_label(&rubric, "principia goal rubric");
        self.add_iri(&rubric, &gm("normIssuer"), &pipeline);

        let scale = corp("scale/unit");
        self.add_type(&scale, &gm("ScoreScale"));
        {
            let s = self.iri(&scale);
            let p = self.iri(&gm("scaleMin"));
            let o = self.typed_lit("0.0", XSD_DECIMAL);
            self.builder.push_quad(s, p, o, None);
        }
        {
            let s = self.iri(&scale);
            let p = self.iri(&gm("scaleMax"));
            let o = self.typed_lit("1.0", XSD_DECIMAL);
            self.builder.push_quad(s, p, o, None);
        }
        self.add_iri(&rubric, &gm("usesScale"), &scale);
    }

    fn narrated_event_type(&mut self) -> String {
        let iri = corp("event-type/narrated-event");
        if !self.narrated_event_type_minted {
            self.add_type(&iri, &gm("EventType"));
            self.add_label(&iri, "narrated event");
            self.narrated_event_type_minted = true;
        }
        iri
    }

    fn criterion(&mut self, goal_id: &str) -> String {
        if let Some(existing) = self.criteria.get(goal_id) {
            return existing.clone();
        }
        let iri = corp(&format!("criterion/{}", slug(goal_id)));
        self.add_type(&iri, &gm("Criterion"));
        self.add_label(&iri, goal_id);
        let rubric = self.rubric_iri.clone();
        self.add_iri(&rubric, &gm("hasCriterion"), &iri);
        // Named poles are the rubric contract; minted as placeholders.
        let up = corp(&format!("pole/{}-embodiment", slug(goal_id)));
        let down = corp(&format!("pole/{}-antithesis", slug(goal_id)));
        for (pole, label) in [(&up, "embodiment"), (&down, "antithesis")] {
            self.add_type(pole, &gm("CriterionPole"));
            self.add_label(pole, &format!("{goal_id} {label}"));
        }
        self.add_iri(&iri, &gm("rewardPole"), &up);
        self.add_iri(&iri, &gm("penaltyPole"), &down);
        self.criteria.insert(goal_id.to_string(), iri.clone());
        iri
    }

    // -- per-record mapping ---------------------------------------------- //

    /// Map all records (sections first, then books) into the dataset.
    pub fn import_corpus(&mut self, records: &[Record], source_path: &str) -> io::Result<()> {
        self.scaffold(source_path);
        for record in records {
            if record.record_type.as_deref() == Some("section") {
                self.import_section(record);
            }
        }
        for record in records {
            if record.record_type.as_deref() == Some("book") {
                self.import_book(record)?;
            }
        }
        Ok(())
    }

    fn import_section(&mut self, record: &Record) {
        let section_id = record
            .section_id
            .as_deref()
            .expect("section record needs section_id");
        let iri = corp(&format!("section/{}", slug(section_id)));
        self.add_type(&iri, &gm("SocialObject"));
        let label = record.title.as_deref().unwrap_or(section_id);
        self.add_label(&iri, label);
    }

    /// Work-scoped character IRI (the corpus .nq convention).
    fn character_iri(&self, record: &Record, name: &str) -> String {
        let book = book_scope(record);
        corp(&format!("character/{book}/{}", slug(name)))
    }

    fn book_iri(&self, record: &Record) -> String {
        corp(&format!("book/{}", book_scope(record)))
    }

    fn import_book(&mut self, record: &Record) -> io::Result<()> {
        let work = self.book_iri(record);
        let expression = format!("{work}/expression");
        let release = format!("{work}/release");
        let title = record.title.as_deref().expect("book record needs title");

        self.add_type(&work, &gm("Work"));
        self.add_label(&work, title);
        self.add_type(&expression, &gm("Expression"));
        self.add_label(&expression, &format!("{title} (text)"));
        self.add_iri(&expression, &gm("realizes"), &work);
        self.add_type(&release, &gm("BookRelease"));
        self.add_label(&release, &format!("{title} (release)"));
        self.add_iri(&release, &gm("embodies"), &expression);

        if let Some(section_id) = record.section_id.as_deref()
            && !section_id.is_empty()
        {
            let section = corp(&format!("section/{}", slug(section_id)));
            self.add_iri(&work, &gm("partOf"), &section);
        }

        let authors_raw = record.author_s.clone().unwrap_or_default();
        for author in authors_raw.split(" and ") {
            let author = author.trim();
            if !author.is_empty() {
                let agent = corp(&format!("person/{}", slug(author)));
                self.add_type(&agent, &gm("Person"));
                self.add_label(&agent, author);
                self.add_iri(&work, &gm("hasContributor"), &agent);
                self.budget.add_flat("contributor", 1);
            }
        }

        self.import_scores(record, &work)?;
        let (frame, positions, segments) = self.import_chapters(record, &work, &expression);
        self.add_iri(&expression, &gm("hasReferenceFrame"), &frame);
        let characters = self.import_characters(record, &work, &frame, &positions, &segments);
        self.import_concepts(record, &segments);

        // thematic_tags (unpromoted — heuristic): count across chapters.
        let mut tag_total: u64 = 0;
        if let Some(chapters) = &record.corpus_db_chapter_summaries {
            for ch in chapters {
                tag_total += ch.thematic_tags.as_ref().map_or(0, |t| t.len() as u64);
            }
        }
        self.budget
            .add_skipped("thematic_tags (unpromoted — heuristic)", tag_total);

        let _ = characters; // bound for clarity, all uses inline above
        Ok(())
    }

    fn import_scores(&mut self, record: &Record, work: &str) -> io::Result<()> {
        let Some(goals) = &record.corpus_db_primary_goals else {
            return Ok(());
        };
        for (goal_id, score) in goals {
            if goal_id.starts_with('_') {
                continue;
            }
            let assessment = format!("{work}/score/{}", slug(goal_id));
            self.add_type(&assessment, &gm("Assessment"));
            let pipeline = self.pipeline_iri.clone();
            self.add_iri(&assessment, &gm("vantage"), &pipeline);
            self.add_iri(&assessment, &gm("assessmentTarget"), work);
            let criterion = self.criterion(goal_id);
            self.add_iri(&assessment, &gm("assessmentCriterion"), &criterion);
            let rubric = self.rubric_iri.clone();
            self.add_iri(&assessment, &gm("assessmentRubric"), &rubric);
            let value = parse_score(goal_id, score)?;
            let lex = canonical_decimal(value);
            {
                let s = self.iri(&assessment);
                let p = self.iri(&gm("assessmentScoreValue"));
                let o = self.typed_lit(&lex, XSD_DECIMAL);
                self.builder.push_quad(s, p, o, None);
            }
            self.budget
                .add_reified("goal-score assessments (zeros are scores)", 1);
        }
        Ok(())
    }

    /// Returns `(frame_iri, index→position_iri, index→segment_iri)`.
    fn import_chapters(
        &mut self,
        record: &Record,
        work: &str,
        expression: &str,
    ) -> (String, BTreeMap<i64, String>, BTreeMap<i64, String>) {
        let frame = format!("{work}/discourse-frame");
        self.add_type(&frame, &gm("NarrativeTimeFrame"));
        self.add_label(&frame, "discourse order");
        self.add_iri(&frame, &gm("narrativeTimeAxis"), &gm("axisDiscourseTime"));
        self.add_iri(&frame, &gm("discourseTimeOf"), work);
        self.add_iri(&frame, &gm("frameRealm"), &gm("frameRealmNarrative"));
        self.add_iri(&frame, &gm("frameKind"), &gm("frameKindNarrative"));
        // The single coordinate axis of the discourse frame. It MUST be typed
        // gmeow:Axis: gmeow:NarrativeTimeFrame ⊑ gmeow:ReferenceFrame, so the
        // closed-world FrameProfileShape requires every gmeow:hasAxis value to be
        // an Axis (sh:class gmeow:Axis). A bare, untyped axis IRI passes only when
        // SHACL runs fixture-alone (no subclass closure); under the whole-ontology
        // merged validation it is a genuine violation.
        let axis = format!("{frame}/axis");
        self.add_type(&axis, &gm("Axis"));
        self.add_iri(&frame, &gm("hasAxis"), &axis);
        {
            let s = self.iri(&frame);
            let p = self.iri(&gm("dimensionCount"));
            let o = self.typed_lit("1", XSD_NNINT);
            self.builder.push_quad(s, p, o, None);
        }
        {
            let s = self.iri(&frame);
            let p = self.iri(&gm("requiresHost"));
            let o = self.typed_lit("false", XSD_BOOLEAN);
            self.builder.push_quad(s, p, o, None);
        }
        self.add_iri(&frame, &gm("determinacyModel"), &gm("determinacyCrisp"));

        let mut positions: BTreeMap<i64, String> = BTreeMap::new();
        let mut segments: BTreeMap<i64, String> = BTreeMap::new();

        let Some(chapters) = &record.corpus_db_chapter_summaries else {
            return (frame, positions, segments);
        };
        // Clone to detach the borrow; chapters are small.
        let chapters = chapters.clone();
        for chapter in &chapters {
            let index = chapter
                .chapter_index
                .as_ref()
                .and_then(value_to_index)
                .expect("chapter needs chapter_index");
            let pos = format!("{frame}/pos/{index}");
            self.add_type(&pos, &gm("NarrativePosition"));
            self.add_iri(&pos, &gm("positionFrame"), &frame);
            {
                let s = self.iri(&pos);
                let p = self.iri(&gm("positionOrdinal"));
                let o = self.typed_lit(&index.to_string(), XSD_INTEGER);
                self.builder.push_quad(s, p, o, None);
            }
            if let Some(ct) = chapter.chapter_title.as_deref()
                && !ct.is_empty()
            {
                let s = self.iri(&pos);
                let p = self.iri(&gm("positionLabel"));
                let o = self.plain_lit(ct);
                self.builder.push_quad(s, p, o, None);
            }
            let segment = format!("{work}/chapter/{index}");
            self.add_type(&segment, &gm("ContentSegment"));
            let seg_label = chapter
                .chapter_title
                .clone()
                .unwrap_or_else(|| index.to_string());
            self.add_label(&segment, &seg_label);
            self.add_iri(&segment, &gm("segmentOf"), expression);
            self.add_iri(&segment, &gm("atNarrativePosition"), &pos);
            positions.insert(index, pos.clone());
            segments.insert(index, segment.clone());

            if let Some(events) = &chapter.key_events {
                for (i, event_text) in events.iter().enumerate() {
                    let event_no = i + 1;
                    let event = format!(
                        "{segment}/event/{event_no}-{}",
                        slug(&char_prefix(event_text, 48))
                    );
                    self.add_type(&event, &gm("Event"));
                    self.add_lang(&event, RDFS_LABEL, &char_prefix(event_text, 120));
                    let et = self.narrated_event_type();
                    self.add_iri(&event, &gm("eventType"), &et);
                    self.add_iri(&segment, &gm("narrates"), &event);
                    self.budget.add_flat("narrates → key event", 1);
                }
            }
            if let Some(actives) = &chapter.active_characters {
                for name in actives {
                    let char_iri = self.character_iri(record, name);
                    self.add_iri(&segment, &gm("narrates"), &char_iri);
                    self.budget.add_flat("narrates → active character", 1);
                }
            }
        }
        (frame, positions, segments)
    }

    fn import_characters(
        &mut self,
        record: &Record,
        work: &str,
        frame: &str,
        positions: &BTreeMap<i64, String>,
        segments: &BTreeMap<i64, String>,
    ) -> BTreeMap<String, String> {
        let mut characters: BTreeMap<String, String> = BTreeMap::new();
        let chars = record.corpus_db_characters.clone().unwrap_or_default();
        for char in &chars {
            let name = char.name.as_deref().expect("character needs name");
            let iri = self.character_iri(record, name);
            characters.insert(name.to_string(), iri.clone());
            self.add_type(&iri, &gm("Person"));
            self.add_label(&iri, name);

            if let Some(appearances) = &char.chapter_appearances {
                for idx_v in appearances {
                    if let Some(index) = value_to_index(idx_v)
                        && let Some(segment) = segments.get(&index)
                    {
                        self.add_iri(&iri, &gm("narratedIn"), segment);
                        self.budget.add_flat("narratedIn ← appearance", 1);
                    }
                }
            }

            let role_text = char.role.as_deref().unwrap_or("").trim().to_lowercase();
            if !role_text.is_empty() {
                let role_value = match role_seed(&role_text) {
                    Some(v) => v,
                    None => {
                        let v = corp(&format!("role/{}", slug(&role_text)));
                        self.add_type(&v, &gm("NarrativeRole"));
                        self.add_label(&v, &role_text);
                        v
                    }
                };
                let claim = format!("{iri}/role-in/{}", char_suffix(&slug(work), 24));
                self.add_type(&claim, &gm("RoleInNarrative"));
                self.add_iri(&claim, &gm("narrativeRoleBearer"), &iri);
                self.add_iri(&claim, &gm("narrativeRoleScope"), work);
                self.add_iri(&claim, &gm("narrativeRoleValue"), &role_value);
                self.budget
                    .add_reified("role claims (scoped, interpretive)", 1);
            }

            if let Some(principia) = &char.exemplar_principia {
                for goal_id in principia {
                    let exemplar = format!("{iri}/exemplifies/{}", slug(goal_id));
                    self.add_type(&exemplar, &gm("Exemplar"));
                    let rubric = self.rubric_iri.clone();
                    self.add_iri(&exemplar, &gm("citingEntity"), &rubric);
                    self.add_iri(&exemplar, &gm("citedEntity"), work);
                    self.add_iri(&exemplar, &gm("citationIntent"), &gm("intentSupports"));
                    self.add_iri(&exemplar, &gm("exemplarSubject"), &iri);
                    self.add_iri(&exemplar, &gm("exemplarPolarity"), &gm("polarityPositive"));
                    if let Some(rationale) = char.exemplar_rationale.as_deref()
                        && !rationale.is_empty()
                    {
                        self.add_lang(&exemplar, &gm("exemplarRationale"), rationale);
                    }
                    let anchor = self.anchor(goal_id, &exemplar);
                    let criterion = self.criterion(goal_id);
                    self.add_iri(&criterion, &gm("hasScoreAnchor"), &anchor);
                    self.budget
                        .add_reified("entity exemplars (exemplarSubject)", 1);
                }
            }
        }
        self.import_arcs(record, &characters, frame, positions);
        characters
    }

    fn anchor(&mut self, goal_id: &str, exemplar: &str) -> String {
        let anchor = format!("{exemplar}/anchor");
        self.add_type(&anchor, &gm("ScoreAnchor"));
        {
            let s = self.iri(&anchor);
            let p = self.iri(&gm("anchorRangeMin"));
            let o = self.typed_lit("0.8", XSD_DECIMAL);
            self.builder.push_quad(s, p, o, None);
        }
        {
            let s = self.iri(&anchor);
            let p = self.iri(&gm("anchorRangeMax"));
            // "1.0" canonicalizes to "1" in rdflib's decimal value space.
            let o = self.typed_lit("1", XSD_DECIMAL);
            self.builder.push_quad(s, p, o, None);
        }
        self.add_lang(
            &anchor,
            &gm("anchorMeaning"),
            &format!("Conduct embodying {goal_id} across the work."),
        );
        self.add_iri(&anchor, &gm("anchorExemplar"), exemplar);
        anchor
    }

    fn import_arcs(
        &mut self,
        record: &Record,
        characters: &BTreeMap<String, String>,
        _frame: &str,
        positions: &BTreeMap<i64, String>,
    ) {
        let chapters = record
            .corpus_db_chapter_summaries
            .clone()
            .unwrap_or_default();
        for chapter in &chapters {
            let Some(index) = chapter.chapter_index.as_ref().and_then(value_to_index) else {
                continue;
            };
            if !positions.contains_key(&index) {
                continue;
            }
            let arcs = chapter.character_arcs.clone().unwrap_or_default();
            for entry in &arcs {
                let name = entry.character_name.as_deref().unwrap_or("");
                let state_text = entry.emotional_state.as_deref().unwrap_or("").trim();
                if name.is_empty() || state_text.is_empty() {
                    self.budget.add_skipped("arc entries without state", 1);
                    continue;
                }
                let subject = characters
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| self.character_iri(record, name));
                let sample = format!("{subject}/sample/{index}/{}", slug(state_text));
                let state = corp(&format!("emotion/{}", slug(state_text)));
                self.add_type(&state, &gm("EmotionType"));
                self.add_label(&state, state_text);
                self.add_type(&sample, &gm("ArcSample"));
                let pipeline = self.pipeline_iri.clone();
                self.add_iri(&sample, &gm("vantage"), &pipeline);
                self.add_iri(&sample, &gm("sampleSubject"), &subject);
                let pos = positions.get(&index).cloned().unwrap();
                self.add_iri(&sample, &gm("samplePosition"), &pos);
                self.add_iri(&sample, &gm("sampleState"), &state);
                if let Some(signals) = &entry.development_signals {
                    for signal in signals {
                        self.add_lang(&sample, &gm("developmentSignalText"), signal);
                    }
                }
                self.budget.add_reified("arc samples (vantage is data)", 1);
            }
        }
    }

    fn import_concepts(&mut self, record: &Record, segments: &BTreeMap<i64, String>) {
        let concepts = record.corpus_db_concepts.clone().unwrap_or_default();
        for concept in &concepts {
            let name = concept.name.as_deref().expect("concept needs name");
            let motif = corp(&format!("motif/{}", slug(name)));
            self.add_type(&motif, &gm("Motif"));
            self.add_label(&motif, name);
            self.add_iri(&motif, &gm("motifKind"), &gm("motifKindTheme"));
            if let Some(appearances) = &concept.chapter_appearances {
                for idx_v in appearances {
                    if let Some(index) = value_to_index(idx_v)
                        && let Some(segment) = segments.get(&index)
                    {
                        self.add_iri(&motif, &gm("motifOccursIn"), segment);
                        self.budget
                            .add_flat("motifOccursIn ← concept appearance", 1);
                    }
                }
            }
        }
    }

    /// Freeze the built dataset.
    pub fn freeze(self) -> std::io::Result<(Arc<RdfDataset>, BudgetReport)> {
        let budget = self.budget.clone();
        let dataset = self
            .builder
            .freeze()
            .map_err(|d| std::io::Error::other(format!("freeze failed: {d:?}")))?;
        Ok((dataset, budget))
    }
}

/// The book-scope component used in IRIs: `book_number` if present, else
/// `_slug(title)`. The Python uses `record.get("book_number", _slug(title))`.
fn book_scope(record: &Record) -> String {
    match &record.book_number {
        Some(v) => value_to_iri_component(v),
        None => {
            let title = record.title.as_deref().unwrap_or("x");
            slug(title)
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests for parse_score.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- canonical_decimal ------------------------------------------------- //

    #[test]
    fn canonical_decimal_canonicalizes() {
        assert_eq!(canonical_decimal(0.9), "0.9");
        assert_eq!(canonical_decimal(0.4), "0.4");
        assert_eq!(canonical_decimal(1.0), "1");
        // Both signed and unsigned zero must canonicalize to "0" (no "-0"):
        // XSD-decimal has no signed zero, and "zeros are scores" here.
        assert_eq!(canonical_decimal(0.0), "0");
        assert_eq!(canonical_decimal(-0.0), "0");
    }

    // -- happy-path -------------------------------------------------------- //

    #[test]
    fn parse_score_accepts_json_number_float() {
        let v = json!(0.9_f64);
        let result = parse_score("P1", &v).expect("should parse");
        assert!((result - 0.9).abs() < 1e-9);
    }

    #[test]
    fn parse_score_accepts_json_number_integer() {
        let v = json!(1_i64);
        let result = parse_score("P2", &v).expect("should parse");
        assert!((result - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_score_accepts_json_number_zero() {
        // Zero is a real score in this corpus — must not be silently swallowed.
        let v = json!(0_i64);
        let result = parse_score("P3", &v).expect("zero is a valid score");
        assert_eq!(result, 0.0);
    }

    #[test]
    fn parse_score_accepts_numeric_string() {
        // Python `float("0.9")` succeeds; so must the Rust port.
        let v = json!("0.9");
        let result = parse_score("P4", &v).expect("numeric string should parse");
        assert!((result - 0.9).abs() < 1e-9);
    }

    #[test]
    fn parse_score_numeric_string_parity_with_number() {
        // The graph value produced for the string "0.9" must equal the value
        // produced for the number 0.9 (the parity case from the reviewer).
        let as_number = parse_score("P5", &json!(0.9_f64)).expect("number");
        let as_string = parse_score("P5", &json!("0.9")).expect("string");
        let lex_number = canonical_decimal(as_number);
        let lex_string = canonical_decimal(as_string);
        assert_eq!(
            lex_number, lex_string,
            "canonical_decimal of 0.9 (number) vs \"0.9\" (string) must match"
        );
    }

    #[test]
    fn parse_score_accepts_trimmed_numeric_string() {
        // Python float() accepts leading/trailing whitespace.
        let v = json!("  0.5  ");
        let result = parse_score("P6", &v).expect("trimmed numeric string");
        assert!((result - 0.5).abs() < 1e-9);
    }

    // -- hard-fail cases --------------------------------------------------- //

    #[test]
    fn parse_score_rejects_non_numeric_string() {
        let v = json!("not-a-number");
        let err = parse_score("P7", &v).expect_err("should fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("P7"));
    }

    #[test]
    fn parse_score_rejects_bool_true() {
        // JSON booleans are NOT numeric in this corpus (Python float(True) = 1.0,
        // but the corpus never sends bools; reject to surface data rot early).
        let v = json!(true);
        let err = parse_score("P8", &v).expect_err("bool should be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("P8"));
    }

    #[test]
    fn parse_score_rejects_null() {
        let v = json!(null);
        let err = parse_score("P9", &v).expect_err("null should be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("P9"));
    }

    #[test]
    fn parse_score_rejects_object() {
        let v = json!({"nested": 0.9});
        let err = parse_score("P10", &v).expect_err("object should be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("P10"));
    }

    #[test]
    fn parse_score_rejects_array() {
        let v = json!([0.9]);
        let err = parse_score("P11", &v).expect_err("array should be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("P11"));
    }
}
