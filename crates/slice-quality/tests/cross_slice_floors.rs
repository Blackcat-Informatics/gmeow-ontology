// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Regression coverage for the DISTRIBUTED governance-source authority: a
//! `gmeow:AxisFloorCommitment` / `gmeow:SliceTierFloor` /
//! `gmeow:ProjectionCeilingCommitment` authored in ANY slice's `module.ttl` — not
//! only the canonical rubric slice — must be discovered and enforced by
//! [`gmeow_slice_quality::load_repo_floors`]. Before this fix the crate-level
//! loader read ONLY `slices/core/slice-quality-rubric/module.ttl`
//! (`RUBRIC_MODULE`), so a floor authored anywhere else was silently
//! unenforced — the headline regression test (a) below.
//!
//! Two guard rails ship alongside the widening so it never opens a new hole in
//! the measurement standard itself:
//!  - (b) a cross-file collision on the same `(slice, axis)` commitment key must
//!    name BOTH offending source files, not just an ambiguous individual IRI.
//!  - (c) a CENTRALIZED individual (`gmeow:QualityAxis` / `gmeow:QualityTier` /
//!    `gmeow:ProjectionVocabulary`) authored outside the rubric slice must hard
//!    fail — only DISTRIBUTED governance commitments may be authored anywhere.

use std::path::{Path, PathBuf};

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n";

/// The GMEOW namespace every fixture IRI shares.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// The repo root this crate lives under — used ONLY to copy the real,
/// structurally-complete rubric module bytes into each fixture's temp root (so
/// the fixture never has to reconstruct the full tier-ladder/axis scaffolding).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// A throwaway temp repo root carrying a structurally-complete rubric slice
/// (the real repo's module copied verbatim) plus whatever extra slices a test
/// adds via [`Fixture::add_slice`].
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let mut root = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        root.push(format!(
            "gmeow-cross-slice-floors-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();

        // The canonical rubric slice: copy the real repo's structurally-complete
        // module verbatim, so the measurement standard (tier ladder + axes) loads
        // cleanly without reconstructing that scaffolding by hand in the fixture.
        let rubric_dir = root.join("slices/core/slice-quality-rubric");
        std::fs::create_dir_all(&rubric_dir).unwrap();
        std::fs::write(
            rubric_dir.join("manifest.ttl"),
            format!("{PREFIXES}<{GMEOW}slices/slice-quality-rubric> a gmeow:Slice .\n"),
        )
        .unwrap();
        let real_rubric_module = repo_root().join("slices/core/slice-quality-rubric/module.ttl");
        let bytes = std::fs::read(&real_rubric_module)
            .unwrap_or_else(|e| panic!("real rubric module must read: {e}"));
        std::fs::write(rubric_dir.join("module.ttl"), bytes).unwrap();

        Self { root }
    }

    /// Author a new slice `slices/<group>/<name>/{manifest.ttl,module.ttl}`, with
    /// `module_body` appended to the standard `gmeow:` prefix declaration —
    /// discoverable via its `manifest.ttl` like any real slice.
    fn add_slice(&self, group: &str, name: &str, module_body: &str) {
        let dir = self.root.join("slices").join(group).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.ttl"),
            format!("{PREFIXES}<{GMEOW}slices/{name}> a gmeow:Slice .\n"),
        )
        .unwrap();
        std::fs::write(dir.join("module.ttl"), format!("{PREFIXES}{module_body}")).unwrap();
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// (a) THE HEADLINE REGRESSION: a `gmeow:AxisFloorCommitment` authored in a
/// NON-rubric slice's `module.ttl` must be discovered and enforced by
/// `load_repo_floors`. Before this fix the crate-level loader read ONLY
/// `RUBRIC_MODULE`, so this commitment would be silently absent.
#[test]
fn non_rubric_slice_floor_is_enforced() {
    let f = Fixture::new("headline");
    f.add_slice(
        "widget",
        "demo",
        &format!(
            "gmeow:floorDemoGrounding a gmeow:AxisFloorCommitment ;\n\
             \x20   gmeow:floorSlice <{GMEOW}slices/demo> ;\n\
             \x20   gmeow:floorAxis gmeow:axisMaximalGrounding ;\n\
             \x20   gmeow:floorValue 0.42 .\n"
        ),
    );

    let floors = gmeow_slice_quality::load_repo_floors(f.root())
        .expect("a floor authored in a non-rubric slice must still load cleanly");

    let found = floors.commitments.iter().find(|c| {
        c.slice == format!("{GMEOW}slices/demo") && c.axis == format!("{GMEOW}axisMaximalGrounding")
    });
    assert!(
        found.is_some(),
        "the AxisFloorCommitment authored in slices/widget/demo/module.ttl must be enforced, \
         got commitments: {:?}",
        floors
            .commitments
            .iter()
            .map(|c| (&c.slice, &c.axis))
            .collect::<Vec<_>>()
    );
    assert!(
        (found.unwrap().floor - 0.42).abs() < f64::EPSILON,
        "the loaded floor must carry the authored value"
    );
}

/// (b) A cross-file collision on the SAME `(slice, axis)` commitment key,
/// authored in TWO different slices' `module.ttl`, must hard-fail naming BOTH
/// offending source files — not just an ambiguous individual IRI, since the
/// human resolving the collision needs to know which two slices to look at.
#[test]
fn cross_file_floor_collision_names_both_files() {
    let f = Fixture::new("collision");
    f.add_slice(
        "widget",
        "collide-a",
        &format!(
            "gmeow:floorCollideA a gmeow:AxisFloorCommitment ;\n\
             \x20   gmeow:floorSlice <{GMEOW}slices/collide> ;\n\
             \x20   gmeow:floorAxis gmeow:axisMaximalGrounding ;\n\
             \x20   gmeow:floorValue 0.3 .\n"
        ),
    );
    f.add_slice(
        "widget",
        "collide-b",
        &format!(
            "gmeow:floorCollideB a gmeow:AxisFloorCommitment ;\n\
             \x20   gmeow:floorSlice <{GMEOW}slices/collide> ;\n\
             \x20   gmeow:floorAxis gmeow:axisMaximalGrounding ;\n\
             \x20   gmeow:floorValue 0.9 .\n"
        ),
    );

    let err = gmeow_slice_quality::load_repo_floors(f.root())
        .expect_err("the same (slice, axis) key authored in two different modules must hard fail");
    let message = err.message();
    assert!(
        message.contains("collide-a") && message.contains("collide-b"),
        "the collision message must name BOTH offending slice modules, got: {message}"
    );
}

/// (c) A `gmeow:QualityAxis` (a CENTRALIZED individual) authored OUTSIDE the
/// rubric slice must hard fail — only DISTRIBUTED governance commitments
/// (floors, tier floors, ceilings, exemptions) may be authored in any slice;
/// the measurement standard itself has exactly one authoring boundary.
#[test]
fn centralized_axis_authored_outside_rubric_hard_fails() {
    let f = Fixture::new("rogue-axis");
    f.add_slice(
        "widget",
        "rogue",
        "gmeow:axisRogue a gmeow:QualityAxis ;\n\
         \x20   gmeow:axisProducer \"rogue_producer\" ;\n\
         \x20   gmeow:axisDimension gmeow:qualityDimensionGrounding ;\n\
         \x20   gmeow:axisContextScope gmeow:scopeSliceLocal ;\n\
         \x20   gmeow:axisThreshold gmeow:thrGroundingGrounded .\n",
    );

    let err = gmeow_slice_quality::load_repo_floors(f.root())
        .expect_err("a QualityAxis authored outside the rubric slice must hard fail");
    let message = err.message();
    assert!(
        message.contains("axisRogue"),
        "the centralized-authority violation must name the offending axis, got: {message}"
    );

    // The same guard fires through the whole-rubric loader too.
    let err2 = gmeow_slice_quality::load_repo_rubric(f.root())
        .expect_err("load_repo_rubric must also hard fail on the same violation");
    assert!(err2.message().contains("axisRogue"), "{}", err2.message());
}
