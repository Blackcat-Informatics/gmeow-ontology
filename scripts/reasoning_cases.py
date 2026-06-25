#!/usr/bin/env python3
"""Repo-local Docker-backed reasoning case runner."""

from __future__ import annotations

import sys

from gmeow_tools.oracles.reasoning_cases import run_all
from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError


def main() -> int:
    """Run the repo-local reasoning cases and return a process exit code."""
    try:
        completed = run_all()
    except ToolUnavailableError as exc:
        print(f"tool unavailable: {exc}", file=sys.stderr)
        return 2
    except ToolExecutionError as exc:
        print(f"reasoning case failed:\n{exc.output}", file=sys.stderr)
        return 2
    except AssertionError as exc:
        print(f"reasoning case failed: {exc}", file=sys.stderr)
        return 2
    for name in completed:
        print(f"ok: {name}")
    print("ok: reasoning cases passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
