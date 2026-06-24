// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_agentic.py (#867)
//!
//! Migrated:
//!   - `test_double_valued_toolcall_violates_the_closed_world_twins`:
//!     uses `run_shacl(_graph() + data)` — merged mode, violation assertion.
//!     Migrated with `validate_with_ontology` (inline Turtle data reproduced
//!     exactly from the Python test).
//!
//! Retained in Python (not migrated):
//!   - `test_example_answers_which_tool_under_which_invocation`: SPARQL SELECT
//!     competency query on a loaded example file — not a `run_shacl` test.
//!   - `test_memory_records_and_reads_tool_calls`: Memory integration test.
//!   - `test_memory_applies_the_verbatim_or_digest_doctrine`: Memory integration
//!     test.
//!   - `test_mcp_triad_is_the_first_live_producer`: MCP producer dogfood test.

mod conformance_support;
use conformance_support::*;

/// `test_double_valued_toolcall_violates_the_closed_world_twins` — the
/// maxCount twins make a double-valued ToolCall record a SHACL violation.
///
/// Mode: `validate_with_ontology` (Python used `_graph() + data` as base —
/// merged ontology required so SHACL class-constraint checks can resolve
/// `gmeow:ToolCall`, `gmeow:SoftwareAgent`, etc.).
#[test]
fn double_valued_toolcall_violates_the_closed_world_twins() {
    let ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/bad/> .
ex:t1 a gmeow:SoftwareAgent .
ex:t2 a gmeow:SoftwareAgent .
ex:fat a gmeow:ToolCall ;
    gmeow:usedTool ex:t1, ex:t2 ;
    gmeow:toolArguments \"a\", \"b\" .
";
    let nt = ttl_str_to_nt(ttl);
    let report = validate_with_ontology(&nt);
    assert!(
        !ok(&report),
        "double-valued ToolCall must fail SHACL; no violations were reported"
    );
    let msgs = violations(&report);
    let text = msgs.join("\n");
    assert!(
        text.contains("exactly one tool agent"),
        "expected 'exactly one tool agent' in violation messages; got: {text:?}"
    );
    assert!(
        text.contains("arguments payload"),
        "expected 'arguments payload' in violation messages; got: {text:?}"
    );
}
