"""Expose the GMEOW developer CLI as a separate workspace package."""

from __future__ import annotations

from gmeow_tools.cli_dev import app

__all__ = ["app"]
