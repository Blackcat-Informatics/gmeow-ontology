<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GTS Verification Output Examples

These examples show the expected shape of verification output for a signed
`gmeow.gts` snapshot.

The concrete key identifiers below come from the in-repository GTS test key,
not the production GMEOW release key. A real release build should show the
release key's OpenPGP fingerprint, emoji hash, labels, and randomart.

## Build a Local Signed Example

For local smoke testing, sign the bundled snapshot path with the GTS fixture
key:

```bash
uv run --package gmeow-dev gmeow-dev gts compile-full \
  --sign-key packages/gts/tests/fixtures/test_key.sec.asc \
  --public-key packages/gts/tests/fixtures/test_key.pub.asc \
  -o dist/gmeow.gts
```

Expected signing summary:

```text
✓ dist/gmeow.gts (7076451 bytes)
✓ signed with kid 93F32F9F1439F0FBA266331B6F4732092D747581
```

## `gmeow verify`

`gmeow verify` checks GTS signatures and also runs source-free ontology checks
over the bundled graph:

```bash
COLUMNS=120 uv run gmeow verify dist/gmeow.gts
```

Example output:

```text
╭───────────────────────────────────────────── GTS Signature Verification ─────────────────────────────────────────────╮
│ snapshot       dist/gmeow.gts                                                                                        │
│ signatures     69 signed, 69 valid, 0 invalid, 0 unverified                                                          │
│ transport key  93F3 2F9F 1439 F0FB A266 331B 6F47 3209 2D74 7581                                                     │
│ emoji hash     🐷 🦆 🐵 🦋 🍎 🍐 🦊 🐸 🐟 🍒 🍎                                                                      │
│ emoji labels   pig duck monkey butterfly apple pear fox frog fish cherries apple                                     │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
+--[GTS transport ]+
|      =  .o .    |
|     = +   o . ..|
|      * . .   .o+|
|     . + o     +*|
|    . o S   . +.=|
|     o     . . BE|
|          .   = O|
|           . o X=|
|           .*+@O=|
+----------------+
                        Bundled Ontology Checks
┏━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ Check               ┃ Status ┃ Detail                                ┃
┡━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━╇━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┩
│ reader diagnostics  │ pass   │ 0 found                               │
│ ontology namespace  │ pass   │ https://blackcatinformatics.ca/gmeow/ │
│ term catalog        │ pass   │ 4040 terms                            │
│ labels              │ pass   │ 0 missing                             │
│ definitions         │ pass   │ 0 missing                             │
│ documentation blobs │ pass   │ 67 blobs                              │
└─────────────────────┴────────┴───────────────────────────────────────┘
verification passed
```

## `gmeow gts verify`

`gmeow gts verify` is the lower-level signature check. It omits the ontology
bundle checks:

```bash
COLUMNS=120 uv run gmeow gts verify dist/gmeow.gts
```

Example output:

```text
transport key: 93F3 2F9F 1439 F0FB A266 331B 6F47 3209 2D74 7581
emoji hash:    🐷 🦆 🐵 🦋 🍎 🍐 🦊 🐸 🐟 🍒 🍎
emoji labels:  pig duck monkey butterfly apple pear fox frog fish cherries apple
+--[GTS transport ]+
|      =  .o .    |
|     = +   o . ..|
|      * . .   .o+|
|     . + o     +*|
|    . o S   . +.=|
|     o     . . BE|
|          .   = O|
|           . o X=|
|           .*+@O=|
+----------------+
signatures: 69 signed, 69 valid, 0 invalid, 0 unverified
verification passed
```

## `gts verify`

The standalone `gts` CLI reports the GTS composition ledger. For signed files,
the `sigs` count should be nonzero:

```bash
COLUMNS=120 uv run gts verify dist/gmeow.gts
```

Example output:

```text
dist/gmeow.gts: 1 segment(s)
  segment 0: head deaafb30d8c5d90e4e04c2637ff0fdb0fcb9879317a6b5e65095b51c77a4d8bb profile dist terms 18105 quads 32954 reifies 116 annot 324 blobs 67 suppress 0 opaque 0 sigs 69
```

## What to Compare

- `signatures` must report all signed frames as valid.
- `transport key` is the grouped OpenPGP fingerprint for copy/paste comparison.
- `emoji hash` is a short visual checksum over the raw Ed25519 public key.
- `emoji labels` are the stable names for the emoji hash and are the preferred
  way to read it aloud or compare it in text-only contexts.
- `randomart` is the OpenSSH-style visual fingerprint for the same public key.
