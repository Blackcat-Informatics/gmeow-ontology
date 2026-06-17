# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Type stub for the gmeow_logic PyO3 extension. The signatures are transcribed
# verbatim from the `#[pyo3(signature = ...)]` annotations in crates/logic/src/py.rs —
# keep them in lockstep with that file (it is the ABI source of truth).
#
# Each function returns a freshly-built Python dict. The shapes are:
#   materialize -> {"facts": [...], "derivations": [...], "budget_status": str,
#                   "incomplete": bool, ...}
#   certify     -> the CertificationVerdict.to_json() dict
#   query       -> {"bindings": [{var: str, ...}, ...], "status": str}
# They are typed as ``dict[str, Any]`` here (mypy then checks the *call sites* —
# arity and argument types — which is where FFI mistakes hide).

from typing import Any

def materialize(
    rules: str,
    input: str,
    max_rule_firings: int | None = ...,
    max_answers: int | None = ...,
    time_ms: int | None = ...,
) -> dict[str, Any]: ...
def certify(rules: str, profile: str) -> dict[str, Any]: ...
def query(
    world_nquads: str,
    query_program: str,
    profile: str,
    world_iri: str | None = ...,
    max_answers: int | None = ...,
    max_steps: int | None = ...,
) -> dict[str, Any]: ...
