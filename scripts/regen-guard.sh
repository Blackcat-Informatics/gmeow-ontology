#!/usr/bin/env bash
# scripts/regen-guard.sh
#
# Steering guard for the `regen` make target.
#
# `make regen` only REGENERATES; it runs no gate, and a direct human/agent
# invocation is almost always a mistake that costs a full pipeline pass. The
# guard therefore HARD-FAILS a bare direct invocation, and exits silently for
# every legitimate caller:
#
#   REGEN_INTERNAL      an in-Makefile caller that must run at the same MAKELEVEL
#   GMEOW_MAKELEVEL!=0  a sub-make from install/build/docs/release recursion
#   CI               continuous integration (GitHub Actions always sets CI=true);
#                    the `generation` job invokes `make regen` directly, so a
#                    hard fail here would break CI by construction
#   REGEN_ACK        the deliberate human escape the banner advertises
#
# This lives in shell rather than Rust on purpose: `make regen` is the
# clean-clone bootstrap path, where no generated bundle exists yet and the
# consumer CLIs therefore cannot compile. A guard whose whole job is to refuse
# in milliseconds must not require `cargo build` first. Its BEHAVIOUR is proven
# in Rust by crates/xtask/tests/regen_guard.rs.
#
# GMEOW_MAKELEVEL, not MAKELEVEL: GNU make exports MAKELEVEL to a recipe's child
# processes ALREADY INCREMENTED, so a recipe of the TOP-LEVEL make sees
# MAKELEVEL=1 in its environment while `$(MAKELEVEL)` expands to 0. Reading the
# environment variable directly would therefore never fire the guard. The
# Makefile passes make's own expansion under this unambiguous name. Unset means
# "invoked directly", which is fail-closed.
set -euo pipefail

if [ -n "${REGEN_INTERNAL:-}" ]; then exit 0; fi
if [ "${GMEOW_MAKELEVEL:-0}" != "0" ]; then exit 0; fi
if [ -n "${CI:-}" ]; then exit 0; fi
if [ -n "${REGEN_ACK:-}" ]; then exit 0; fi

printf '\033[1;33m%s\033[0m\n' \
  "──────────────────────────────────────────────────────────────────────" \
  "NOTE: 'make regen' only REGENERATES generated/ + the bundle — it does NOT" \
  "run any gate. You almost never need it directly: 'make check' ALREADY" \
  "syncs (CHECK_SYNC_MODE=update) and THEN runs the full gate, so 'make regen'" \
  "before 'make check' just regenerates twice. Run 'make regen' alone ONLY for" \
  "a clean-clone bootstrap or a regen-without-gate. To verify work: make check" \
  "" \
  "To run it anyway: REGEN_ACK=1 make regen" \
  "──────────────────────────────────────────────────────────────────────" >&2
exit 1
