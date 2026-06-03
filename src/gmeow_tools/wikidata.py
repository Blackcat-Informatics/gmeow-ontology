"""Wikidata QID/PID validation.

Two tiers, matching the plan:

* **Syntax** — an always-on, offline regex gate. Automated mapping tools readily
  emit syntactically-plausible-but-wrong identifiers; this rejects malformed
  ones (``Q0``, ``Q12abc``, ``P0`` …) with no network access.
* **Existence** — a network-gated check that each identifier resolves on
  Wikidata and is not a redirect or tombstone. Skipped cleanly when offline.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from enum import StrEnum

import httpx

from gmeow_tools.config import PREFIXES

#: A valid Wikidata item id: ``Q`` followed by a non-zero-leading integer.
QID_RE = re.compile(r"^Q[1-9]\d*$")
#: A valid Wikidata property id.
PID_RE = re.compile(r"^P[1-9]\d*$")

_WD_NS = PREFIXES["wd"]
_API = "https://www.wikidata.org/w/api.php"


def is_valid_qid(identifier: str) -> bool:
    """Return whether a string is a syntactically valid Wikidata item id."""
    return QID_RE.fullmatch(identifier) is not None


def is_valid_pid(identifier: str) -> bool:
    """Return whether a string is a syntactically valid Wikidata property id."""
    return PID_RE.fullmatch(identifier) is not None


def is_valid_id(identifier: str) -> bool:
    """Return whether a string is a valid Wikidata item *or* property id."""
    return is_valid_qid(identifier) or is_valid_pid(identifier)


def local_name(iri: str) -> str | None:
    """Return the Wikidata local id for an IRI in the ``wd:`` namespace.

    Args:
        iri: A full IRI.

    Returns:
        The local id (e.g. ``"Q42"``) if ``iri`` is in the Wikidata entity
        namespace, otherwise ``None``.
    """
    if iri.startswith(_WD_NS):
        return iri[len(_WD_NS) :]
    return None


class ExistenceStatus(StrEnum):
    """Result of a Wikidata existence check for one identifier."""

    OK = "ok"
    MISSING = "missing"
    REDIRECT = "redirect"
    BAD_SYNTAX = "bad-syntax"


@dataclass(slots=True)
class SyntaxReport:
    """Outcome of the offline syntax gate over a set of identifiers."""

    valid: list[str]
    invalid: list[str]

    @property
    def ok(self) -> bool:
        """Return whether every identifier passed the syntax gate."""
        return not self.invalid


def check_syntax(identifiers: list[str]) -> SyntaxReport:
    """Partition identifiers into syntactically valid and invalid sets."""
    valid: list[str] = []
    invalid: list[str] = []
    for identifier in identifiers:
        (valid if is_valid_id(identifier) else invalid).append(identifier)
    return SyntaxReport(valid=valid, invalid=invalid)


def check_existence(
    identifiers: list[str], *, timeout: float = 30.0
) -> dict[str, ExistenceStatus]:
    """Check that each identifier exists on Wikidata and is not a redirect.

    Performs a single batched ``wbgetentities`` request (Wikidata allows up to
    50 ids per call; callers should chunk larger sets). Network access is
    required; callers gate this behind a connectivity/opt-in check.

    Args:
        identifiers: Wikidata ids (``Q…``/``P…``). Malformed ids are reported
            as ``BAD_SYNTAX`` without being sent to the API.
        timeout: HTTP timeout in seconds.

    Returns:
        Mapping of identifier to :class:`ExistenceStatus`.
    """
    statuses: dict[str, ExistenceStatus] = {}
    queryable = []
    for identifier in identifiers:
        if is_valid_id(identifier):
            queryable.append(identifier)
        else:
            statuses[identifier] = ExistenceStatus.BAD_SYNTAX
    if not queryable:
        return statuses

    response = httpx.get(
        _API,
        params={
            "action": "wbgetentities",
            "ids": "|".join(queryable),
            "props": "info",
            "format": "json",
        },
        timeout=timeout,
        headers={"User-Agent": "gmeow-tools/0.1 (ontology mapping validator)"},
    )
    response.raise_for_status()
    entities = response.json().get("entities", {})
    for identifier in queryable:
        entity = entities.get(identifier)
        if entity is None or "missing" in entity:
            statuses[identifier] = ExistenceStatus.MISSING
        elif "redirects" in entity:
            statuses[identifier] = ExistenceStatus.REDIRECT
        else:
            statuses[identifier] = ExistenceStatus.OK
    return statuses
