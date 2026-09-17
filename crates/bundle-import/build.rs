// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared typed production selection; test/debug builds cannot impersonate the producer.

fn main() {
    gmeow_build_inputs::emit_action_identity("GMEOW_BUNDLE_IMPORT_BUILD_FINGERPRINT", false)
        .expect("complete selected action implementation identity");
}
