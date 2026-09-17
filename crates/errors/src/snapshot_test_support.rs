// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/// Render-test snapshot helper (U5): a thin wrapper over
/// `insta::assert_snapshot!` so every renderer golden goes through one
/// substrate-owned entry point. The wrapper forwards its tokens verbatim, so
/// the auto-derived snapshot name and rendered body are byte-identical to a
/// direct `insta::assert_snapshot!` call.
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($($tokens:tt)*) => {
        ::insta::assert_snapshot!($($tokens)*)
    };
}
