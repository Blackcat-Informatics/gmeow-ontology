// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

impl SuppressionVocab {
    /// An empty synthetic suppression vocabulary for isolated mapping controls.
    pub(crate) fn empty() -> Self {
        Self {
            bearer_props: Vec::new(),
            appellation_domain_props: BTreeSet::new(),
            appellation_classes: BTreeSet::new(),
            coarsen_guarded: BTreeSet::new(),
        }
    }
}
