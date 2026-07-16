// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;

const CROSSWALK: &str =
    include_str!("../../../slices/grounding/math/design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md");
const GMEOW_BASE: &str = "fe7244a5bf6a202bbfb68a3b9212f31ac53e91dd";
const TABLE_HEADER: &str =
    "| topic | disposition | exact GMEOW target/composition/profile | rationale | verification |";
const ALLOWED_DISPOSITIONS: [&str; 5] = ["REUSE", "COMPOSE", "EXTEND", "MINT", "PROFILE"];

#[test]
fn external_corpus_crosswalk_is_a_complete_pinned_audit() {
    assert!(
        CROSSWALK.contains(GMEOW_BASE),
        "external-corpus crosswalk must pin GMEOW base commit {GMEOW_BASE}"
    );
    assert!(
        !CROSSWALK.contains("https://example.org/math-corpus/"),
        "external-corpus crosswalk must not contain provisional math-corpus IRIs"
    );

    let mut lines = CROSSWALK.lines();
    let header = lines
        .find(|line| line.trim() == TABLE_HEADER)
        .expect("external-corpus crosswalk must contain the canonical five-column topic ledger");
    assert_eq!(header.trim(), TABLE_HEADER);

    let separator = lines
        .next()
        .expect("external-corpus topic ledger must have a Markdown separator row");
    let separator_cells = table_cells(separator);
    assert_eq!(
        separator_cells,
        vec!["---"; 5],
        "external-corpus topic ledger must have exactly five columns"
    );

    let mut topics = BTreeSet::new();
    let mut row_count = 0usize;
    for (offset, line) in lines
        .take_while(|line| line.trim_start().starts_with('|'))
        .enumerate()
    {
        let line_number = offset + 1;
        let cells = table_cells(line);
        assert_eq!(
            cells.len(),
            5,
            "external-corpus topic row {line_number} must have exactly five columns: {line:?}"
        );

        let topic = strip_code_span(cells[0]).unwrap_or_else(|| {
            panic!("external-corpus topic row {line_number} must use one nonempty code-span slug")
        });
        assert!(
            topics.insert(topic),
            "duplicate external-corpus topic slug in the crosswalk: {topic}"
        );
        assert!(
            ALLOWED_DISPOSITIONS.contains(&cells[1]),
            "external-corpus topic {topic} has invalid disposition {:?}",
            cells[1]
        );
        for (column, value) in [
            ("target/composition/profile", cells[2]),
            ("rationale", cells[3]),
            ("verification", cells[4]),
        ] {
            assert!(
                !value.trim().is_empty(),
                "external-corpus topic {topic} has an empty {column} column"
            );
        }
        row_count += 1;
    }

    assert_eq!(
        row_count, 95,
        "external-corpus crosswalk must contain exactly 95 topic data rows"
    );
    assert_eq!(
        topics.len(),
        95,
        "external-corpus crosswalk must contain exactly 95 unique topic slugs"
    );
}

fn table_cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

fn strip_code_span(cell: &str) -> Option<&str> {
    let slug = cell.strip_prefix('`')?.strip_suffix('`')?;
    (!slug.is_empty()).then_some(slug)
}
