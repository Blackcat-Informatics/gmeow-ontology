// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for the per-term previous-release authority.
//!
//! Ordinary synchronization consumes this evidence and never rewrites it. The
//! normal mode advances an existing accepted release boundary; `--bootstrap` is
//! only for the repository's one-time initial authority and refuses an overwrite.
//! The CLI authenticates its O3/full-LTO producer before dispatching this command.

/// Compute and publish the selected checkout's accepted release authority.
///
/// The CLI admits the producer first. Return 0 after a successful fixed-point
/// check and publication (or an unchanged release), or 1 on refusal or failure.
/// `bootstrap` permits only the initial authority and never overwrites one.
pub(crate) fn run(bootstrap: bool) -> i32 {
    let root = crate::dev_common::project_root();

    match gmeow_pipeline::stages::term_manifest::refresh_release_authority(&root, bootstrap) {
        Ok((release, terms, wrote)) => {
            let action = if wrote { "wrote" } else { "kept" };
            println!(
                "{action} {} term records for ontology release {release} at {}",
                terms,
                root.join(gmeow_pipeline::stages::term_manifest::TERM_RELEASE_AUTHORITY_PATH)
                    .display()
            );
            0
        }
        Err(error) => {
            eprintln!("term release authority: {error}");
            1
        }
    }
}
