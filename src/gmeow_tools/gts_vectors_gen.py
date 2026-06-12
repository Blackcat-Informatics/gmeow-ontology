# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""The ``vectors`` generator: the frozen GTS conformance corpus (#327, §18)."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING

from gmeow_tools.config import GENERATED_DIR, PROJECT_ROOT
from gmeow_tools.generator import Generator, register
from gts.vectors import corpus, expected_for

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

VECTORS_DIR = GENERATED_DIR / "gts-vectors"


@register
class GtsVectorsGenerator(Generator):
    """Emit the language-neutral GTS conformance corpus (bytes + expected)."""

    name: str = "gts-vectors"
    # The corpus includes canonical payloads (dist/ai-package segments) that
    # legitimately carry internal private-use language tags per §13.1; the
    # expected.json folds those payloads and therefore contains the same tags.
    # This is the canonical internal form, not a projection, so it opts out of
    # the internal-tag leak gate just as the statements generator does.
    allows_internal_tags: bool = True

    @property
    def inputs(self) -> Sequence[Path]:
        """The GTS reference implementation defines the corpus."""
        gts_dir = PROJECT_ROOT / "packages" / "gts" / "src" / "gts"
        return sorted(gts_dir.glob("*.py"))

    @property
    def outputs(self) -> Sequence[Path]:
        """One ``.gts`` + one ``.expected.json`` per corpus case."""
        out: list[Path] = []
        for case in corpus():
            out.append(VECTORS_DIR / f"{case.name}.gts")
            out.append(VECTORS_DIR / f"{case.name}.expected.json")
        return out

    def render(self, staging: Path) -> None:
        """Write corpus bytes and oracle-computed expectations."""
        target = staging / VECTORS_DIR.relative_to(PROJECT_ROOT)
        target.mkdir(parents=True, exist_ok=True)
        for case in corpus():
            (target / f"{case.name}.gts").write_bytes(case.data)
            (target / f"{case.name}.expected.json").write_text(
                json.dumps(expected_for(case), indent=1, sort_keys=True) + "\n",
                encoding="utf-8",
            )
