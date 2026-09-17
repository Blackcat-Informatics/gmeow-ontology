// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GTS authorship-profile diagnostic kinds.
//!
//! Every violation of the mandatory frame profile is a HARD fail
//! (no-optionality): a torn CBOR sequence, a missing or unreadable header, a codec
//! catalog that does not name exactly one `zstd-rsyncable` entry, a payload-bearing
//! frame with no transform chain or with a chain that is not exactly the mandated
//! codec, a payload-free frame that nonetheless carries a chain, a bundle with no
//! payload frames at all, or a refusal from the writer itself. Each is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `gts-profile.*`
//! code namespace, so the distribution contract reports on the shared substrate
//! rather than a bare string.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A bundle violates the mandatory GMEOW GTS frame profile, or the one
    /// permitted production emitter refused. The transform contract is a hard
    /// distribution invariant, never a preference: a bundle that does not satisfy
    /// it is not a GMEOW bundle.
    pub struct Profile { message: String }
    code = "gts-profile.frame";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "GTS frame profile: {}", message;
    failure_class = "https://blackcatinformatics.ca/gmeow/GtsFrameProfileViolation";
}

define_diag_kind! {
    /// Selected native snapshot input failed admission. The original typed cause
    /// remains downcastable, including the failed checkpoint or offending term.
    pub struct SnapshotAdmission { cause: purrdf::gts_compose::GtsIngestError }
    code = "gts-profile.snapshot-admission";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "GTS snapshot input admission: {}", cause;
}

/// The complete GTS-profile diagnostic-code catalog, in registration order.
pub const GTS_PROFILE_DIAG_CODES: &[&str] =
    &[Profile::CODE, Archive::CODE, SnapshotAdmission::CODE];

define_diag_kind! {
    /// A required native artifact has no unique authenticated archive member.
    pub struct Archive { message: String }
    code = "gts-profile.archive";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "GTS archive profile: {}", message;
    failure_class = "https://blackcatinformatics.ca/gmeow/BundleArtifactUnreadable";
}

/// Eagerly intern every GTS-profile diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        Profile::register(),
        Archive::register(),
        SnapshotAdmission::register(),
    ]
}

#[path = "error.tests.rs"]
#[cfg(test)]
mod tests;
