// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Serde record model for the JSONL corpus.
//!
//! Every field is optional (the Python uses `dict.get(...)` throughout). The two
//! load-bearing required-in-practice fields (`section_id`, `title`,
//! `chapter_index`, character `name`, concept `name`) are still modeled as
//! `Option`/raw `Value` so a malformed record degrades the way the Python does
//! (a `KeyError` there; here a controlled `expect`/`unwrap` at the use site) rather
//! than failing to deserialize the whole corpus.

use serde::Deserialize;
use serde_json::Value;

/// A single JSONL corpus record (a section or a book; `type` selects).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Record {
    #[serde(default, rename = "type")]
    pub record_type: Option<String>,

    // -- section fields -- //
    #[serde(default)]
    pub section_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,

    // -- book fields -- //
    /// `book_number` may be an int OR a string in the corpus; keep it raw and
    /// render it the way Python does (`str(record["book_number"])` via f-string).
    #[serde(default)]
    pub book_number: Option<Value>,
    #[serde(default, rename = "author_s_")]
    pub author_s: Option<String>,
    /// Goal-id → score map; `serde_json::Map` preserves insertion order (we keep
    /// the `preserve_order` feature on), though ordering is not load-bearing.
    #[serde(default)]
    pub corpus_db_primary_goals: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    pub corpus_db_characters: Option<Vec<Character>>,
    #[serde(default)]
    pub corpus_db_concepts: Option<Vec<Concept>>,
    #[serde(default)]
    pub corpus_db_chapter_summaries: Option<Vec<Chapter>>,
}

/// A character record under `corpus_db_characters`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Character {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    /// Chapter indices the character appears in (ints in the corpus).
    #[serde(default)]
    pub chapter_appearances: Option<Vec<Value>>,
    #[serde(default)]
    pub exemplar_principia: Option<Vec<String>>,
    #[serde(default)]
    pub exemplar_rationale: Option<String>,
}

/// A concept record under `corpus_db_concepts`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Concept {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub chapter_appearances: Option<Vec<Value>>,
}

/// A chapter-summary record under `corpus_db_chapter_summaries`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Chapter {
    #[serde(default)]
    pub chapter_index: Option<Value>,
    #[serde(default)]
    pub chapter_title: Option<String>,
    #[serde(default)]
    pub thematic_tags: Option<Vec<Value>>,
    #[serde(default)]
    pub active_characters: Option<Vec<String>>,
    #[serde(default)]
    pub key_events: Option<Vec<String>>,
    #[serde(default)]
    pub character_arcs: Option<Vec<ArcEntry>>,
}

/// A `character_arcs` entry within a chapter.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ArcEntry {
    #[serde(default)]
    pub character_name: Option<String>,
    #[serde(default)]
    pub emotional_state: Option<String>,
    #[serde(default)]
    pub development_signals: Option<Vec<String>>,
}

/// Render a JSON value the way the Python f-strings/`int(...)` do.
///
/// - For `book_number` used in IRIs: Python interpolates the value directly,
///   so an int `1` becomes `"1"` and a string `"1a"` stays `"1a"`.
pub fn value_to_iri_component(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => {
            // Python str(True) -> "True"; not expected in the corpus but faithful.
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Null => "None".to_string(),
        other => other.to_string(),
    }
}

/// Coerce a JSON value to an integer index (`int(index)` semantics).
pub fn value_to_index(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}
