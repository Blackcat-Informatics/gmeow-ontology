// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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

/// Self-enforcing coverage gate: every MINT/EXTEND crosswalk row whose
/// verification cell PROMISES competency + counterexample coverage must name at
/// least one `math:` target term that is actually backed by BOTH a competency
/// query and a counter-example fixture. This closes the honesty hole where a
/// row could claim coverage it never shipped (the non-empty check alone let the
/// information-theoretic, Hodge, Mapper, and vector-symbolic families promise
/// coverage while delivering none). The gate is data-driven from the crosswalk
/// plus the two fixture directories, so any FUTURE over-promise also turns red.
#[test]
fn crosswalk_coverage_claims_are_backed_by_query_and_counterexample_terms() {
    let query_terms = math_terms_in_dir(
        &fixture_dir("../../slices/grounding/math/queries/competency"),
        "rq",
    );
    let counterexample_terms = math_terms_in_dir(
        &fixture_dir("../../slices/grounding/math/tests/counter-examples"),
        "ttl",
    );
    let backed: BTreeSet<String> = query_terms
        .intersection(&counterexample_terms)
        .cloned()
        .collect();
    assert!(
        !backed.is_empty(),
        "no math: term is backed by both a competency query and a counter-example; \
         the fixture directories are empty or unreadable"
    );

    let mut lines = CROSSWALK.lines();
    lines
        .find(|line| line.trim() == TABLE_HEADER)
        .expect("external-corpus crosswalk must contain the canonical five-column topic ledger");
    lines
        .next()
        .expect("external-corpus topic ledger must have a Markdown separator row");

    let mut unbacked: Vec<String> = Vec::new();
    for line in lines.take_while(|line| line.trim_start().starts_with('|')) {
        let cells = table_cells(line);
        assert_eq!(
            cells.len(),
            5,
            "external-corpus topic row must have exactly five columns: {line:?}"
        );
        let disposition = cells[1];
        if disposition != "MINT" && disposition != "EXTEND" {
            continue;
        }
        let verification = cells[4].to_ascii_lowercase();
        let claims_coverage = verification.contains("competenc")
            || verification.contains("counterexample")
            || verification.contains("counter-example");
        if !claims_coverage {
            continue;
        }

        let slug = strip_code_span(cells[0])
            .expect("external-corpus topic row must use one nonempty code-span slug");
        let target_terms = math_terms_in_text(cells[2]);
        let has_backed_term = target_terms.iter().any(|term| backed.contains(term));
        if !has_backed_term {
            unbacked.push(slug.to_string());
        }
    }

    assert!(
        unbacked.is_empty(),
        "external-corpus row(s) {:?} claim competency/counterexample coverage but no target \
         term is backed by both a competency query and a counter-example",
        unbacked
    );
}

/// Resolve a fixture directory relative to this crate (`crates/math`) and assert
/// it exists — a missing directory is a HARD FAIL, never a silent skip.
fn fixture_dir(relative: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    assert!(
        path.is_dir(),
        "expected fixture directory to resolve: {}",
        path.display()
    );
    path
}

/// Collect the set of `math:` identifier tokens appearing across every file with
/// the given extension in `dir`. Determinism: file order is sorted and the
/// result is a `BTreeSet`, so pass/fail never depends on `read_dir` ordering.
fn math_terms_in_dir(dir: &Path, extension: &str) -> BTreeSet<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read fixture directory {}: {error}", dir.display()))
        .map(|entry| entry.expect("directory entry must be readable").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(extension))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "fixture directory {} contains no .{extension} files",
        dir.display()
    );

    let mut terms = BTreeSet::new();
    for file in files {
        let contents = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", file.display()));
        terms.extend(math_terms_in_text(&contents));
    }
    terms
}

/// Scan `text` for every `math:` prefix and read the following identifier
/// (ASCII alphanumeric or `_`, up to the first non-identifier char). This gives
/// exact whole-token matching symmetric with the crosswalk parse, so
/// `math:Vector` never matches `math:VectorBinding`.
fn math_terms_in_text(text: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let bytes = text.as_bytes();
    let needle = b"math:";
    let mut index = 0;
    while let Some(found) = text[index..].find("math:") {
        let start = index + found + needle.len();
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end > start {
            terms.insert(text[start..end].to_string());
        }
        index = start;
    }
    terms
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
