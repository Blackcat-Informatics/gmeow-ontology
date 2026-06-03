"""Generate a JSON-LD context for the GMEOW vocabulary.

The context publishes the canonical prefix registry plus ``@vocab`` pointing at
the GMEOW namespace, so JSON-LD consumers can use compact GMEOW terms directly.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from gmeow_tools.config import DIST_DIR, NAMESPACE, PREFIXES


def build_context() -> dict[str, Any]:
    """Build the JSON-LD ``@context`` object from the prefix registry.

    Returns:
        A mapping suitable for JSON serialization, with ``@vocab`` set to the
        GMEOW namespace and one entry per registered prefix.
    """
    context: dict[str, Any] = {"@vocab": NAMESPACE}
    for prefix, namespace in PREFIXES.items():
        context[prefix] = namespace
    return {"@context": context}


def write_context(
    dist_dir: Path = DIST_DIR, *, filename: str = "context.jsonld"
) -> Path:
    """Write the JSON-LD context document to ``dist/``.

    Args:
        dist_dir: Target directory (created if absent).
        filename: Output filename.

    Returns:
        The path to the written context file.
    """
    dist_dir.mkdir(parents=True, exist_ok=True)
    out = dist_dir / filename
    out.write_text(json.dumps(build_context(), indent=2, sort_keys=False) + "\n")
    return out
