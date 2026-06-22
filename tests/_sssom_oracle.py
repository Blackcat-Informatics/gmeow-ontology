# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Oracle for the *current* ``sssom`` Python validation behaviour (#848, Task 1).

This module runs the EXACT validation path that production uses today in
:func:`gmeow_tools.mapping_compile._validate_sssom`:

    safe  = _sssom_for_validation(text)              # GMEOW→sssom-py YAML shim
    msdf  = sssom.parse_tsv(io.StringIO(safe))       # sssom-py TSV parser
    reps  = validate(msdf, validation_types=None,     # default check set
                     fail_on_error=False)

and normalises the resulting ``ERROR``/``FATAL`` diagnostics into a stable,
JSON-serialisable shape so the future native Rust validator can be proven to
match it byte-for-byte.

.. warning::
    This module **imports the ``sssom`` package**, which #848 is in the process
    of removing. The oracle is therefore *transient* — it exists only to mint
    the durable golden artifact (``tests/fixtures/lint-golden/sssom_validation.json``).
    Once the native Rust validator is wired and the parity test passes against
    that frozen golden, this module (and the ``sssom`` dependency) can be
    deleted. The golden is the durable artifact; this oracle is the scaffolding
    that produced it.

Captured behavioural facts (sssom-py 0.4.17), for the native re-implementation:

* The default check set (``validation_types=None``) resolves to
  ``sssom.validators.DEFAULT_VALIDATION_TYPES`` =
  ``[JsonSchema, PrefixMapCompleteness, StrictCurieFormat]``.
  ``Shacl`` and ``Sparql`` exist in the enum but are **not** run by default.
* ``parse_tsv`` is a *pre-filter*: any data row that is not well-formed (blank
  required field, non-CURIE entity reference, a ``|`` pipe inside an entity
  slot, non-numeric confidence, …) is **silently dropped** from the
  ``MappingSetDataFrame`` before ``validate`` ever sees it. Such rows therefore
  produce **no** validation result. Only defects that survive parsing reach the
  validators — in practice that is ``JsonSchema`` (numeric range / enum) and
  ``PrefixMapCompleteness`` (CURIE prefix not in the curie_map).
* ``StrictCurieFormat`` only flags a ``|`` in a single-valued entity-reference
  slot, but ``parse_tsv`` drops any pipe-bearing row first, so this check is
  effectively unreachable through the parse→validate path GMEOW uses.
"""

from __future__ import annotations

import io
import logging
from collections.abc import Iterator
from contextlib import contextmanager
from typing import Any

import sssom
import sssom.validators as sssom_validators
from sssom.validators import validate

from gmeow_tools.mapping_compile import _sssom_for_validation

#: The default check set ``validate`` runs when ``validation_types=None``.
#: ``DEFAULT_VALIDATION_TYPES`` is a real module-level constant in sssom-py but
#: is not in its ``__all__``, so it is fetched via ``getattr`` (yielding ``Any``)
#: to satisfy strict mypy without depending on the untyped package's exports.
DEFAULT_VALIDATION_TYPES = getattr(sssom_validators, "DEFAULT_VALIDATION_TYPES")  # noqa: B009

#: Normalised result record.
OracleResult = dict[str, Any]

#: The severities that production collects (``_validate_sssom``).
_ERROR_SEVERITIES = ("ERROR", "FATAL")


def default_validation_types() -> list[str]:
    """Return the ordered ``.value`` names of the checks ``validate`` runs by default.

    This is the set the native validator must replicate exactly when invoked
    with ``validation_types=None``. Captured from
    ``sssom.validators.DEFAULT_VALIDATION_TYPES`` so it tracks the installed
    sssom-py rather than a hand-copied list.
    """
    return [vt.value for vt in DEFAULT_VALIDATION_TYPES]


@contextmanager
def _quiet() -> Iterator[None]:
    """Silence sssom-py / linkml chatter.

    Both ``parse_tsv`` (dropped-row warnings) and the validators
    (``print_linkml_report``) write to the root logger / stdout. The diagnostics
    we care about come back through the returned :class:`ValidationReport`
    objects, not the log stream, so we suppress the noise to keep the oracle
    output deterministic and quiet. This changes *no* validation behaviour.
    """
    previous = logging.root.manager.disable
    logging.disable(logging.CRITICAL)
    try:
        yield
    finally:
        logging.disable(previous)


def validate_sssom_text(text: str) -> list[OracleResult]:
    """Run the production validation path over one SSSOM TSV ``text``.

    Mirrors :func:`gmeow_tools.mapping_compile._validate_sssom` for a single
    file, but returns *structured* records instead of pre-formatted strings:

    .. code-block:: python

        [{"severity": "ERROR", "type": "jsonschema validation",
          "message": "...", "instance": None, "check": "JsonSchema"}, ...]

    Records are filtered to ``severity in {"ERROR", "FATAL"}`` (exactly what
    production collects) and returned in deterministic order: first by the
    order checks run (``DEFAULT_VALIDATION_TYPES``), then by the order results
    appear within each report.

    If ``sssom.parse_tsv`` itself raises (input too malformed to parse at all),
    a single synthetic ``parse-error``-class record is returned instead of
    propagating the exception — matching the way production records parse
    failures as a problem string rather than crashing the compile.
    """
    safe = _sssom_for_validation(text)
    with _quiet():
        try:
            msdf = sssom.parse_tsv(io.StringIO(safe))
        except Exception as exc:  # faithfully record any parse failure
            return [
                {
                    "severity": "FATAL",
                    "type": "parse error",
                    "message": f"{type(exc).__name__}: {exc}",
                    "instance": None,
                    "check": "parse_tsv",
                }
            ]
        reports = validate(msdf, validation_types=None, fail_on_error=False)

    results: list[OracleResult] = []
    for validation_type, report in reports.items():
        for result in report.results:
            severity = result.severity.value
            if severity not in _ERROR_SEVERITIES:
                continue
            results.append(
                {
                    "severity": severity,
                    "type": result.type,
                    "message": result.message,
                    "instance": result.instance
                    if result.instance is not None
                    else None,
                    "check": validation_type.value,
                }
            )
    return results
