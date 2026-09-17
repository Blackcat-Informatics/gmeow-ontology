// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers kept outside the selected production source closure.

use super::*;

impl FanoutRule {
    /// Whether this is an `opaque`-family row (a byte-exact archive member, not a
    /// named-graph fold). Crate-visible so the carrier's emit-side tests can assert the
    /// gate's OWN reader recovered the opaque rows it emitted.
    #[cfg(test)]
    pub(crate) fn is_opaque(&self) -> bool {
        self.family == FanoutFamily::Opaque
    }

    /// The committed path (exact) or directory prefix this rule matches.
    #[cfg(test)]
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
}
