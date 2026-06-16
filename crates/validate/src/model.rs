// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Namespace IRI constants the validation lints need.
//!
//! PyO3-free. The GMEOW vocabulary namespace itself is NOT a constant here: it
//! is passed in from the Python `config.NAMESPACE` single-source-of-truth
//! (`https://blackcatinformatics.ca/gmeow/`) so the two never drift.

use oxigraph::model::NamedNodeRef;

/// OWL namespace constants (`http://www.w3.org/2002/07/owl#`).
pub mod owl {
    use super::NamedNodeRef;

    /// `owl:sameAs` — the predicate the Principle 5 ban scans for.
    pub const SAME_AS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#sameAs");
}
