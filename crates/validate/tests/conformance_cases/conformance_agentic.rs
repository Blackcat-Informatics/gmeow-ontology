// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_agentic.py
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

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::double_valued_toolcall_violates_the_closed_world_twins(
    Case::inline("\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/bad/> .
ex:t1 a gmeow:SoftwareAgent .
ex:t2 a gmeow:SoftwareAgent .
ex:fat a gmeow:ToolCall ;
    gmeow:usedTool ex:t1, ex:t2 ;
    gmeow:toolArguments \"a\", \"b\" .
")
        .with_ontology()
        .fails()
        .violations(&["exactly one tool agent", "arguments payload"])
)]
fn agentic(#[case] case: Case) {
    case.run();
}
