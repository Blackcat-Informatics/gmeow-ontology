// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated maintenance publication of the bounded native codec references.

use std::io::Write;

pub(crate) fn run() -> i32 {
    let output = crate::dev_common::project_root()
        .join("crates/logic/src/result_rdf/fixtures/native-receipt-v1.txt");
    let produce = || -> gmeow_errors::Result<usize> {
        let vectors = gmeow_logic::result_rdf::native_receipt_reference_vectors()?;
        let parent = output.parent().ok_or_else(|| {
            crate::error::reasoning("native receipt reference path has no parent")
        })?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        staged.write_all(vectors.as_bytes())?;
        staged.flush()?;
        staged
            .persist(&output)
            .map_err(|cause| gmeow_errors::Diag::from(cause.error))?;
        Ok(vectors.lines().count())
    };
    match produce() {
        Ok(count) => {
            println!(
                "{}: {count} native receipt reference rows",
                output.display()
            );
            0
        }
        Err(cause) => crate::dev_common::fail(format!(
            "native receipt reference {}: {cause}",
            output.display()
        )),
    }
}
