// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::io::Write as _;

#[test]
fn streamed_input_digest_matches_the_action_key_framing_across_chunks() {
    let mut file = tempfile::NamedTempFile::new().expect("temporary input");
    let bytes = vec![0xa5_u8; 2 * 1024 * 1024 + 17];
    file.write_all(&bytes).expect("write multi-chunk input");
    file.flush().expect("flush input");
    assert_eq!(
        super::digest_input_file(file.path()).expect("stream digest"),
        crate::cache::content_digest(&[&bytes]),
    );
}
