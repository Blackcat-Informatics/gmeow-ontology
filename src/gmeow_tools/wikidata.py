"""Wikidata QID/PID validation.

Two tiers, matching the plan:

* **Syntax** — an always-on, offline regex gate. Automated mapping tools readily
  emit syntactically-plausible-but-wrong identifiers; this rejects malformed
  ones (``Q0``, ``Q12abc``, ``P0`` …) with no network access.
* **Existence** — a network-gated check that each identifier resolves on
  Wikidata and is not a redirect or tombstone. Skipped cleanly when offline.

API responses are cached on disk (see :func:`_cache_dir`) with a TTL so
repeated runs are fast and respectful of Wikidata's infrastructure.
"""

from __future__ import annotations

import hashlib
import json
import logging
import re
import time
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path

import httpx

from gmeow_tools.config import PREFIXES, PROJECT_ROOT

_log = logging.getLogger(__name__)

#: A valid Wikidata item id: ``Q`` followed by a non-zero-leading integer.
QID_RE = re.compile(r"^Q[1-9]\d*$")
#: A valid Wikidata property id.
PID_RE = re.compile(r"^P[1-9]\d*$")

_WD_NS = PREFIXES["wd"]
_WDT_NS = PREFIXES["wdt"]
_API = "https://www.wikidata.org/w/api.php"

#: Default cache TTL in seconds (7 days).
DEFAULT_CACHE_TTL = 7 * 24 * 60 * 60


def _cache_dir() -> Path:
    """Return the directory used for cached Wikidata API responses."""
    cache = PROJECT_ROOT / ".cache" / "wikidata"
    cache.mkdir(parents=True, exist_ok=True)
    return cache


def _cache_key(identifiers: list[str]) -> str:
    """Return a stable cache key for a sorted list of identifiers."""
    payload = "|".join(sorted(identifiers))
    return hashlib.sha256(payload.encode()).hexdigest()


def _cache_path(key: str) -> Path:
    """Return the filesystem path for a given cache key."""
    return _cache_dir() / f"{key}.json"


def _load_cached(key: str, ttl: float = DEFAULT_CACHE_TTL) -> dict[str, object] | None:
    """Load a cached API response if it exists and is fresh."""
    path = _cache_path(key)
    if not path.exists():
        return None
    if time.time() - path.stat().st_mtime > ttl:
        return None
    try:
        with path.open("r", encoding="utf-8") as fh:
            return json.load(fh)  # type: ignore[no-any-return]
    except (json.JSONDecodeError, OSError) as exc:
        _log.debug("wikidata cache read failed for %s: %s", key, exc)
        return None


def _save_cached(key: str, data: dict[str, object]) -> None:
    """Save an API response to the cache."""
    path = _cache_path(key)
    try:
        with path.open("w", encoding="utf-8") as fh:
            json.dump(data, fh)
    except OSError as exc:
        _log.debug("wikidata cache write failed for %s: %s", key, exc)


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


def local_name_wdt(iri: str) -> str | None:
    """Return the Wikidata local id for an IRI in the ``wdt:`` namespace.

    Args:
        iri: A full IRI.

    Returns:
        The local id (e.g. ``"P31"``) if ``iri`` is in the Wikidata direct
        property namespace, otherwise ``None``.
    """
    if iri.startswith(_WDT_NS):
        return iri[len(_WDT_NS) :]
    return None


class NamespaceMisuse(StrEnum):
    """Category of namespace misuse detected in offline validation."""

    WD_PROP_SHOULD_BE_WDT = "wd-prop-should-be-wdt"
    WDT_ITEM_SHOULD_BE_WD = "wdt-item-should-be-wd"
    HTTPS_URL_SHOULD_BE_CURIE = "https-url-should-be-curie"
    BAD_SYNTAX = "bad-syntax"


@dataclass(slots=True)
class SyntaxReport:
    """Outcome of the offline syntax gate over a set of identifiers."""

    valid: list[str]
    invalid: list[str]
    misuses: list[tuple[str, NamespaceMisuse, str]]

    @property
    def ok(self) -> bool:
        """Return whether every identifier passed the syntax gate."""
        return not self.invalid and not self.misuses


def check_syntax(identifiers: list[str]) -> SyntaxReport:
    """Partition identifiers into syntactically valid and invalid sets."""
    valid: list[str] = []
    invalid: list[str] = []
    misuses: list[tuple[str, NamespaceMisuse, str]] = []

    for identifier in identifiers:
        if not is_valid_id(identifier):
            invalid.append(identifier)
            continue

        valid.append(identifier)

    return SyntaxReport(valid=valid, invalid=invalid, misuses=misuses)


def check_syntax_iri(
    iri: str, *, in_object_position: bool = False
) -> list[tuple[str, NamespaceMisuse, str]]:
    """Detect namespace misuse and syntax errors in a Wikidata IRI.

    When *in_object_position* is ``False`` (the default), ``wd:P…`` is flagged
    because the typical intent in predicate position is a direct-claim property.
    When ``True`` (e.g. the IRI appears as the object of a mapping triple),
    ``wd:P…`` is accepted because it may be a legitimate property-concept
    reference.

    Returns a list of (local_id, misuse_type, message) tuples.
    """
    misuses: list[tuple[str, NamespaceMisuse, str]] = []

    # HTTPS URL that should be a CURIE
    if iri.startswith("https://www.wikidata.org/entity/"):
        local = iri[len("https://www.wikidata.org/entity/") :]
        if is_valid_id(local):
            misuses.append(
                (
                    local,
                    NamespaceMisuse.HTTPS_URL_SHOULD_BE_CURIE,
                    (
                        f"{iri} should be written as "
                        f"{'wd:' if local.startswith('Q') else 'wdt:'}{local}"
                    ),
                )
            )
        else:
            misuses.append(
                (
                    local,
                    NamespaceMisuse.BAD_SYNTAX,
                    f"malformed identifier in HTTPS URL: {iri}",
                )
            )
        return misuses

    # wd: namespace checks
    if iri.startswith(_WD_NS):
        local = iri[len(_WD_NS) :]
        if not is_valid_id(local):
            misuses.append(
                (
                    local,
                    NamespaceMisuse.BAD_SYNTAX,
                    f"malformed wd: identifier: {local}",
                )
            )
        # wd:P… is technically valid (properties are entities), but we flag it
        # as a potential misuse when the intent is a direct property.
        elif local.startswith("P") and not in_object_position:
            misuses.append(
                (
                    local,
                    NamespaceMisuse.WD_PROP_SHOULD_BE_WDT,
                    (
                        f"wd:{local} is a property entity; "
                        f"use wdt:{local} for direct-claim property mappings"
                    ),
                )
            )
        return misuses

    # wdt: namespace checks
    if iri.startswith(_WDT_NS):
        local = iri[len(_WDT_NS) :]
        if not is_valid_pid(local):
            if is_valid_qid(local):
                misuses.append(
                    (
                        local,
                        NamespaceMisuse.WDT_ITEM_SHOULD_BE_WD,
                        f"wdt:{local} is an item ID; use wd:{local} for item mappings",
                    )
                )
            else:
                misuses.append(
                    (
                        local,
                        NamespaceMisuse.BAD_SYNTAX,
                        f"malformed wdt: identifier: {local}",
                    )
                )
        return misuses

    return misuses


class ExistenceStatus(StrEnum):
    """Result of a Wikidata existence check for one identifier."""

    OK = "ok"
    MISSING = "missing"
    REDIRECT = "redirect"
    BAD_SYNTAX = "bad-syntax"


def _fetch_entities(
    queryable: list[str], timeout: float
) -> dict[str, dict[str, object]]:
    """Fetch entity data from Wikidata, using the on-disk cache when fresh."""
    key = _cache_key(queryable)
    cached = _load_cached(key)
    if cached is not None:
        return cached  # type: ignore[return-value]

    response = httpx.get(
        _API,
        params={
            "action": "wbgetentities",
            "ids": "|".join(queryable),
            "props": "info|labels",
            "format": "json",
            "languages": "en",
            "languagefallback": "1",
        },
        timeout=timeout,
        headers={"User-Agent": "gmeow-tools/0.1 (ontology mapping validator)"},
    )
    response.raise_for_status()
    payload = response.json()
    _save_cached(key, payload)
    entities = payload.get("entities", {})
    if isinstance(entities, dict):
        return entities
    return {}


def check_existence(
    identifiers: list[str],
    *,
    timeout: float = 30.0,
    chunk_size: int = 50,
    delay: float = 0.1,
) -> dict[str, ExistenceStatus]:
    """Check that each identifier exists on Wikidata and is not a redirect.

    Performs batched ``wbgetentities`` requests (chunked to ``chunk_size``).
    Responses are cached on disk with a TTL for fast repeated runs. Network
    access is required; callers gate this behind a connectivity/opt-in check.

    Args:
        identifiers: Wikidata ids (``Q…``/``P…``). Malformed ids are reported
            as ``BAD_SYNTAX`` without being sent to the API.
        timeout: HTTP timeout in seconds per chunk.
        chunk_size: Maximum ids per API call (Wikidata limit is 50).
        delay: Seconds to sleep between chunks to respect rate limits.

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

    for i in range(0, len(queryable), chunk_size):
        chunk = queryable[i : i + chunk_size]
        entities = _fetch_entities(chunk, timeout)
        for identifier in chunk:
            entity = entities.get(identifier)
            if entity is None or "missing" in entity:
                statuses[identifier] = ExistenceStatus.MISSING
            elif "redirects" in entity:
                statuses[identifier] = ExistenceStatus.REDIRECT
            else:
                statuses[identifier] = ExistenceStatus.OK
        if i + chunk_size < len(queryable):
            time.sleep(delay)
    return statuses


def check_labels(
    identifiers: list[str],
    expected: dict[str, str],
    *,
    timeout: float = 30.0,
    chunk_size: int = 50,
    delay: float = 0.1,
) -> dict[str, tuple[str, str | None]]:
    """Compare authored objectLabels with live Wikidata English labels.

    Returns a mapping of identifier → (expected_label, live_label_or_none).
    A mismatch (or missing live label) can be flagged by the caller.

    Args:
        identifiers: Wikidata ids to check.
        expected: Mapping of identifier → authored label.
        timeout: HTTP timeout in seconds per chunk.
        chunk_size: Maximum ids per API call.
        delay: Seconds to sleep between chunks.

    Returns:
        Mapping of identifier to (expected_label, live_label).
    """
    result: dict[str, tuple[str, str | None]] = {}
    queryable = [id_ for id_ in identifiers if is_valid_id(id_)]
    if not queryable:
        return result

    for i in range(0, len(queryable), chunk_size):
        chunk = queryable[i : i + chunk_size]
        entities = _fetch_entities(chunk, timeout)
        for identifier in chunk:
            entity = entities.get(identifier)
            live_label: str | None = None
            if entity is not None and "labels" in entity:
                labels = entity["labels"]
                if isinstance(labels, dict):
                    en_label = labels.get("en", {})
                    if isinstance(en_label, dict):
                        live_label = en_label.get("value")
            result[identifier] = (expected.get(identifier, ""), live_label)
        if i + chunk_size < len(queryable):
            time.sleep(delay)
    return result
