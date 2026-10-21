// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::producer_parity_mismatch_message;

#[test]
fn parity_failure_retains_complete_actual_graph_and_textual_differences() {
    let expected = "<urn:s> <urn:p> \"old\" .\n<urn:s> <urn:q> <urn:o> .\n";
    let actual = "<urn:s> <urn:p> \"é\" .\n<urn:s> <urn:q> <urn:o> .\n";
    let message =
        producer_parity_mismatch_message("tiny", "urn:g", "fixture.ttl", expected, actual);
    assert_eq!(
        message,
        concat!(
            "tiny producer output is not graph-isomorphic to producer fixture fixture.ttl\n",
            "producer graph: urn:g\n",
            "expected canonical graph: bytes=50 quads=2\n",
            "actual canonical graph: bytes=49 quads=2\n",
            "BEGIN ACTUAL CANONICAL N-QUADS bytes=49 quads=2\n",
            "<urn:s> <urn:p> \"é\" .\n<urn:s> <urn:q> <urn:o> .\n",
            "END ACTUAL CANONICAL N-QUADS\n",
            "Canonical-line differences below are textual; blank-node relabeling may contribute differences.\n",
            "BEGIN TEXTUAL EXPECTED-ONLY CANONICAL LINES count=1\n",
            "<urn:s> <urn:p> \"old\" .\n",
            "END TEXTUAL EXPECTED-ONLY CANONICAL LINES\n",
            "BEGIN TEXTUAL ACTUAL-ONLY CANONICAL LINES count=1\n",
            "<urn:s> <urn:p> \"é\" .\n",
            "END TEXTUAL ACTUAL-ONLY CANONICAL LINES\n",
        )
    );
}

#[test]
fn parity_failure_frames_an_empty_actual_graph_without_inventing_payload() {
    let message = producer_parity_mismatch_message(
        "tiny",
        "urn:g",
        "fixture.ttl",
        "<urn:s> <urn:p> <urn:o> .\n",
        "",
    );
    assert!(message.contains(
        "BEGIN ACTUAL CANONICAL N-QUADS bytes=0 quads=0\nEND ACTUAL CANONICAL N-QUADS\n"
    ));
    assert!(message.contains(
        "BEGIN TEXTUAL EXPECTED-ONLY CANONICAL LINES count=1\n<urn:s> <urn:p> <urn:o> .\n"
    ));
    assert!(message.contains("BEGIN TEXTUAL ACTUAL-ONLY CANONICAL LINES count=0\nEND TEXTUAL ACTUAL-ONLY CANONICAL LINES\n"));
}
