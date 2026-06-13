# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Human-friendly visual hashes for keys and file checksums.

Provides:

* ``emojihash`` — map a digest to a short sequence of emojis.
* ``randomart`` — OpenSSH-style "Drunken Bishop" ASCII art fingerprint.
"""

from __future__ import annotations

from typing import Final

from blake3 import blake3

# A fixed table of 256 single-codepoint emojis.  No zero-width joiners, no
# variation selectors, no skin-tone modifiers.  The order is part of the spec:
# changing it changes the visual hash for existing releases.
_EMOJIS: Final[tuple[str, ...]] = (
    "😀",
    "😁",
    "😂",
    "😃",
    "😄",
    "😅",
    "😆",
    "😇",
    "😈",
    "😉",
    "😊",
    "😋",
    "😌",
    "😍",
    "😎",
    "😏",
    "😐",
    "😑",
    "😒",
    "😓",
    "😔",
    "😕",
    "😖",
    "😗",
    "😘",
    "😙",
    "😚",
    "😛",
    "😜",
    "😝",
    "😞",
    "😟",
    "😠",
    "😡",
    "😢",
    "😣",
    "😤",
    "😥",
    "😦",
    "😧",
    "😨",
    "😩",
    "😪",
    "😫",
    "😬",
    "😭",
    "😮",
    "😯",
    "😰",
    "😱",
    "😲",
    "😳",
    "😴",
    "😵",
    "😶",
    "😷",
    "😸",
    "😹",
    "😺",
    "😻",
    "😼",
    "😽",
    "😾",
    "😿",
    "🙀",
    "🙁",
    "🙂",
    "🙃",
    "🙄",
    "🙅",
    "🙆",
    "🙇",
    "🙈",
    "🙉",
    "🙊",
    "🙋",
    "🙌",
    "🙍",
    "🙎",
    "🙏",
    "🚀",
    "🚁",
    "🚂",
    "🚃",
    "🚄",
    "🚅",
    "🚆",
    "🚇",
    "🚈",
    "🚉",
    "🚊",
    "🚋",
    "🚌",
    "🚍",
    "🚎",
    "🚏",
    "🚐",
    "🚑",
    "🚒",
    "🚓",
    "🚔",
    "🚕",
    "🚖",
    "🚗",
    "🚘",
    "🚙",
    "🚚",
    "🚛",
    "🚜",
    "🚝",
    "🚞",
    "🚟",
    "🚠",
    "🚡",
    "🚢",
    "🚣",
    "🚤",
    "🚥",
    "🚦",
    "🚧",
    "🚨",
    "🚩",
    "🚪",
    "🚫",
    "🚬",
    "🚭",
    "🚮",
    "🚯",
    "🚰",
    "🚱",
    "🚲",
    "🚳",
    "🚴",
    "🚵",
    "🚶",
    "🚷",
    "🚸",
    "🚹",
    "🚺",
    "🚻",
    "🚼",
    "🚽",
    "🚾",
    "🚿",
    "🛀",
    "🛁",
    "🛂",
    "🛃",
    "🛄",
    "🛅",
    "🐀",
    "🐁",
    "🐂",
    "🐃",
    "🐄",
    "🐅",
    "🐆",
    "🐇",
    "🐈",
    "🐉",
    "🐊",
    "🐋",
    "🐌",
    "🐍",
    "🐎",
    "🐏",
    "🐐",
    "🐑",
    "🐒",
    "🐓",
    "🐔",
    "🐕",
    "🐖",
    "🐗",
    "🐘",
    "🐙",
    "🐚",
    "🐛",
    "🐜",
    "🐝",
    "🐞",
    "🐟",
    "🐠",
    "🐡",
    "🐢",
    "🐣",
    "🐤",
    "🐥",
    "🐦",
    "🐧",
    "🐨",
    "🐩",
    "🐪",
    "🐫",
    "🐬",
    "🐭",
    "🐮",
    "🐯",
    "🐰",
    "🐱",
    "🐲",
    "🐳",
    "🐴",
    "🐵",
    "🐶",
    "🐷",
    "🐸",
    "🐹",
    "🐺",
    "🐻",
    "🐼",
    "🐽",
    "🐾",
    "🐿",
    "🍀",
    "🍁",
    "🍂",
    "🍃",
    "🍄",
    "🍅",
    "🍆",
    "🍇",
    "🍈",
    "🍉",
    "🍊",
    "🍋",
    "🍌",
    "🍍",
    "🍎",
    "🍏",
    "🍐",
    "🍑",
    "🍒",
    "🍓",
    "🍔",
    "🍕",
    "🍖",
    "🍗",
    "🍘",
    "🍙",
    "🍚",
    "🍛",
    "🍜",
    "🍝",
    "🍞",
    "🍟",
    "🍠",
    "🍡",
    "🍢",
    "🍣",
    "🍤",
    "🍥",
    "🍦",
    "🍧",
    "🍨",
    "🍩",
)

assert len(_EMOJIS) == 256

# Character ramp used by OpenSSH's randomart.  The first slot is the start
# position (overwritten with 'S' below), the last is the end position.
_RANDOMART_VALUES: Final[str] = " .o+=*BOX@%&#/^"


def emojihash(data: bytes, length: int = 8) -> str:
    """Map ``data`` to ``length`` emoji using BLAKE3 and the fixed 256-table."""
    digest = blake3(data).digest(length=max(1, length))
    return " ".join(_EMOJIS[b] for b in digest)


def randomart(data: bytes, label: str = "") -> str:
    """Return an OpenSSH-style "Drunken Bishop" ASCII art fingerprint.

    The grid is 17 columns by 9 rows.  The bishop starts in the centre and
    makes four moves per input byte, using two bits per move.  The resulting
    grid shows how often each square was visited.
    """
    width, height = 17, 9
    start_x, start_y = width // 2, height // 2
    grid: list[list[int]] = [[0] * width for _ in range(height)]
    x, y = start_x, start_y

    for byte in data:
        for shift in range(0, 8, 2):
            move = (byte >> shift) & 0x3
            if move == 0:  # up-left
                y = max(0, y - 1)
                x = max(0, x - 1)
            elif move == 1:  # up-right
                y = max(0, y - 1)
                x = min(width - 1, x + 1)
            elif move == 2:  # down-left
                y = min(height - 1, y + 1)
                x = max(0, x - 1)
            else:  # down-right
                y = min(height - 1, y + 1)
                x = min(width - 1, x + 1)
            grid[y][x] += 1

    end_x, end_y = x, y
    grid[start_y][start_x] = 0
    grid[end_y][end_x] = len(_RANDOMART_VALUES) - 1

    header = f"+--[{label:14s}]+" if label else "+----------------+"
    footer = "+----------------+"
    lines = [header]
    for row_idx, row in enumerate(grid):
        line_chars = ["|"]
        for col_idx, count in enumerate(row):
            if row_idx == start_y and col_idx == start_x:
                line_chars.append("S")
            elif row_idx == end_y and col_idx == end_x:
                line_chars.append("E")
            else:
                line_chars.append(
                    _RANDOMART_VALUES[min(count, len(_RANDOMART_VALUES) - 1)]
                )
        line_chars.append("|")
        lines.append("".join(line_chars))
    lines.append(footer)
    return "\n".join(lines)
