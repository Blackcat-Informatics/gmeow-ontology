// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Architecture-independent framing shared by correspondence certificates.

/// A versioned digest writer whose fields are self-delimiting. Certificate
/// identity must never depend on Rust `Debug`, map iteration, or a serializer's
/// incidental representation.
pub(super) struct StableDigest(blake3::Hasher);

impl StableDigest {
    pub(super) fn new(domain: &str) -> Self {
        let mut value = Self(blake3::Hasher::new());
        value.bytes("domain", domain.as_bytes());
        value
    }

    pub(super) fn bytes(&mut self, label: &str, value: &[u8]) {
        self.0.update(&(label.len() as u64).to_le_bytes());
        self.0.update(label.as_bytes());
        self.0.update(&(value.len() as u64).to_le_bytes());
        self.0.update(value);
    }

    pub(super) fn text(&mut self, label: &str, value: &str) {
        self.bytes(label, value.as_bytes());
    }

    pub(super) fn usize(&mut self, label: &str, value: usize) {
        self.bytes(label, &(value as u64).to_le_bytes());
    }

    pub(super) fn boolean(&mut self, label: &str, value: bool) {
        self.bytes(label, &[u8::from(value)]);
    }

    pub(super) fn finish(self) -> String {
        self.0.finalize().to_hex().to_string()
    }
}
