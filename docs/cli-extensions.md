<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# CLI extension roll-up: `gmeow` discovers installed slice CLIs

> **Status:** specification (issue #326, design locked 2026-06-11 under #287).
> **Implementation is deferred** until a second real extension CLI exists
> (Principle 15) — Music's render tooling (#319) is the natural first
> consumer, `gmeow-image` the worked exemplar throughout. This document and
> the manifest fields it consumes are the deliverable; when the first
> extension CLI ships, no re-architecture is required.

## The vision

```sh
apt-get install gmeow gmeow-image gmeow-music
gmeow image.gts -o test.gif          # just works
gmeow-image my.gts --format-to image/jpeg+lossless
gmeow-image my.gts --add-frame-transform 'sample-to:800x600,rotate:90right,colorspace:sRGB'
gmeow music.gts                      # → "this data requires the gmeow-music profile:
                                     #    apt-get install gmeow-music"
```

One base command, independently installable extensions, and data files that
name their own dependencies.

## 1. Discovery and dispatch

An extension slice declares the subcommands it contributes in its
`manifest.ttl` (#287) with **`gmeow:providesSubcommand`** (multi-valued,
defined in `slices/vocabulary.ttl`, validated structurally by
`shapes/slice-manifest-shapes.ttl`):

```turtle
<https://blackcatinformatics.ca/gmeow/slices/image>
    gmeow:providesSubcommand "image" .
```

Installed extensions register with the base command in two phases:

- **Day one (pip):** Python entry points in the group `gmeow.subcommands` —
  the entry-point name is the subcommand, the target is a `main(argv) -> int`
  callable. The manifest declaration and the entry point MUST agree; the
  roll-up refuses a subcommand whose manifest does not declare it
  (declarations a tool reads must not rot against the code that backs them —
  the GTS-SPEC §14.1 posture applied to packaging).
- **Packaged binaries (later):** drop-in manifests under a well-known
  directory (`/usr/share/gmeow/extensions/*.manifest.ttl` or
  `$XDG_DATA_HOME/gmeow/extensions/`), each naming the executable
  (`gmeow-<subcommand>` on `$PATH` by convention).

Dispatch is mechanical: `gmeow image …` execs the `image` provider with the
remaining argv. `gmeow <file.gts>` with no subcommand consults profile gating
(next section) to pick or recommend one. The base `gmeow` command remains
fully useful alone (core slices, `describe`, the `gts` file tools) —
Principle 13.

## 2. Profile gating is metadata, not error handling

A GTS file's header declares the profiles its data requires — the `"prof"`
field (GTS-SPEC §5, §13) and, for slice-built packages, the multi-valued
`gmeow:sliceProfile` names that double as GTS sub-profile tags (#330 twin
doctrine: one name is both the OWL composition profile IRI and the package
tag). **The data file is its own dependency manifest.**

The base CLI reads the header — a header read, not a fold — and:

- if an installed extension provides the profile, dispatches to it;
- if not, reports the missing extension **by name**:
  `this data requires the gmeow-music profile: apt-get install gmeow-music`.

A reader without the profile installed degrades gracefully, never errors:
the unhandled segments fold to opaque nodes with the profile named in the
diagnostics (GTS-SPEC §7.6 and the §3.1 profile-union rule; vector 19 pins
the behaviour). Missing capability is a *reported state*, not a crash —
exactly the §8.3 capability model surfaced at the CLI.

## 3. Transforms are solver-layer (Principle 12)

`--add-frame-transform 'sample-to:800x600,rotate:90right,colorspace:sRGB'`
never mutates source frames. Each transform step is a **declared FnO
function**; the tool computes the derived representation and **appends** a
new frame carrying it, with provenance quads linking derivation → function →
source digest (the same maximal-projection discipline as every other GMEOW
projection). The source bytes, chain, and signatures are untouched —
append-only, Principle 10 at the wire level.

Interaction with layout states (GTS-SPEC §3.3): appending derived frames to
a streamable-compacted file is legal — the additions are the segment's
accretive tail, and the file reports "streamable through frame *N*,
accretive after". Re-run `gts compact --streamable` to re-streamline a
delivery copy.

## 4. Packaging

Each extension is independently installable and independently useful:

- **Now:** pip distributions (`gmeow-image`), optionally aggregated as
  extras (`pip install gmeow[image]`). The extension depends on `gmeow`;
  never the reverse.
- **Later:** OS packages (`apt-get install gmeow-image`) shipping the
  binary + drop-in manifest.

`gmeow` alone MUST stay a complete tool; an extension MUST NOT be required
for any core slice workflow.

## 5. Versioning

An extension's manifest declares the core version range it was built against
— **`gmeow:builtAgainstCore`** (`slices/vocabulary.ttl`; required for
third-party slices). At dispatch the roll-up compares the declaration with
the installed core's `owl:versionInfo` and **warns on mismatch** (it does not
refuse: the ontology's immutable-release discipline, Principle 6, makes
skew detectable and usually harmless; refusal is reserved for the extension
itself when its data contract is actually broken).

## Manifest fields consumed (all already in the #287 schema)

| field | role here |
| --- | --- |
| `gmeow:providesSubcommand` | discovery: the subcommands this slice contributes |
| `gmeow:sliceProfile` | gating: the profile names a package header carries |
| `gmeow:builtAgainstCore` | versioning: the core range the extension targets |
| `gmeow:sliceTier` | only `gmeow:tierExtension` slices may provide subcommands |
| `gmeow:sliceConsumer` | the P15 record of who actually uses the command |

Constitution: **P13** (tools are the interface), **P12** (solver boundary),
**P15** (loader deferred until a named consumer), **P16** (extensions),
**P10** (append-only transforms). Related issues: #287 (manifests), #267
(GTS), #330 (profile twins), #319 (first consumer), #306 (Music EPIC).
