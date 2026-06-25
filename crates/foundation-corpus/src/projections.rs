// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The six lossy projections, each a faithful port of the Python emitter, plus
//! the deterministic helpers (CSV writer, XML escape, JSON-LD) they need to
//! reproduce the byte-exact goldens.
//!
//! Every artifact is emitted repo-clean: LF line endings and exactly one
//! trailing newline (the project enforces this via pre-commit hooks, and the
//! `csv.reader`/`json.loads`/XML parsers the Python acceptance test used are
//! line-ending agnostic, so structural parity with the Python output holds).
//! CSV uses `QUOTE_MINIMAL` (Python `csv.writer` default quoting). JSON-LD uses
//! a crate-local 2-space ordered writer with no ASCII escaping (Python
//! `json.dumps(..., indent=2, ensure_ascii=False)`). TEI escapes `& < >` only
//! (Python `xml.sax.saxutils.escape`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::graphview::{GraphView, Object};

// -- IRI constants used by the projections (mirror the importer/Python). -- //
const NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

fn gm(term: &str) -> String {
    format!("{NS}{term}")
}

// -- Ordered JSON-LD pretty writer. -------------------------------------- //

enum OrderedJson {
    String(String),
    Array(Vec<OrderedJson>),
    Object(Vec<(&'static str, OrderedJson)>),
}

fn ordered_string(value: impl Into<String>) -> OrderedJson {
    OrderedJson::String(value.into())
}

fn render_ordered_jsonld(value: &OrderedJson) -> String {
    let mut out = String::new();
    render_ordered_value(value, 0, &mut out);
    out.push('\n');
    out
}

fn render_ordered_value(value: &OrderedJson, indent: usize, out: &mut String) {
    match value {
        OrderedJson::String(s) => out.push_str(&serde_json::to_string(s).expect("json string")),
        OrderedJson::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (idx, item) in items.iter().enumerate() {
                push_indent(out, indent + 2);
                render_ordered_value(item, indent + 2, out);
                if idx + 1 != items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push(']');
        }
        OrderedJson::Object(fields) => {
            if fields.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (idx, (key, field_value)) in fields.iter().enumerate() {
                push_indent(out, indent + 2);
                out.push_str(&serde_json::to_string(key).expect("json key"));
                out.push_str(": ");
                render_ordered_value(field_value, indent + 2, out);
                if idx + 1 != fields.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push('}');
        }
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push(' ');
    }
}

// -- CSV (Python csv.writer: \r\n terminator, QUOTE_MINIMAL). -------------- //

/// Quote a CSV field the way Python's `csv.writer` does with the excel dialect
/// (`QUOTE_MINIMAL`): quote only if the field contains the delimiter, the
/// quotechar, `\r`, or `\n`; double any embedded quotechar.
fn csv_field(field: &str) -> String {
    let needs_quote =
        field.contains(',') || field.contains('"') || field.contains('\r') || field.contains('\n');
    if needs_quote {
        let escaped = field.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        field.to_string()
    }
}

/// Render CSV rows with `\n` terminators (including a trailing terminator after
/// the final row). The content matches Python `csv.writer`; only the terminator
/// is LF (repo-clean) instead of the RFC-4180 `\r\n` default.
fn csv_render(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let line: Vec<String> = row.iter().map(|f| csv_field(f)).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

// -- XML escape (xml.sax.saxutils.escape: & < > only). -------------------- //

fn xml_escape(s: &str) -> String {
    // & must be replaced first (saxutils replaces & then < then >).
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ------------------------------------------------------------------------- //
// 1. DraCor co-occurrence edges.
// ------------------------------------------------------------------------- //

/// DraCor-style co-occurrence edges, one row per character pair.
///
/// DECLARED LOSS: frames, vantage, and event co-occurrents drop.
pub fn project_dracor_csv(g: &GraphView) -> String {
    let narrates = gm("narrates");
    let narrated_in = gm("narratedIn");
    let person = gm("Person");

    let mut pairs: BTreeMap<(String, String), u64> = BTreeMap::new();
    // segment universe = subjects(narrates) ∪ objects(narratedIn).
    let mut segments: BTreeSet<String> = g.subjects_of(&narrates);
    segments.extend(g.object_iris_of_predicate(&narrated_in));

    for segment in &segments {
        // members = {objects(segment, narrates)} ∪ {subjects(narratedIn, segment)}
        // filtered to rdf:type Person, sorted.
        let mut members: BTreeSet<String> = g.object_iris(segment, &narrates);
        members.extend(g.subjects_with_object(&narrated_in, segment));
        let members: Vec<String> = members
            .into_iter()
            .filter(|c| g.has_iri(c, RDF_TYPE, &person))
            .collect();
        for i in 0..members.len() {
            for b in &members[i + 1..] {
                *pairs.entry((members[i].clone(), b.clone())).or_insert(0) += 1;
            }
        }
    }

    let mut rows: Vec<Vec<String>> = vec![vec![
        "Source".to_string(),
        "Target".to_string(),
        "Weight".to_string(),
    ]];
    // pairs is a BTreeMap → sorted by (a, b) already.
    for ((a, b), weight) in &pairs {
        rows.push(vec![a.clone(), b.clone(), weight.to_string()]);
    }
    csv_render(&rows)
}

// ------------------------------------------------------------------------- //
// 2. Syuzhet trajectory rows.
// ------------------------------------------------------------------------- //

/// Syuzhet-style trajectory rows (subject, vantage, ordinal, state).
///
/// DECLARED LOSS: states flatten to labels; no valence scalar is invented.
pub fn project_syuzhet_csv(g: &GraphView) -> String {
    let arc_sample = gm("ArcSample");
    let sample_position = gm("samplePosition");
    let position_ordinal = gm("positionOrdinal");
    let sample_state = gm("sampleState");
    let vantage = gm("vantage");
    let sample_subject = gm("sampleSubject");

    // Rows sort lexicographically by (subject, vantage, ordinal, state); the
    // ordinal sorts numerically in Python (tuple of int). We mirror Python's
    // tuple ordering: subject, vantage are strings; ordinal is an int; state is
    // a string.
    let mut rows: Vec<(String, String, i64, String)> = Vec::new();
    for sample in g.subjects_with_object(RDF_TYPE, &arc_sample) {
        let pos = g.value_iri(&sample, &sample_position);
        let ordinal = pos
            .as_deref()
            .and_then(|p| ordinal_int(g, p, &position_ordinal))
            .unwrap_or(-1);
        let state = g.value_iri(&sample, &sample_state);
        let label = state
            .as_deref()
            .and_then(|st| g.value(st, RDFS_LABEL))
            .map(|o| o.as_str().to_string());
        let state_str = label.or(state).unwrap_or_default();
        let subject = g.value_iri(&sample, &sample_subject).unwrap_or_default();
        let vant = g.value_iri(&sample, &vantage).unwrap_or_default();
        rows.push((subject, vant, ordinal, state_str));
    }
    rows.sort();

    let mut csv_rows: Vec<Vec<String>> = vec![vec![
        "subject".to_string(),
        "vantage".to_string(),
        "ordinal".to_string(),
        "state".to_string(),
    ]];
    for (subject, vant, ordinal, state) in rows {
        csv_rows.push(vec![subject, vant, ordinal.to_string(), state]);
    }
    csv_render(&csv_rows)
}

/// Read an `xsd:integer` ordinal off a position (the Python `int(ordinal.toPython())`
/// when the value is a Literal).
fn ordinal_int(g: &GraphView, pos: &str, position_ordinal: &str) -> Option<i64> {
    match g.value(pos, position_ordinal) {
        Some(Object::Literal { lexical, .. }) => lexical.trim().parse::<i64>().ok(),
        _ => None,
    }
}

// ------------------------------------------------------------------------- //
// 3. schema.org Book JSON-LD.
// ------------------------------------------------------------------------- //

/// schema.org Book JSON-LD.
///
/// DECLARED LOSS: WEMI tiers collapse to one Book node; scores, arcs, and frames
/// drop entirely.
pub fn project_schema_jsonld(g: &GraphView) -> String {
    let work_ty = gm("Work");
    let has_contributor = gm("hasContributor");

    let mut books: Vec<(String, OrderedJson)> = Vec::new();
    for work in g.subjects_with_object(RDF_TYPE, &work_ty) {
        let label = g
            .value(&work, RDFS_LABEL)
            .map(|o| o.as_str().to_string())
            .unwrap_or_else(|| work.clone());
        // authors = [str(value(a, label) or a) for a in objects(work, hasContributor)]
        let mut authors: Vec<String> = g
            .object_iris(&work, &has_contributor)
            .into_iter()
            .map(|a| {
                g.value(&a, RDFS_LABEL)
                    .map(|o| o.as_str().to_string())
                    .unwrap_or(a)
            })
            .collect();
        // Insertion order: @type, @id, name, [author].
        let mut fields = vec![
            ("@type", ordered_string("Book")),
            ("@id", ordered_string(work.clone())),
            ("name", ordered_string(label)),
        ];
        if !authors.is_empty() {
            authors.sort();
            let author_arr: Vec<OrderedJson> = authors
                .into_iter()
                .map(|name| {
                    OrderedJson::Object(vec![
                        ("@type", ordered_string("Person")),
                        ("name", ordered_string(name)),
                    ])
                })
                .collect();
            fields.push(("author", OrderedJson::Array(author_arr)));
        }
        books.push((work, OrderedJson::Object(fields)));
    }
    // sorted by @id.
    books.sort_by(|(a, _), (b, _)| a.cmp(b));
    let books: Vec<OrderedJson> = books.into_iter().map(|(_, entry)| entry).collect();

    render_ordered_jsonld(&OrderedJson::Object(vec![
        ("@context", ordered_string("https://schema.org")),
        ("@graph", OrderedJson::Array(books)),
    ]))
}

// ------------------------------------------------------------------------- //
// 4. TEI skeleton.
// ------------------------------------------------------------------------- //

/// TEI skeleton: castList of persons + chapter div per segment.
///
/// DECLARED LOSS: positions flatten to div order; roles/arcs drop.
pub fn project_tei_xml(g: &GraphView) -> String {
    let narrates = gm("narrates");
    let narrated_in = gm("narratedIn");
    let person = gm("Person");
    let content_segment = gm("ContentSegment");
    let at_position = gm("atNarrativePosition");
    let position_ordinal = gm("positionOrdinal");

    // narrated = objects(narrates) ∪ subjects(narratedIn)
    let mut narrated: BTreeSet<String> = g.object_iris_of_predicate(&narrates);
    narrated.extend(g.subjects_of(&narrated_in));

    // people = sorted(str(value(p, label) or p) for p in subjects(rdf:type Person) if p in narrated)
    let mut people: Vec<String> = g
        .subjects_with_object(RDF_TYPE, &person)
        .into_iter()
        .filter(|p| narrated.contains(p))
        .map(|p| {
            g.value(&p, RDFS_LABEL)
                .map(|o| o.as_str().to_string())
                .unwrap_or(p)
        })
        .collect();
    people.sort();

    // segments = sorted((ordinal_int, label) for s in subjects(rdf:type ContentSegment))
    let mut segments: Vec<(i64, String)> = Vec::new();
    for s in g.subjects_with_object(RDF_TYPE, &content_segment) {
        let pos = g.value_iri(&s, &at_position);
        let n = pos
            .as_deref()
            .and_then(|p| ordinal_int(g, p, &position_ordinal))
            .unwrap_or(-1);
        let label = g
            .value(&s, RDFS_LABEL)
            .map(|o| o.as_str().to_string())
            .unwrap_or(s);
        segments.push((n, label));
    }
    segments.sort();

    let cast: String = people
        .iter()
        .map(|p| format!("<castItem><role>{}</role></castItem>", xml_escape(p)))
        .collect();
    let divs: String = segments
        .iter()
        .map(|(n, t)| {
            format!(
                "<div type=\"chapter\" n=\"{n}\"><head>{}</head></div>",
                xml_escape(t)
            )
        })
        .collect();

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<TEI xmlns=\"http://www.tei-c.org/ns/1.0\"><teiHeader/>\
<text><front><castList>{cast}</castList></front><body>{divs}</body></text></TEI>\n"
    )
}

// ------------------------------------------------------------------------- //
// 5. Web Annotation JSON-LD.
// ------------------------------------------------------------------------- //

/// Web Annotation: each flat narrates link as an oa:Annotation.
///
/// DECLARED LOSS: promoted NarrationUsage modes flatten to bodies.
pub fn project_web_annotation_jsonld(g: &GraphView) -> String {
    let narrates = gm("narrates");
    let mut pairs = g.subject_objects(&narrates);
    pairs.sort();

    let annotations: Vec<OrderedJson> = pairs
        .into_iter()
        .map(|(segment, target)| {
            OrderedJson::Object(vec![
                ("@type", ordered_string("Annotation")),
                ("motivation", ordered_string("describing")),
                ("target", ordered_string(segment)),
                ("body", ordered_string(target)),
            ])
        })
        .collect();

    render_ordered_jsonld(&OrderedJson::Object(vec![
        (
            "@context",
            ordered_string("http://www.w3.org/ns/anno.jsonld"),
        ),
        ("@graph", OrderedJson::Array(annotations)),
    ]))
}

// ------------------------------------------------------------------------- //
// 6. Training-corpus manifest.
// ------------------------------------------------------------------------- //

/// Training-corpus manifest: one record per (work, criterion) score.
///
/// DECLARED LOSS: none at the score level; chunk pairing happens downstream.
pub fn project_training_manifest_jsonl(g: &GraphView) -> String {
    let assessment_ty = gm("Assessment");
    let assessment_target = gm("assessmentTarget");
    let assessment_criterion = gm("assessmentCriterion");
    let score_value = gm("assessmentScoreValue");
    let vantage = gm("vantage");

    let mut lines: Vec<String> = Vec::new();
    for assessment in g.subjects_with_object(RDF_TYPE, &assessment_ty) {
        let target = g
            .value_iri(&assessment, &assessment_target)
            .unwrap_or_default();
        let criterion = g.value_iri(&assessment, &assessment_criterion);
        let work_title = target
            .is_empty()
            .then(String::new)
            .or_else(|| g.value(&target, RDFS_LABEL).map(|o| o.as_str().to_string()))
            .unwrap_or_default();
        let criterion_str = criterion
            .as_deref()
            .and_then(|c| g.value(c, RDFS_LABEL).map(|o| o.as_str().to_string()))
            .or(criterion)
            .unwrap_or_default();
        let score = g
            .value(&assessment, &score_value)
            .map(|o| o.as_str().to_string())
            .unwrap_or_default();
        let score_f: f64 = score.parse().unwrap_or(0.0);
        let vant = g.value_iri(&assessment, &vantage).unwrap_or_default();

        // sort_keys=True → BTreeMap-backed map plus a writer with Python's
        // compact separators.
        let mut record: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        record.insert("work".to_string(), json!(target));
        record.insert("work_title".to_string(), json!(work_title));
        record.insert("criterion".to_string(), json!(criterion_str));
        record.insert("score".to_string(), json!(score_f));
        record.insert("vantage".to_string(), json!(vant));
        record.insert("assessment".to_string(), json!(assessment));
        lines.push(serialize_sorted(&record));
    }
    lines.sort();
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Serialize a sorted string-keyed map as a one-line JSON object with
/// `": "`/`", "` separators (Python `json.dumps(..., sort_keys=True)` default
/// separators) and `ensure_ascii=False`.
fn serialize_sorted(record: &BTreeMap<String, serde_json::Value>) -> String {
    // Render compactly with Python's default `, ` / `: ` separators.
    let mut parts: Vec<String> = Vec::with_capacity(record.len());
    for (k, v) in record {
        // keys and string values render via serde_json (handles escaping,
        // ensure_ascii=False is serde_json's default).
        let key = serde_json::to_string(k).expect("json key");
        let val = serde_json::to_string(v).expect("json val");
        parts.push(format!("{key}: {val}"));
    }
    format!("{{{}}}", parts.join(", "))
}

/// The projection table, in emission order (Python `PROJECTIONS` dict order).
pub const PROJECTION_NAMES: [&str; 6] = [
    "dracor.csv",
    "syuzhet.csv",
    "schema-org.jsonld",
    "tei.xml",
    "web-annotation.jsonld",
    "training-manifest.jsonl",
];

/// Run a projection by name. Panics on an unknown name (the table is closed).
pub fn project(name: &str, g: &GraphView) -> String {
    match name {
        "dracor.csv" => project_dracor_csv(g),
        "syuzhet.csv" => project_syuzhet_csv(g),
        "schema-org.jsonld" => project_schema_jsonld(g),
        "tei.xml" => project_tei_xml(g),
        "web-annotation.jsonld" => project_web_annotation_jsonld(g),
        "training-manifest.jsonl" => project_training_manifest_jsonl(g),
        other => panic!("unknown projection: {other}"),
    }
}
