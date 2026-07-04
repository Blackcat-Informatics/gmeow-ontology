// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-addressed interning.
//!
//! Text is high-volume and highly repetitive; a naive lift of every document would
//! flood the ABox with near-duplicate nodes. The interner keeps one node per distinct
//! content key — structurally identical forms collapse to a single entry, and the
//! realization links fan out from it. Deduplication uses the first-seen form as the
//! canonical representative (stable), keyed by [`Form::content_key`]; it never relies
//! on a derived `Ord`.

use crate::ast::Form;
use std::collections::BTreeMap;

/// A content-addressed store of forms: one entry per distinct content key.
#[derive(Debug, Default)]
pub struct Interner {
    by_key: BTreeMap<String, Form>,
}

impl Interner {
    /// A new, empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a form, returning its content key. Structurally identical forms return
    /// the same key and do not add a second entry; the first-seen form is kept as the
    /// canonical representative.
    pub fn intern(&mut self, form: Form) -> String {
        let key = form.content_key();
        self.by_key.entry(key.clone()).or_insert(form);
        key
    }

    /// The number of distinct forms interned.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Whether the interner holds no forms.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// Look up the canonical form for a content key, if interned.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Form> {
        self.by_key.get(key)
    }
}

/// Deduplicate a vector of forms in place by content key, keeping the first occurrence
/// of each distinct form. This is the free-function counterpart of [`Interner::intern`]
/// and the idiom to prefer over `Vec::sort` + `Vec::dedup`, which would order by the
/// derived `Ord` (variant-declaration order) rather than lexically by content key.
pub fn dedup_by_content_key(forms: &mut Vec<Form>) {
    let mut seen = std::collections::HashSet::new();
    forms.retain(|f| seen.insert(f.content_key()));
}
