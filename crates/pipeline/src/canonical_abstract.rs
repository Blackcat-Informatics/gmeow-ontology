// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic field projection of the one authored GMEOW abstract.
//!
//! The three public/self-description documents remain independently authored
//! documents, but their abstract field is producer-owned. The unified sync run
//! calls this projection before `source_load`, so every downstream consumer sees
//! the exact canonical lexical value and check mode can report byte drift.

use std::path::Path;

/// The sole authored abstract source.
pub const CANONICAL_ABSTRACT_PATH: &str = "metadata/gmeow-abstract.txt";
/// Every document whose abstract field is projected from the canonical source.
pub const ABSTRACT_TARGET_PATHS: &[&str] = &[
    "ontology/gmeow.ttl",
    "metadata/gmeow-self.ttl",
    "CITATION.cff",
];

const ONTOLOGY_PATH: &str = ABSTRACT_TARGET_PATHS[0];
const SELF_DESCRIPTION_PATH: &str = ABSTRACT_TARGET_PATHS[1];
const CITATION_PATH: &str = ABSTRACT_TARGET_PATHS[2];

/// One fully rendered managed field target.
pub(crate) struct ProjectedAbstract {
    pub path: &'static str,
    pub bytes: Vec<u8>,
}

fn error(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "canonical-abstract".to_owned(),
        message: detail.into(),
    })
}

fn read_utf8(root: &Path, relative: &str) -> gmeow_errors::Result<String> {
    std::fs::read_to_string(root.join(relative))
        .map_err(|source| error(format!("read {relative}: {source}")))
}

fn canonical_text(root: &Path) -> gmeow_errors::Result<String> {
    let source = read_utf8(root, CANONICAL_ABSTRACT_PATH)?;
    let text = source.strip_suffix('\n').unwrap_or(&source);
    if text.is_empty()
        || text.trim() != text
        || text.contains('\n')
        || text.contains('\r')
        || source.ends_with("\n\n")
    {
        return Err(error(format!(
            "{CANONICAL_ABSTRACT_PATH} must contain exactly one non-empty, unpadded line and at most one final newline"
        )));
    }
    Ok(text.to_owned())
}

fn turtle_literal(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\t', "\\t");
    format!("\"{escaped}\"@{}", gmeow_errors::abox::X_GMEOW_ENGLISH)
}

fn unique_span(haystack: &str, needle: &str, source: &str) -> gmeow_errors::Result<(usize, usize)> {
    let mut matches = haystack.match_indices(needle);
    let Some((start, _)) = matches.next() else {
        return Err(error(format!("{source} lacks required anchor {needle:?}")));
    };
    if matches.next().is_some() {
        return Err(error(format!(
            "{source} repeats required anchor {needle:?}"
        )));
    }
    Ok((start, start + needle.len()))
}

fn replace_turtle_field(
    input: &str,
    source: &str,
    block_header: &str,
    predicate: &str,
    value: &str,
) -> gmeow_errors::Result<String> {
    let (_, block_start) = unique_span(input, block_header, source)?;
    let block_end = input[block_start..]
        .find("\n\n")
        .map_or(input.len(), |relative| block_start + relative);
    let block = &input[block_start..block_end];
    let prefix = format!("    {predicate} ");
    let mut matches = block.match_indices(&prefix);
    let Some((relative_start, _)) = matches.next() else {
        return Err(error(format!(
            "{source} typed block {block_header:?} lacks {predicate}"
        )));
    };
    if matches.next().is_some() {
        return Err(error(format!(
            "{source} typed block {block_header:?} repeats {predicate}"
        )));
    }
    let start = block_start + relative_start;
    let end = input[start..]
        .find('\n')
        .map_or(input.len(), |relative| start + relative + 1);
    let newline = if input[start..end].ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let replacement = format!("{prefix}{} ;{newline}", turtle_literal(value));
    Ok(format!(
        "{}{}{}",
        &input[..start],
        replacement,
        &input[end..]
    ))
}

fn replace_citation_abstract(input: &str, value: &str) -> gmeow_errors::Result<String> {
    let mut starts = input.match_indices("abstract:");
    let Some((start, _)) = starts.next() else {
        return Err(error(format!("{CITATION_PATH} lacks abstract field")));
    };
    if start != 0 && !input[..start].ends_with('\n') {
        return Err(error(format!(
            "{CITATION_PATH} abstract is not a top-level YAML field"
        )));
    }
    if starts.next().is_some() {
        return Err(error(format!("{CITATION_PATH} repeats abstract field")));
    }
    let first_end = input[start..]
        .find('\n')
        .map_or(input.len(), |relative| start + relative + 1);
    let mut end = first_end;
    for line in input[first_end..].split_inclusive('\n') {
        if line.starts_with([' ', '\t']) || line.trim().is_empty() {
            end += line.len();
        } else {
            break;
        }
    }
    let encoded = serde_json::to_string(value)
        .map_err(|source| error(format!("encode {CITATION_PATH} abstract: {source}")))?;
    let replacement = format!("abstract: {encoded}\n");
    Ok(format!(
        "{}{}{}",
        &input[..start],
        replacement,
        &input[end..]
    ))
}

/// Render all three target documents without writing them.
pub(crate) fn render_targets(root: &Path) -> gmeow_errors::Result<Vec<ProjectedAbstract>> {
    let abstract_text = canonical_text(root)?;
    let ontology = replace_turtle_field(
        &read_utf8(root, ONTOLOGY_PATH)?,
        ONTOLOGY_PATH,
        "<https://blackcatinformatics.ca/gmeow>\n    a owl:Ontology ;\n",
        "dcterms:description",
        &abstract_text,
    )?;
    let self_description = replace_turtle_field(
        &read_utf8(root, SELF_DESCRIPTION_PATH)?,
        SELF_DESCRIPTION_PATH,
        "<https://blackcatinformatics.ca/gmeow>\n    a gmeow:Work ;\n",
        "skos:definition",
        &abstract_text,
    )?;
    let citation = replace_citation_abstract(&read_utf8(root, CITATION_PATH)?, &abstract_text)?;
    Ok(vec![
        ProjectedAbstract {
            path: ONTOLOGY_PATH,
            bytes: ontology.into_bytes(),
        },
        ProjectedAbstract {
            path: SELF_DESCRIPTION_PATH,
            bytes: self_description.into_bytes(),
        },
        ProjectedAbstract {
            path: CITATION_PATH,
            bytes: citation.into_bytes(),
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABSTRACT: &str = "One canonical abstract with a \\\"quote\\\".";

    #[test]
    fn turtle_projection_is_deterministic_and_field_local() {
        let input = "<x>\n    a owl:Ontology ;\n    dcterms:description \"old\"@en ;\n    dcterms:title \"kept\" .\n\n<y> a <Z> .\n";
        let once = replace_turtle_field(
            input,
            "ontology/gmeow.ttl",
            "<x>\n    a owl:Ontology ;\n",
            "dcterms:description",
            ABSTRACT,
        )
        .expect("projection succeeds");
        let twice = replace_turtle_field(
            &once,
            "ontology/gmeow.ttl",
            "<x>\n    a owl:Ontology ;\n",
            "dcterms:description",
            ABSTRACT,
        )
        .expect("second projection succeeds");
        assert_eq!(once, twice);
        assert!(once.contains("dcterms:title \"kept\""));
        assert!(once.contains("@x-gmeow-english"));
        assert_ne!(input.as_bytes(), once.as_bytes(), "drift is byte-visible");
    }

    #[test]
    fn turtle_projection_anchors_subject_and_type_together() {
        let input = "<target>\n    a gmeow:Work ;\n    skos:definition \"old\"@x-gmeow-english ;\n    rdfs:isDefinedBy <target> .\n\n<other>\n    a gmeow:Work ;\n    skos:definition \"kept\"@x-gmeow-english .\n";
        let projected = replace_turtle_field(
            input,
            "metadata/gmeow-self.ttl",
            "<target>\n    a gmeow:Work ;\n",
            "skos:definition",
            ABSTRACT,
        )
        .expect("targeted projection succeeds");

        assert!(projected.contains(&format!(
            "<target>\n    a gmeow:Work ;\n    skos:definition {} ;",
            turtle_literal(ABSTRACT)
        )));
        assert!(projected.contains(
            "<other>\n    a gmeow:Work ;\n    skos:definition \"kept\"@x-gmeow-english ."
        ));
    }

    #[test]
    fn citation_projection_replaces_the_whole_folded_scalar_only() {
        let input = "title: kept\nabstract: >-\n  stale text\n  on two lines\ntype: dataset\n";
        let rendered = replace_citation_abstract(input, ABSTRACT).expect("projection succeeds");
        assert!(rendered.starts_with("title: kept\nabstract: \""));
        assert!(rendered.ends_with("\ntype: dataset\n"));
        assert!(!rendered.contains("stale text"));
        assert_eq!(
            rendered,
            replace_citation_abstract(&rendered, ABSTRACT).expect("fixed point")
        );
    }
}
