# Retention: `tests/test_competency.py`

**Category:** Static repo guard

## What it tests

Clock-relative competency retain (the rest migrated to native slice-test cells).

Retained dynamic tests:

- `test_competency_expertise_expiring_credentials_query` — Expiring-credentials query returns credentials with a near-future expiry.

## Why it cannot be deleted or moved to Rust today

ONE test is deliberately retained here: ``expertise-expiring-credentials``. Its query selects credentials whose ``gmeow:validUntil`` falls within one year of ``NOW()`` — a clock-RELATIVE window. No static fixture date can satisfy "within a year of now" perpetually: a far-future literal falls outside the window, and any fixed near date becomes a time-bomb that silently reds once wall-clock time passes it. A faithful native cell would need clock-relative date templating the test-DSL deliberately does not have, so this stays a pytest retain that builds its data relative to the current clock at run time (the verification-honesty doctrine: never author a test that silently breaks later).
