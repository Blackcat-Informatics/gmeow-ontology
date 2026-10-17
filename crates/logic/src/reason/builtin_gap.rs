// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Ledgered moded-builtin gaps — the "never silently wrong" terminal for the native
//! numeric builtins.
//!
//! The moded evaluator ([`crate::physical::eval_builtin`]) declines a builtin either
//! because an operand is still unbound in its required mode
//! ([`BuiltinGapKind::Unbound`]) or because it hit a typed `math:` domain fault
//! ([`BuiltinGapKind::Error`] — ÷0, overflow, incommensurable dimensions). The
//! seminaive spine used to collapse both into a single payload-less `gap: &mut bool`,
//! so a refused program named neither the KIND, the operation, nor the operands: a
//! diagnostics producer with no ledger identity (the repo invariant this module
//! restores).
//!
//! Each [`BuiltinGap`] now threads the whole spine intact and terminates here, minted
//! into a [`gmeow_errors::DiagLedger`] the SAME way
//! [`crate::reason::ledger::divergence_diag_ledger`] mints its divergence rows: one
//! [`Diag`] per gap, at a blocking [`Grade`], with a message-independent distinctness
//! `focus` so the projected finding carries a distinct per-kind `finding_iri` /
//! `anchor_iri`. This is the single kind→identity mapping; no parallel ledger is
//! invented.

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};

use crate::physical::{BuiltinGap, BuiltinGapKind};

/// The [`gmeow_errors::StageId`] every builtin-gap witness is attached under — the
/// native moded-builtin refusal producer on the single diagnostics substrate.
const BUILTIN_GAP_STAGE: &str = "reason.builtin-gap";

/// The ASCII unit separator (`U+001F`) joining a gap's structural distinctness fields
/// into a message-independent fingerprint `focus`. It cannot occur in a `math:` class
/// IRI, an operator token, or a rendered operand, so the joined key is unambiguous —
/// the same discipline [`crate::reason::ledger`] uses.
const FOCUS_SEP: &str = "\u{1f}";

/// The `math:` class label carried in the `focus`/message for a pure mode gap
/// (`Unbound` names no domain-failure class).
const MODE_GAP_CLASS: &str = "mode-gap";

/// The blocking [`Grade`] every builtin gap is interned at — the SAME grade the
/// divergence ledger's failing kinds take: an [`Severity::Error`]
/// [`FindingCategory::ContradictionWitness`] at [`Standpoint::Binding`], so the
/// ledger's gate verdict is `Fatal` and the whole program is refused. A moded-builtin
/// gap is never a soft warning: an incomplete native answer is a wrong answer.
#[must_use]
fn builtin_gap_grade() -> Grade {
    Grade::new(
        Severity::Error,
        FindingCategory::ContradictionWitness,
        Standpoint::Binding,
    )
}

/// Render a gap's antecedent bindings `(var, surface)` into a stable, deterministic
/// operand list for the message and the distinctness key.
#[must_use]
fn render_bindings(gap: &BuiltinGap) -> String {
    let mut parts: Vec<String> = gap
        .bindings
        .iter()
        .map(|(name, surface)| format!("{name}={surface}"))
        .collect();
    parts.sort();
    parts.join(", ")
}

/// Mint the single ledgered [`Diag`] for one moded-builtin gap — the ONE shared
/// helper every terminal routes a produced gap through.
///
/// Native operational failures retain their typed cause through `NativeBuiltin`.
/// The ordinary mode/math witness carries:
///
/// * `code` = `reason.builtin-gap.{suffix}` (`unbound` for a mode gap, else the
///   [`BuiltinError`](crate::physical::BuiltinError) kind's kebab suffix), registered
///   via [`register_code`] — so a distinct kind hashes to a distinct `finding_iri`;
/// * `grade` = [`builtin_gap_grade`] (blocking), so the gap gates `Fatal`;
/// * `message` = names the `math:` conformance class (or the mode-gap marker), the
///   rendered operation, and the antecedent operands — so the kind is never anonymous;
/// * `focus` = a message-INDEPENDENT distinctness key over `(math-class, suffix, op,
///   bindings)`, so two gaps of different kinds never hash-cons-merge and each keeps a
///   distinct `anchor_iri`.
#[must_use]
pub(crate) fn builtin_gap_diag(gap: &BuiltinGap) -> Diag {
    if let BuiltinGapKind::NativeFailure(cause) = &gap.kind {
        let source_code = cause.code().unwrap_or("native-operational");
        return Diag::of_kind(crate::error::NativeBuiltin {
            operation: gap.op.clone(),
            cause: cause.clone(),
        })
        .with_focus([source_code, gap.op.as_str(), render_bindings(gap).as_str()].join(FOCUS_SEP));
    }
    let suffix = gap.kind.code_suffix_or_unbound();
    let code = register_code(&format!("reason.builtin-gap.{suffix}"));
    let class = gap.kind.math_class().unwrap_or(MODE_GAP_CLASS);
    let operands = render_bindings(gap);

    let message = match &gap.kind {
        BuiltinGapKind::NativeFailure(_) => {
            unreachable!("native failures retain their typed cause above")
        }
        BuiltinGapKind::Unbound => format!(
            "moded builtin `{}` declined: an operand is unbound in its required mode \
             (antecedents: {operands})",
            gap.op
        ),
        BuiltinGapKind::Error(_) => format!(
            "moded builtin `{}` raised {class}: a typed math conformance failure \
             (antecedents: {operands})",
            gap.op
        ),
    };

    let focus = [class, suffix, gap.op.as_str(), operands.as_str()].join(FOCUS_SEP);
    Diag::new(code, builtin_gap_grade(), message).with_focus(focus)
}

/// Intern a set of moded-builtin gaps into a fresh [`gmeow_errors::DiagLedger`] — one
/// [`builtin_gap_diag`] per gap under [`BUILTIN_GAP_STAGE`].
///
/// This is the terminal projection the demand/dispatch refusal path attaches to: the
/// ledger's [`findings`](DiagLedger::findings) carry a distinct per-kind
/// `finding_iri`/`anchor_iri`, and its [`verdict`](DiagLedger::verdict) is `Fatal`
/// whenever any gap is present (each gap is graded blocking).
#[must_use]
pub(crate) fn builtin_gap_ledger(gaps: &[BuiltinGap]) -> DiagLedger {
    let mut ledger = DiagLedger::new();
    let stage = StageId::new(BUILTIN_GAP_STAGE);
    for gap in gaps {
        ledger.attach(builtin_gap_diag(gap), stage.clone());
    }
    ledger
}

/// Retain every original diagnostic through the production chase refusal.
#[must_use]
pub(crate) fn builtin_gap_refusal(gaps: &[BuiltinGap]) -> Diag {
    Diag::of_kind(crate::error::BuiltinRefusal {
        failures: gaps.iter().map(builtin_gap_diag).collect(),
    })
}

#[path = "builtin_gap.tests.rs"]
#[cfg(test)]
mod tests;
