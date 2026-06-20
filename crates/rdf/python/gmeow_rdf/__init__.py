# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Package wrapper for the gmeow_rdf PyO3 extension.

Mirrors the ``__init__.py`` maturin auto-generates for a bare cdylib: it
re-exports the compiled submodule's public surface so ``import gmeow_rdf`` works.
We check it in (mixed Rust/Python layout) only so the type stub ``__init__.pyi``
and the PEP 561 ``py.typed`` marker can ship alongside the extension — see
``crates/rdf/pyproject.toml``. The runtime ``__doc__`` below is overridden with
the extension's own docstring, matching maturin's generated wrapper.
"""

from . import gmeow_rdf as _ext
from .gmeow_rdf import *  # noqa: F403

__doc__ = _ext.__doc__
if hasattr(_ext, "__all__"):
    __all__ = _ext.__all__
