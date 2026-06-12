# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""gmeow — grounded agent memory in five minutes.

Store, recall, and revise claims with confidence, standpoint, and
provenance. Every claim is a reified RDF 1.2 statement under the hood;
the package on disk is a verifiable GTS file; revision is supersession,
never deletion.

>>> from gmeow import Memory
>>> mem = Memory("assistant.gts")
"""

from __future__ import annotations

from gmeow.memory import Claim, Memory, ToolCallRecord

__all__ = ["Claim", "Memory", "ToolCallRecord"]
