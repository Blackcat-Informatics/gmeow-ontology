// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn parse_test_ledger(body: &str) -> Arc<RdfDataset> {
    let ttl = format!(
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix gmeow: <{GM}> .\n{body}"
    );
    parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("test ledger parses")
}

#[test]
fn github_comment_labels_match_their_exact_url_identities() {
    let store = parse_test_ledger(
        "<https://example.test/review> rdfs:label \
             \"GitHub PR review comment discussion_r12345 by review-bot\" ; \
             gmeow:citingEntity \
             <https://github.com/example/project/pull/7#discussion_r12345> .\n\
             <https://github.com/example/project/issues/8#issuecomment-67890> \
             rdfs:label \"GitHub comment issuecomment-67890 by assistant\" ; \
             gmeow:sourceLocation \
             \"https://github.com/example/project/issues/8#issuecomment-67890\" .",
    );
    validate_github_citation_labels(&store).expect("matching identities are valid");
}

#[test]
fn unknown_github_comment_labels_are_rejected() {
    let label = format!(
        "GitHub PR review comment {} by review-bot",
        UNKNOWN_LABEL_MARKER
    );
    let store = parse_test_ledger(&format!(
        "<https://example.test/review> rdfs:label \"{label}\" ; \
             gmeow:citingEntity \
             <https://github.com/example/project/pull/7#discussion_r12345> ."
    ));
    let error = validate_github_citation_labels(&store).unwrap_err();
    assert!(error.message().contains("unknown GitHub comment identity"));
}

#[test]
fn malformed_or_mismatched_github_comment_labels_are_rejected() {
    for label in [
        "GitHub PR review comment discussion_rbad by review-bot",
        "GitHub PR review comment discussion_r12345 by review-bot",
        "GitHub PR review comment issuecomment-67890 by review-bot",
        "GitHub comment discussion_r67890 by review-bot",
        "GitHub PR review comment missing-identity by review-bot",
    ] {
        let store = parse_test_ledger(&format!(
            "<https://example.test/review> rdfs:label \"{label}\" ; \
                 gmeow:citingEntity \
                 <https://github.com/example/project/pull/7#discussion_r67890> ."
        ));
        assert!(validate_github_citation_labels(&store).is_err(), "{label}");
    }
}

#[test]
fn canonical_github_comment_labels_require_one_exact_url_identity() {
    for locators in [
        "gmeow:citingEntity <https://github.com/example/project/pull/7>",
        "gmeow:citingEntity \
             <https://github.com/example/project/pull/7#discussion_r12345> ; \
             gmeow:sourceLocation \
             <https://github.com/example/project/pull/7#discussion_r67890>",
    ] {
        let store = parse_test_ledger(&format!(
            "<https://example.test/review> rdfs:label \
                 \"GitHub PR review comment discussion_r12345 by review-bot\" ; \
                 {locators} ."
        ));
        assert!(
            validate_github_citation_labels(&store).is_err(),
            "{locators}"
        );
    }
}
