# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Required native-extension imports for tests."""

from __future__ import annotations

from importlib import import_module
from typing import Any


def require_gmeow_logic() -> Any:
    """Import the native logic engine or fail the test environment."""
    try:
        return import_module("gmeow_logic")
    except ImportError as exc:
        raise AssertionError(
            "gmeow_logic native extension is required for logic tests; "
            "run `make native-py`, `make test-fast`, or `make test`."
        ) from exc
