# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Package wrapper for the gmeow_logic PyO3 extension.

Mirrors the ``__init__.py`` maturin auto-generates for a bare cdylib: it
re-exports the compiled submodule's public surface so
``import gmeow_logic; gmeow_logic.query(...)`` works. We check it in (mixed
Rust/Python layout) only so the type stub ``__init__.pyi`` and the PEP 561
``py.typed`` marker can ship alongside the extension — see
``crates/logic/pyproject.toml``. The runtime ``__doc__`` below is overridden
with the extension's own docstring, matching maturin's generated wrapper.
"""

from .gmeow_logic import *  # noqa: F403

__doc__ = gmeow_logic.__doc__  # noqa: F405
if hasattr(gmeow_logic, "__all__"):  # noqa: F405
    __all__ = gmeow_logic.__all__  # noqa: F405
