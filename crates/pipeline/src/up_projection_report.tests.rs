// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::up_projection_gates::{AuditLedger, TierCounts};
use std::collections::BTreeMap;

#[test]
fn headline_is_proved_plus_claimed_with_the_split_disclosed() {
    let mut per_vocab = BTreeMap::new();
    per_vocab.insert(
        "schema".to_owned(),
        TierCounts {
            proved: 2,
            claimed: 5,
            red_excluded: 1,
            unsupported: 3,
        },
    );
    let ledger = AuditLedger {
        totals: TierCounts {
            proved: 2,
            claimed: 5,
            red_excluded: 1,
            unsupported: 3,
        },
        per_vocab,
        gaps: vec!["schema:foo".to_owned()],
    };
    let md = render_audit_markdown(&ledger);
    // headline numerator = proved + claimed = 7 of 11 total.
    assert!(md.contains("Headline: 7/11 target terms liftable (63%)"));
    assert!(md.contains("proved"));
    assert!(md.contains("claimed"));
    assert!(md.contains("red_excluded"));
    assert!(md.contains("Coverage gaps (1 distinct terms)"));
    // No process-flow references (a '#' immediately followed by a digit) leak
    // into the committed doc — Markdown headers use '# ' (hash then space).
    assert!(
        !md.chars()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|w| w[0] == '#' && w[1].is_ascii_digit()),
        "the committed audit doc must carry no issue/PR numbers"
    );
}
