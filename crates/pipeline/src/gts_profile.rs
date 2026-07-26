// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Re-export of the mandatory GMEOW GTS authorship profile.
//!
//! The profile itself lives in the `gmeow-gts-profile` LEAF crate. It cannot live
//! here: `gmeow-pipeline` already depends on `gmeow-math` and `gmeow-music`, both
//! of which author GTS bytes, so pointing them back at `gmeow_pipeline` would be
//! a cargo dependency cycle. This module keeps the in-crate paths
//! (`crate::gts_profile::emit_gmeow_gts`, …) working for the pipeline's own
//! emitters.

pub use gmeow_gts_profile::{
    GmeowGtsWriter, dataset_to_gmeow_gts, emit_gmeow_gts, validate_mandated_frames,
};

#[cfg(test)]
mod tests {
    /// The committed bundle is a pipeline-owned artifact, so its mandated-frame
    /// audit stays with the pipeline crate rather than moving to the leaf.
    #[test]
    fn committed_bundle_uses_the_mandated_frame_profile() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let bytes = std::fs::read(root.join("generated/dist/gmeow.gts"))
            .expect("read committed GMEOW bundle");
        super::validate_mandated_frames(&bytes)
            .expect("committed bundle uses mandated frame profile");
    }
}
