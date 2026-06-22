// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-case orchestration (`run_case`).
//!
//! Implemented in Task 4: drives the `gmeow_logic` native cores (compile →
//! certify → materialize+explain / foundation → answers → witnesses) into a typed
//! `CaseOutputs`, mirroring the retired Python `logic_runner.run` 1:1 by calling
//! the SAME native functions the PyO3 surface wraps.
