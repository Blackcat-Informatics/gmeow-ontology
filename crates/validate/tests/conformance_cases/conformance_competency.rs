// SPDX-License-Identifier: AGPL-3.0-only

//! Clock-relative conformance twin migrated from tests/test_competency.py
//!
//! Ports the ONE deliberately-retained clock-relative competency question:
//! `expertise-expiring-credentials.rq`. Its window is `[NOW(), ~NOW()+1yr]`
//! computed IN the query from `NOW()`/`year()`/`month()` builtins, so no static
//! fixture date can satisfy it perpetually. The twin therefore builds its ABox
//! at run time from the current clock (via `std::time::SystemTime`), keeps
//! `NOW()` in the query text (that is the surface under test), and asserts
//! SET-MEMBERSHIP of the credential subject — never an exact date value.

use crate::conformance_support::*;

use std::time::{SystemTime, UNIX_EPOCH};

const EX: &str = "https://example.org/test/";

fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

/// Convert a count of days since the Unix epoch (1970-01-01) into a proleptic
/// Gregorian `(year, month, day)`. Howard Hinnant's `civil_from_days` algorithm
/// — exact integer arithmetic, no external date crate. `days` for any plausible
/// wall-clock time is well within `i64` range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = y + i64::from(m <= 2);
    (year, m as u32, d as u32)
}

/// An `xsd:dateTime` literal at midnight UTC for `secs` seconds since the epoch.
fn xsd_datetime_utc(secs: i64) -> String {
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}T00:00:00Z")
}

/// Twin of `test_competency_expertise_expiring_credentials_query`.
///
/// Clock-relative: the query selects credentials whose `gmeow:validUntil` is in
/// the future AND within ~one calendar year of `NOW()`. We build the fixture from
/// the current clock so it provably brackets the window on every run.
///
/// Boundary safety: `validUntil = now + 180 days` (~6 months). In the same
/// calendar year it satisfies `year(validUntil) < nowYear + 1`; when +180 days
/// crosses into next year (only when `nowMonth >= ~7`), the landing month is
/// `nowMonth - 6 <= nowMonth`, so it still satisfies the
/// `year == nowYear+1 && month <= nowMonth` branch. Either way it stays inside
/// the window regardless of the month/year boundary. The 180-day cushion also
/// keeps `validUntil >= NOW()` strictly true despite the small gap between
/// building the fixture and the query evaluating `NOW()` — no near-boundary flake.
#[gmeow_test_batch_macros::batch_test]
fn competency_expertise_expiring_credentials_query() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_secs() as i64;
    let expires_soon = xsd_datetime_utc(now + 180 * 86_400);

    // Standalone ABox (the Python original ran over a bare in-memory graph, not
    // the merged ontology). Built in memory and parsed via `parse_ttl` — no file
    // is written.
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix ex: <{EX}> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         ex:cred1 a gmeow:Credential ;\n\
             gmeow:credentialIssuer ex:amazon ;\n\
             gmeow:validUntil \"{expires_soon}\"^^xsd:dateTime .\n\
         ex:amazon a gmeow:Organization .\n"
    );

    // NOW() stays IN the query (the surface under test); assert set-membership of
    // the credential subject, never an exact date value.
    QueryCase::new(
        "competency/expertise-expiring-credentials",
        &[Feature::Bind],
    )
    .over_raw_ttl(ttl)
    .query_file("expertise-expiring-credentials.rq")
    .column_superset("credential", vec![iri(&ex("cred1"))])
    .run();
}
