// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn digests_render_and_validate_in_the_canonical_form() {
    let digest = blake3_digest(b"medium");
    assert!(is_canonical_digest(&digest), "{digest}");
    assert!(!is_canonical_digest("blake3:CAFE"));
    assert!(!is_canonical_digest(&digest.replace("blake3:", "sha256:")));
    // Upper-case hex is NOT canonical: two spellings of one digest would
    // compare unequal while naming the same bytes.
    assert!(!is_canonical_digest(&digest.to_uppercase()));
}
