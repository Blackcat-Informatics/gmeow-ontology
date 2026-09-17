// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only render-cache path probes.

use super::*;

#[cfg(test)]
pub(super) fn site_cache_path(root: &Path, lang: &str, model: &FixtureIdentity) -> PathBuf {
    render_cache_path(root, &render_context("site", Some(lang), model))
}

#[cfg(test)]
pub(super) fn book_cache_path(root: &Path, model: &FixtureIdentity) -> PathBuf {
    render_cache_path(root, &render_context("book", None, model))
}
