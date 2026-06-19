"""Native SLME module extraction with license-policy enforcement.

Module extraction is native (Java/Docker-free): it runs the Rust ``gmeow_logic``
syntactic-locality extractor in-process, replacing the retired ROBOT shell-out.

Extraction *copies* axioms/labels from a source ontology into GMEOW (a CC BY 4.0
work). That is only permissible for compatibly-licensed sources, so every
extraction is guarded by the link policy from ``config``: a ``REFERENCE_ONLY``
target (NC/ND/share-alike/copyleft/proprietary) is refused, loudly. Such targets
may still be *linked* by IRI via the mappings layer.

This is the concrete teeth behind the plan's "refuses reference-only imports".

Maintainer-only (#666 / Principle 18)
-------------------------------------
ROBOT module-extraction requires Java/Docker, so this is a **maintainer-only**
tool (``make extract`` / ``make refresh-target-axioms``): it is NOT on the
normal-use primary path, never part of ``make check`` or the required CI
``quality`` gate. Replicating ROBOT's SLME module extraction natively in the Rust
core (to drop the Java/Docker dependency entirely) is tracked in **#695**; until
then extraction stays a maintainer-run, Docker-gated step alongside the
``classic-cross-check`` lane.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    DIST_DIR,
    LinkPolicy,
)


class LicensePolicyError(RuntimeError):
    """Raised when extraction is attempted from a reference-only source."""


def guard_importable(target_name: str) -> None:
    """Raise if a target may not have its axioms copied into GMEOW.

    Args:
        target_name: Key into :data:`config.ALIGNMENT_TARGETS`.

    Raises:
        LicensePolicyError: If the target is unknown or ``REFERENCE_ONLY``.
    """
    target = ALIGNMENT_TARGETS.get(target_name)
    if target is None:
        raise LicensePolicyError(
            f"unknown alignment target {target_name!r}; refusing to extract"
        )
    if target.policy is not LinkPolicy.IMPORT_OK:
        raise LicensePolicyError(
            f"refusing to extract {target.name} ({target.license}): "
            f"{target.policy.value}. Link it by IRI instead — do not copy its "
            f"axioms into CC BY 4.0 GMEOW."
        )


def extract_terms(
    target_name: str,
    *,
    source: Path,
    terms: list[str],
    output: Path,
    method: str = "STAR",
) -> Path:
    """Extract a term subset (SLME) from a source ontology, if license permits.

    Args:
        target_name: Key into :data:`config.ALIGNMENT_TARGETS` (license-checked).
        source: Source ontology file under the repo (e.g. a vendored import).
        terms: Seed term IRIs to extract the module around.
        output: Destination Turtle file.
        method: SLME extraction notion (``STAR`` nested bot/top by default;
            also ``BOT``/``TOP``, case-insensitive).

    Returns:
        The path to the extracted module.

    Raises:
        LicensePolicyError: If the target is reference-only/unknown.
        FileNotFoundError: If the source ontology is missing.
    """
    guard_importable(target_name)
    if not source.exists():
        raise FileNotFoundError(f"extract source not found: {source}")
    output.parent.mkdir(parents=True, exist_ok=True)

    import gmeow_logic

    result = gmeow_logic.extract_module(
        source.read_text(encoding="utf-8"), list(terms), method
    )
    output.write_text(result["module_ttl"], encoding="utf-8")
    return output


def umbel_extract_path() -> Path:
    """Return the conventional path for the extracted UMBEL connectivity layer."""
    return DIST_DIR / "umbel-extract.ttl"
