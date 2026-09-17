// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared typed production selection; test/debug builds cannot impersonate the producer.

fn main() {
    gmeow_build_inputs::emit_action_identity("GMEOW_BUILD_FINGERPRINT", true)
        .expect("complete selected action implementation identity");
}
