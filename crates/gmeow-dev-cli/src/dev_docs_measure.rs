// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev docs-measure` — print the measured, deterministic per-format
//! documentation byte sizes and the three external-distribution design
//! totals (`gmeow_pipeline::docs_measure::measure_docs_designs`).

use crate::dev_common::{fail, project_root};

/// `gmeow-dev docs-measure`.
pub fn docs_measure() -> i32 {
    let root = project_root();
    let measurements = match gmeow_pipeline::docs_measure::measure_docs_designs(&root) {
        Ok(measurements) => measurements,
        Err(e) => return fail(format!("docs-measure failed: {e}")),
    };

    println!(
        "{:<12} {:<14} {:>16} {:>16}",
        "format", "family", "uncompressed", "l12-framed"
    );
    for format in &measurements.formats {
        println!(
            "{:<12} {:<14} {:>16} {:>16}",
            format.format_name, format.family, format.uncompressed_bytes, format.l12_bytes
        );
    }
    println!();
    println!(
        "design-a (external + manifest): {} bytes",
        measurements.design_a_bytes
    );
    println!(
        "design-b (sidecar .gts):        {} bytes",
        measurements.design_b_bytes
    );
    println!(
        "design-c (embedded profile, analytical proxy): {} bytes",
        measurements.design_c_bytes
    );
    0
}
