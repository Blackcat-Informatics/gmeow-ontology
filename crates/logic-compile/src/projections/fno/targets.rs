// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit authority for mathematical targets of canonical callable references.

use std::collections::BTreeSet;

/// Borrowed evidence from one admitted physical Module declaration.
///
/// The producer supplies the actual manifest owner, original artifact identity and
/// types declared in that artifact. Mapping annotations and Example declarations
/// cannot construct authority for a canonical Module target.
#[must_use]
pub struct CanonicalModuleTarget<'a> {
    /// Actual manifest owner of the declaring Module artifact.
    pub owner: &'a str,
    /// Repository-relative path of the original declaring artifact.
    pub path: &'a str,
    /// SHA-256 identity of the original declaring artifact bytes.
    pub digest: &'a str,
    /// Named types explicitly declared on this target in that artifact.
    pub types: &'a BTreeSet<String>,
}

/// Physical Module authority supplied by the producer's original-source admission.
pub trait CanonicalModuleTargets {
    /// Resolve one unambiguous declaration in its actual owner's Module artifact.
    ///
    /// # Errors
    ///
    /// Missing, malformed or ambiguous ownership and declarations outside the
    /// actual owner's Module artifacts must fail. An Example is not a Module.
    fn module_target(&self, target: &str) -> gmeow_errors::Result<CanonicalModuleTarget<'_>>;
}

/// Selected target authority; interface fields always stay in their source view.
#[derive(Clone, Copy)]
pub enum CallableTargetTyping<'a> {
    /// Self-contained source input, with no authority from another view.
    SourceLocal,
    /// Required physical Module authority for repository production.
    ///
    /// Every selected target is resolved here, even if its type also occurs in
    /// the interface view. A refusal never falls back to source-local typing.
    CanonicalModules(&'a dyn CanonicalModuleTargets),
}
