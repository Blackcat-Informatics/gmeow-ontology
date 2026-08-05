// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Re-export of the mandatory GMEOW GTS authorship profile.
//!
//! The profile itself lives in the `gmeow-gts-profile` leaf crate so that every
//! bundle author in the workspace can depend on it — including `gmeow-math`, which
//! this crate depends on and therefore cannot depend on this crate. See that
//! crate's documentation for why the narrow waist has to sit below the pipeline.
//!
//! This module keeps the pipeline-local surface (the committed-bundle audit) that
//! is about *this* crate's output rather than about the profile mechanism.

pub use gmeow_gts_profile::{emit_gmeow_gts, validate_mandated_frames};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_bundle_uses_the_mandated_frame_profile() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let bytes = std::fs::read(root.join("generated/dist/gmeow.gts"))
            .expect("read committed GMEOW bundle");
        validate_mandated_frames(&bytes).expect("committed bundle uses mandated frame profile");
    }
}
