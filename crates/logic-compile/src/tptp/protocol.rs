// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict admission of external-prover SZS status observations.
//!
//! A subprocess transcript is evidence about the exact task it was given, not a
//! native verdict by itself. This parser inventories every status line, rejects
//! conflicts and explicit foreign problem names, and applies the selected task
//! shape before a caller may interpret the observation.

use std::collections::BTreeSet;
use std::fmt;

/// Shape of the submitted TPTP problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FofTask {
    /// A set of FOF axioms with no conjecture, queried for model existence.
    AxiomConsistency,
}

/// Transcript stream carrying an SZS observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SzsTranscriptChannel {
    Stdout,
    Stderr,
}

/// One parsed SZS status line. Line numbers are one-based within the channel.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SzsStatusObservation {
    pub channel: SzsTranscriptChannel,
    pub line: usize,
    pub status: String,
    pub problem: Option<String>,
}

/// Model-theoretic meaning admitted for the selected task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SzsOutcome {
    Consistent,
    Inconsistent,
    Undecided,
}

/// Complete protocol admission. Repeated identical observations remain visible.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SzsAdmission {
    pub task: FofTask,
    pub status: String,
    pub outcome: SzsOutcome,
    pub observations: Vec<SzsStatusObservation>,
}

/// Stable protocol refusal suitable for a typed consumer report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SzsProtocolError {
    pub code: String,
    pub detail: String,
}

impl fmt::Display for SzsProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for SzsProtocolError {}

/// Admit all SZS observations in the selected child's stdout and stderr.
///
/// `expected_problem_names` contains the exact filename/stem aliases supplied to
/// the child. A transcript may omit `for <problem>` because several conforming
/// provers do, but any explicit problem name must name this invocation.
///
/// # Errors
/// Refuses absent/malformed/conflicting status lines, unknown status tokens,
/// explicit foreign problem names and statuses inappropriate for `task`.
pub fn admit_szs_transcript(
    stdout: &str,
    stderr: &str,
    expected_problem_names: &BTreeSet<String>,
    task: FofTask,
) -> Result<SzsAdmission, SzsProtocolError> {
    let mut observations = Vec::new();
    scan_channel(stdout, SzsTranscriptChannel::Stdout, &mut observations)?;
    scan_channel(stderr, SzsTranscriptChannel::Stderr, &mut observations)?;
    let Some(first) = observations.first() else {
        return Err(protocol_error(
            "MISSING_SZS_STATUS",
            "the selected child produced no SZS status observation",
        ));
    };
    if observations
        .iter()
        .any(|observation| observation.status != first.status)
    {
        return Err(protocol_error(
            "CONFLICTING_SZS_STATUS",
            format!(
                "the selected child reported conflicting statuses: {}",
                observations
                    .iter()
                    .map(|observation| observation.status.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    for observation in &observations {
        if let Some(problem) = observation.problem.as_deref()
            && !problem_name_matches(problem, expected_problem_names)
        {
            return Err(protocol_error(
                "FOREIGN_SZS_PROBLEM",
                format!(
                    "status names problem {problem:?}, expected one of {:?}",
                    expected_problem_names
                ),
            ));
        }
    }
    let outcome = outcome_for_task(&first.status, task)?;
    Ok(SzsAdmission {
        task,
        status: first.status.clone(),
        outcome,
        observations,
    })
}

fn scan_channel(
    source: &str,
    channel: SzsTranscriptChannel,
    observations: &mut Vec<SzsStatusObservation>,
) -> Result<(), SzsProtocolError> {
    for (offset, line) in source.lines().enumerate() {
        let Some(rest) = line
            .trim_start()
            .strip_prefix('%')
            .or_else(|| line.trim_start().strip_prefix('#'))
        else {
            continue;
        };
        let mut tokens = rest.split_whitespace();
        if tokens.next() != Some("SZS") || tokens.next() != Some("status") {
            continue;
        }
        let Some(status) = tokens.next() else {
            return Err(protocol_error(
                "MALFORMED_SZS_STATUS",
                format!("{channel:?} line {} has no status token", offset + 1),
            ));
        };
        let remaining: Vec<_> = tokens.collect();
        let problem = remaining
            .windows(2)
            .find(|pair| pair[0] == "for")
            .map(|pair| normalize_problem_name(pair[1]));
        observations.push(SzsStatusObservation {
            channel,
            line: offset + 1,
            status: normalize_status(status),
            problem,
        });
    }
    Ok(())
}

fn outcome_for_task(status: &str, task: FofTask) -> Result<SzsOutcome, SzsProtocolError> {
    match (task, status) {
        (FofTask::AxiomConsistency, "Satisfiable") => Ok(SzsOutcome::Consistent),
        (FofTask::AxiomConsistency, "Unsatisfiable" | "ContradictoryAxioms") => {
            Ok(SzsOutcome::Inconsistent)
        }
        (FofTask::AxiomConsistency, "Unknown" | "GaveUp" | "Timeout" | "ResourceOut") => {
            Ok(SzsOutcome::Undecided)
        }
        (FofTask::AxiomConsistency, "Theorem" | "CounterSatisfiable") => Err(protocol_error(
            "SZS_TASK_SHAPE_MISMATCH",
            format!(
                "status {status:?} is conjecture-shaped but the submitted problem contains axioms only"
            ),
        )),
        (_, other) => Err(protocol_error(
            "UNKNOWN_SZS_STATUS",
            format!("status token {other:?} has no admitted meaning for {task:?}"),
        )),
    }
}

fn normalize_status(status: &str) -> String {
    status
        .trim_matches(|character: char| matches!(character, '\'' | '"' | ',' | ';' | '.'))
        .to_owned()
}

fn normalize_problem_name(problem: &str) -> String {
    problem
        .trim_matches(|character: char| {
            matches!(character, '\'' | '"' | ',' | ';' | '.' | '(' | ')')
        })
        .to_owned()
}

/// Return whether an SZS problem token names one of the exact submitted-problem aliases.
///
/// Provers may quote the token or repeat the submitted path rather than only its basename.
/// Status and artifact framing use this same normalization so the two protocol layers cannot
/// disagree about which problem a child named.
#[must_use]
pub fn problem_name_matches(problem: &str, expected: &BTreeSet<String>) -> bool {
    let normalized = normalize_problem_name(problem);
    if expected.contains(&normalized) {
        return true;
    }
    let path = std::path::Path::new(&normalized);
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| expected.contains(name))
        || path
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| expected.contains(name))
}

fn protocol_error(code: &str, detail: impl Into<String>) -> SzsProtocolError {
    SzsProtocolError {
        code: code.to_owned(),
        detail: detail.into(),
    }
}

#[cfg(test)]
#[path = "protocol.tests.rs"]
mod tests;
