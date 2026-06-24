// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The XSD temporal value space: `dateTime`, `date`, `time`, and `duration` (with
//! its `dayTimeDuration`/`yearMonthDuration` subtypes).
//!
//! Comparison follows XSD's **partial order**: dateTime/date/time values carry an
//! optional timezone; a value without a timezone is compared against one with a
//! timezone via the ±14:00 rule, which is **indeterminate** (`None`) in the
//! overlap. `duration` has a two-component (months, seconds) partial order: a pair
//! like `P1M` vs `P30D` is genuinely incomparable. The hand-rolled calendar uses a
//! proleptic Gregorian day count (valid for negative years), no external deps.

use std::cmp::Ordering;

use crate::datatype::XsdDatatype;
use crate::numeric::{parse_decimal, Decimal};
use crate::value::XsdError;

/// Maximum timezone offset magnitude in minutes (±14:00).
const MAX_TZ_MIN: i32 = 14 * 60;
/// ±14:00 expressed in seconds, for the no-timezone comparison bound.
const TZ_BOUND_SECS: i128 = 14 * 3600;
const SECS_PER_DAY: i128 = 86_400;

/// `xsd:dateTime`.
#[derive(Debug, Clone)]
pub struct DateTime {
    year: i64,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: Decimal,
    /// Timezone offset in minutes; `None` = no timezone.
    tz: Option<i32>,
}

/// `xsd:date`.
#[derive(Debug, Clone)]
pub struct Date {
    year: i64,
    month: u8,
    day: u8,
    tz: Option<i32>,
}

/// `xsd:time`.
#[derive(Debug, Clone)]
pub struct Time {
    hour: u8,
    minute: u8,
    second: Decimal,
    tz: Option<i32>,
}

/// `xsd:duration` and its `dayTimeDuration`/`yearMonthDuration` subtypes. The value
/// space is the pair (months, seconds); `datatype` records which lexical subtype it
/// was parsed as (for `canonical_lexical` and `datatype()`).
#[derive(Debug, Clone)]
pub struct Duration {
    months: i64,
    seconds: Decimal,
    datatype: XsdDatatype,
}

impl Duration {
    /// The originating XSD datatype (`Duration`/`DayTimeDuration`/`YearMonthDuration`).
    #[must_use]
    pub fn datatype(&self) -> XsdDatatype {
        self.datatype
    }
}

// ── Proleptic Gregorian calendar (Howard Hinnant's algorithm) ────────────────────

/// Days since 1970-01-01 for a proleptic-Gregorian civil date (valid for any year,
/// including negative). `m` in 1..=12, `d` in 1..=31.
fn days_from_civil(y: i64, m: u8, d: u8) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1; // [0,365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719_468
}

// ── Parsing ──────────────────────────────────────────────────────────────────────

fn invalid(dt: XsdDatatype, lexical: &str, reason: &'static str) -> XsdError {
    XsdError::InvalidLexical {
        datatype: dt,
        lexical: lexical.to_string(),
        reason,
    }
}

/// Split a trailing timezone (`Z`, `+hh:mm`, `-hh:mm`) off the time portion. Returns
/// `(body_without_tz, tz_minutes_option)`.
fn split_tz(dt: XsdDatatype, lexical: &str, s: &str) -> Result<(String, Option<i32>), XsdError> {
    if let Some(body) = s.strip_suffix('Z') {
        return Ok((body.to_string(), Some(0)));
    }
    // A tz sign is the last '+' or '-' AND must look like "±hh:mm" (len 6).
    if s.len() >= 6 {
        let tail = &s[s.len() - 6..];
        let sign = tail.as_bytes()[0];
        if (sign == b'+' || sign == b'-') && tail.as_bytes()[3] == b':' {
            let hh: i32 = tail[1..3]
                .parse()
                .map_err(|_| invalid(dt, lexical, "bad timezone hour"))?;
            let mm: i32 = tail[4..6]
                .parse()
                .map_err(|_| invalid(dt, lexical, "bad timezone minute"))?;
            if hh > 14 || mm > 59 {
                return Err(invalid(dt, lexical, "timezone out of range"));
            }
            let mut off = hh * 60 + mm;
            if sign == b'-' {
                off = -off;
            }
            if off.abs() > MAX_TZ_MIN {
                return Err(invalid(dt, lexical, "timezone exceeds ±14:00"));
            }
            return Ok((s[..s.len() - 6].to_string(), Some(off)));
        }
    }
    Ok((s.to_string(), None))
}

/// Number of days in a given month for a proleptic-Gregorian year.
/// Uses the signed year directly; negative years follow the same leap-year rule as
/// positive ones (proleptic Gregorian: leap iff divisible by 4, except centuries
/// unless also divisible by 400).
fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            if is_leap {
                29
            } else {
                28
            }
        }
        _ => 0, // invalid month — caught by caller before reaching here
    }
}

/// Parse `[-]YYYY[Y...]-MM-DD` into `(year, month, day)`.
fn parse_ymd(dt: XsdDatatype, lexical: &str, s: &str) -> Result<(i64, u8, u8), XsdError> {
    let neg = s.starts_with('-');
    let body = if neg { &s[1..] } else { s };
    let parts: Vec<&str> = body.split('-').collect();
    if parts.len() != 3 {
        return Err(invalid(dt, lexical, "expected YYYY-MM-DD"));
    }
    if parts[0].len() < 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err(invalid(dt, lexical, "bad date field widths"));
    }
    let year_mag: i64 = parts[0]
        .parse()
        .map_err(|_| invalid(dt, lexical, "bad year"))?;
    let month: u8 = parts[1]
        .parse()
        .map_err(|_| invalid(dt, lexical, "bad month"))?;
    let day: u8 = parts[2]
        .parse()
        .map_err(|_| invalid(dt, lexical, "bad day"))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(invalid(dt, lexical, "month/day out of range"));
    }
    let year = if neg { -year_mag } else { year_mag };
    if day > days_in_month(year, month) {
        return Err(invalid(dt, lexical, "day out of range for month"));
    }
    Ok((year, month, day))
}

/// Parse `hh:mm:ss(.fff)?` into `(hour, minute, second)`.
fn parse_hms(dt: XsdDatatype, lexical: &str, s: &str) -> Result<(u8, u8, Decimal), XsdError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(invalid(dt, lexical, "expected hh:mm:ss"));
    }
    if parts[0].len() != 2 || parts[1].len() != 2 {
        return Err(invalid(dt, lexical, "bad time field widths"));
    }
    let hour: u8 = parts[0]
        .parse()
        .map_err(|_| invalid(dt, lexical, "bad hour"))?;
    let minute: u8 = parts[1]
        .parse()
        .map_err(|_| invalid(dt, lexical, "bad minute"))?;
    // Reject a trailing-dot seconds lexical (e.g. "00.") — parse_decimal accepts it
    // as a valid decimal ("1.0") but it is not a valid XSD time seconds field.
    if parts[2].ends_with('.') {
        return Err(invalid(dt, lexical, "seconds has trailing decimal point"));
    }
    // Reject a leading sign in the seconds field — seconds must be non-negative.
    if parts[2].starts_with('-') || parts[2].starts_with('+') {
        return Err(invalid(dt, lexical, "seconds must not have a sign"));
    }
    let second = parse_decimal(parts[2]).map_err(|_| invalid(dt, lexical, "bad second"))?;
    // XSD has no leap seconds: seconds must be in [0, 60). Whole part >= 60 is invalid.
    if second.whole_part() >= 60 {
        return Err(invalid(dt, lexical, "seconds out of range (must be < 60)"));
    }
    if minute > 59 {
        return Err(invalid(dt, lexical, "minute out of range"));
    }
    if hour > 24 {
        return Err(invalid(dt, lexical, "hour out of range"));
    }
    // Hour 24 is only valid as exactly 24:00:00 (end-of-day sentinel).
    if hour == 24 && (minute != 0 || !second.is_zero()) {
        return Err(invalid(dt, lexical, "hour 24 is only valid as 24:00:00"));
    }
    Ok((hour, minute, second))
}

/// `xsd:dateTime` = `date 'T' time tz?`.
pub fn parse_datetime(s: &str) -> Result<DateTime, XsdError> {
    let dt = XsdDatatype::DateTime;
    let (date_part, time_part) = s
        .split_once('T')
        .ok_or_else(|| invalid(dt, s, "missing 'T'"))?;
    let (time_no_tz, tz) = split_tz(dt, s, time_part)?;
    let (year, month, day) = parse_ymd(dt, s, date_part)?;
    let (hour, minute, second) = parse_hms(dt, s, &time_no_tz)?;
    Ok(DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz,
    })
}

/// `xsd:date` = `date tz?`.
pub fn parse_date(s: &str) -> Result<Date, XsdError> {
    let dt = XsdDatatype::Date;
    let (body, tz) = split_tz(dt, s, s)?;
    let (year, month, day) = parse_ymd(dt, s, &body)?;
    Ok(Date {
        year,
        month,
        day,
        tz,
    })
}

/// `xsd:time` = `time tz?`.
pub fn parse_time(s: &str) -> Result<Time, XsdError> {
    let dt = XsdDatatype::Time;
    let (body, tz) = split_tz(dt, s, s)?;
    let (hour, minute, second) = parse_hms(dt, s, &body)?;
    Ok(Time {
        hour,
        minute,
        second,
        tz,
    })
}

/// `xsd:duration` and subtypes: `[-]PnYnMnDTnHnMnS` (any component group optional,
/// at least one present; the `T` separates date from time components).
pub fn parse_duration(dt: XsdDatatype, s: &str) -> Result<Duration, XsdError> {
    let neg = s.starts_with('-');
    let body = s.strip_prefix('-').unwrap_or(s);
    let body = body
        .strip_prefix('P')
        .ok_or_else(|| invalid(dt, s, "duration must start with 'P'"))?;
    let (date_part, time_part) = match body.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (body, None),
    };

    let mut months: i64 = 0;
    let mut seconds = 0i128; // whole seconds accumulator
    let mut sec_frac = Decimal::from_parts(0, 0);
    let mut any = false;

    // Date components: nY nM nD.
    let mut num = String::new();
    for ch in date_part.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
        } else {
            let n: i64 = num
                .parse()
                .map_err(|_| invalid(dt, s, "bad duration number"))?;
            num.clear();
            any = true;
            match ch {
                'Y' => months += n * 12,
                'M' => months += n,
                'D' => seconds += i128::from(n) * SECS_PER_DAY,
                _ => return Err(invalid(dt, s, "bad duration date component")),
            }
        }
    }
    if !num.is_empty() {
        return Err(invalid(dt, s, "dangling number in duration date part"));
    }

    // Time components: nH nM n(.f)S.
    if let Some(time_part) = time_part {
        let mut tnum = String::new();
        for (i, ch) in time_part.char_indices() {
            if ch.is_ascii_digit() || ch == '.' {
                tnum.push(ch);
            } else {
                any = true;
                match ch {
                    'H' => {
                        seconds += i128::from(
                            tnum.parse::<i64>()
                                .map_err(|_| invalid(dt, s, "bad hours"))?,
                        ) * 3600;
                    }
                    'M' => {
                        seconds += i128::from(
                            tnum.parse::<i64>()
                                .map_err(|_| invalid(dt, s, "bad minutes"))?,
                        ) * 60;
                    }
                    'S' => {
                        let d = parse_decimal(&tnum).map_err(|_| invalid(dt, s, "bad seconds"))?;
                        seconds += d.whole_part();
                        sec_frac = d.frac_part();
                        if i != time_part.len() - 1 {
                            return Err(invalid(dt, s, "'S' must be last"));
                        }
                    }
                    _ => return Err(invalid(dt, s, "bad duration time component")),
                }
                tnum.clear();
            }
        }
        if !tnum.is_empty() {
            return Err(invalid(dt, s, "dangling number in duration time part"));
        }
    }
    if !any {
        return Err(invalid(dt, s, "duration has no components"));
    }

    // Combine whole + fractional seconds into one Decimal at the fraction's scale.
    let scale = sec_frac.scale();
    let combined = seconds
        .checked_mul(10i128.pow(u32::from(scale)))
        .and_then(|w| w.checked_add(sec_frac.mantissa()))
        .ok_or_else(|| XsdError::OutOfRange {
            datatype: dt,
            lexical: s.to_string(),
        })?;
    let mut total_secs = Decimal::from_parts(combined, scale);
    if neg {
        months = -months;
        total_secs = Decimal::from_parts(-total_secs.mantissa(), total_secs.scale());
    }
    Ok(Duration {
        months,
        seconds: total_secs,
        datatype: dt,
    })
}

// ── Comparison (XSD partial order) ───────────────────────────────────────────────

/// The naive whole-seconds offset (timezone NOT applied) on the proleptic timeline.
fn naive_secs(days: i64, hour: u8, minute: u8, sec_whole: i128) -> i128 {
    i128::from(days) * SECS_PER_DAY + i128::from(hour) * 3600 + i128::from(minute) * 60 + sec_whole
}

/// Compare two `(whole_secs, frac)` points.
fn cmp_point(a_whole: i128, a_frac: &Decimal, b_whole: i128, b_frac: &Decimal) -> Ordering {
    a_whole.cmp(&b_whole).then_with(|| a_frac.cmp_exact(b_frac))
}

/// The XSD partial-order comparison for a timezoned point: naive whole seconds +
/// fractional seconds + optional timezone (minutes). `None` = indeterminate.
fn cmp_timeline(
    a_naive: i128,
    a_frac: &Decimal,
    a_tz: Option<i32>,
    b_naive: i128,
    b_frac: &Decimal,
    b_tz: Option<i32>,
) -> Option<Ordering> {
    match (a_tz, b_tz) {
        (Some(ta), Some(tb)) => Some(cmp_point(
            a_naive - i128::from(ta) * 60,
            a_frac,
            b_naive - i128::from(tb) * 60,
            b_frac,
        )),
        (None, None) => Some(cmp_point(a_naive, a_frac, b_naive, b_frac)),
        (None, Some(tb)) => {
            let b_utc = b_naive - i128::from(tb) * 60;
            // a's UTC instant lies in [a_naive - 14h, a_naive + 14h].
            if cmp_point(a_naive + TZ_BOUND_SECS, a_frac, b_utc, b_frac) == Ordering::Less {
                Some(Ordering::Less)
            } else if cmp_point(a_naive - TZ_BOUND_SECS, a_frac, b_utc, b_frac) == Ordering::Greater
            {
                Some(Ordering::Greater)
            } else {
                None
            }
        }
        (Some(_), None) => {
            cmp_timeline(b_naive, b_frac, b_tz, a_naive, a_frac, a_tz).map(Ordering::reverse)
        }
    }
}

/// Compare two `dateTime` values (XSD partial order).
#[must_use]
pub fn cmp_datetime(a: &DateTime, b: &DateTime) -> Option<Ordering> {
    let an = naive_secs(
        days_from_civil(a.year, a.month, a.day),
        a.hour,
        a.minute,
        a.second.whole_part(),
    );
    let bn = naive_secs(
        days_from_civil(b.year, b.month, b.day),
        b.hour,
        b.minute,
        b.second.whole_part(),
    );
    cmp_timeline(
        an,
        &a.second.frac_part(),
        a.tz,
        bn,
        &b.second.frac_part(),
        b.tz,
    )
}

/// Compare two `date` values (XSD partial order; midnight on the proleptic timeline).
#[must_use]
pub fn cmp_date(a: &Date, b: &Date) -> Option<Ordering> {
    let zero = Decimal::from_parts(0, 0);
    let an = naive_secs(days_from_civil(a.year, a.month, a.day), 0, 0, 0);
    let bn = naive_secs(days_from_civil(b.year, b.month, b.day), 0, 0, 0);
    cmp_timeline(an, &zero, a.tz, bn, &zero, b.tz)
}

/// Compare two `time` values (XSD partial order; within a single notional day).
#[must_use]
pub fn cmp_time(a: &Time, b: &Time) -> Option<Ordering> {
    let an = naive_secs(0, a.hour, a.minute, a.second.whole_part());
    let bn = naive_secs(0, b.hour, b.minute, b.second.whole_part());
    cmp_timeline(
        an,
        &a.second.frac_part(),
        a.tz,
        bn,
        &b.second.frac_part(),
        b.tz,
    )
}

/// Compare two `duration` values: a two-component partial order over (months,
/// seconds). Agreement on both components gives the order; disagreement is
/// indeterminate (`None`). Totally-ordered subtypes (`dayTimeDuration` with
/// months = 0, `yearMonthDuration` with seconds = 0) always resolve.
#[must_use]
pub fn cmp_duration(a: &Duration, b: &Duration) -> Option<Ordering> {
    let m = a.months.cmp(&b.months);
    let s = a.seconds.cmp_exact(&b.seconds);
    match (m, s) {
        (Ordering::Equal, Ordering::Equal) => Some(Ordering::Equal),
        (Ordering::Equal, o) | (o, Ordering::Equal) => Some(o),
        (a, b) if a == b => Some(a),
        _ => None,
    }
}

// ── Canonical lexical mapping ────────────────────────────────────────────────────

fn fmt_year(year: i64) -> String {
    if year < 0 {
        format!("-{:04}", -year)
    } else {
        format!("{year:04}")
    }
}

fn fmt_tz(tz: Option<i32>) -> String {
    match tz {
        None => String::new(),
        Some(0) => "Z".to_string(),
        Some(off) => {
            let sign = if off < 0 { '-' } else { '+' };
            let a = off.abs();
            format!("{sign}{:02}:{:02}", a / 60, a % 60)
        }
    }
}

/// Canonical seconds field: two integer digits, fractional part trimmed of trailing
/// zeros (and dropped entirely if zero).
fn fmt_seconds(sec: &Decimal) -> String {
    let whole = sec.whole_part();
    let frac = sec.frac_part();
    if frac.is_zero() {
        format!("{whole:02}")
    } else {
        // `canonical_lexical` yields e.g. "0.5"; take the fractional digits.
        let canon = frac.canonical_lexical();
        let digits = canon.split_once('.').map_or("", |(_, f)| f);
        format!("{whole:02}.{digits}")
    }
}

impl DateTime {
    /// XSD canonical lexical form.
    #[must_use]
    pub fn canonical_lexical(&self) -> String {
        format!(
            "{}-{:02}-{:02}T{:02}:{:02}:{}{}",
            fmt_year(self.year),
            self.month,
            self.day,
            self.hour,
            self.minute,
            fmt_seconds(&self.second),
            fmt_tz(self.tz),
        )
    }
}

impl Date {
    /// XSD canonical lexical form.
    #[must_use]
    pub fn canonical_lexical(&self) -> String {
        format!(
            "{}-{:02}-{:02}{}",
            fmt_year(self.year),
            self.month,
            self.day,
            fmt_tz(self.tz)
        )
    }
}

impl Time {
    /// XSD canonical lexical form.
    #[must_use]
    pub fn canonical_lexical(&self) -> String {
        format!(
            "{:02}:{:02}:{}{}",
            self.hour,
            self.minute,
            fmt_seconds(&self.second),
            fmt_tz(self.tz)
        )
    }
}

impl Duration {
    /// Canonical lexical form `[-]PnYnMnDTnHnMnS` (general duration grammar).
    #[must_use]
    pub fn canonical_lexical(&self) -> String {
        let neg = self.months < 0 || self.seconds.mantissa() < 0;
        let months = self.months.unsigned_abs();
        let years = months / 12;
        let rem_months = months % 12;
        let total_secs = self.seconds.whole_part().unsigned_abs();
        let frac = self.seconds.frac_part();
        let days = total_secs / 86_400;
        let rem = total_secs % 86_400;
        let hours = rem / 3600;
        let mins = (rem % 3600) / 60;
        let secs = rem % 60;

        let mut out = String::new();
        if neg {
            out.push('-');
        }
        out.push('P');
        if years > 0 {
            out.push_str(&format!("{years}Y"));
        }
        if rem_months > 0 {
            out.push_str(&format!("{rem_months}M"));
        }
        if days > 0 {
            out.push_str(&format!("{days}D"));
        }
        let has_time = hours > 0 || mins > 0 || secs > 0 || !frac.is_zero();
        if has_time {
            out.push('T');
            if hours > 0 {
                out.push_str(&format!("{hours}H"));
            }
            if mins > 0 {
                out.push_str(&format!("{mins}M"));
            }
            if secs > 0 || !frac.is_zero() {
                if frac.is_zero() {
                    out.push_str(&format!("{secs}S"));
                } else {
                    let canon = frac.canonical_lexical();
                    let digits = canon.split_once('.').map_or("", |(_, f)| f);
                    out.push_str(&format!("{secs}.{digits}S"));
                }
            }
        }
        // The zero duration canonicalizes to "PT0S".
        if out == "P" || out == "-P" {
            out.push_str("T0S");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn datetime_parse_canonical_roundtrip() {
        let dt = parse_datetime("2024-02-29T12:30:00Z").unwrap();
        assert_eq!(dt.canonical_lexical(), "2024-02-29T12:30:00Z");
        let dt = parse_datetime("2024-02-29T12:30:00.500-05:00").unwrap();
        assert_eq!(dt.canonical_lexical(), "2024-02-29T12:30:00.5-05:00");
        let dt = parse_datetime("-0044-03-15T12:00:00").unwrap();
        assert_eq!(dt.canonical_lexical(), "-0044-03-15T12:00:00");
    }

    #[test]
    fn datetime_ordering_with_timezones() {
        let a = parse_datetime("2024-01-01T00:00:00Z").unwrap();
        let b = parse_datetime("2024-01-01T01:00:00+01:00").unwrap(); // same instant
        assert_eq!(cmp_datetime(&a, &b), Some(Ordering::Equal));
        let c = parse_datetime("2024-01-01T00:00:01Z").unwrap();
        assert_eq!(cmp_datetime(&a, &c), Some(Ordering::Less));
    }

    #[test]
    fn datetime_indeterminate_when_one_lacks_timezone() {
        // No tz vs a tz'd value within the ±14h overlap → indeterminate.
        let no_tz = parse_datetime("2024-01-01T12:00:00").unwrap();
        let tzd = parse_datetime("2024-01-01T12:00:00Z").unwrap();
        assert_eq!(cmp_datetime(&no_tz, &tzd), None);
        // Far enough apart → determinate.
        let early = parse_datetime("2024-01-01T00:00:00").unwrap();
        let late_z = parse_datetime("2024-01-02T20:00:00Z").unwrap();
        assert_eq!(cmp_datetime(&early, &late_z), Some(Ordering::Less));
    }

    #[test]
    fn duration_partial_order() {
        let p1y = parse_duration(XsdDatatype::Duration, "P1Y").unwrap();
        let p13m = parse_duration(XsdDatatype::Duration, "P13M").unwrap();
        assert_eq!(cmp_duration(&p1y, &p13m), Some(Ordering::Less)); // 12mo < 13mo
                                                                     // P1M vs P30D: months differ one way, seconds the other → indeterminate.
        let p1m = parse_duration(XsdDatatype::Duration, "P1M").unwrap();
        let p30d = parse_duration(XsdDatatype::Duration, "P30D").unwrap();
        assert_eq!(cmp_duration(&p1m, &p30d), None);
        // dayTimeDuration is totally ordered.
        let h1 = parse_duration(XsdDatatype::DayTimeDuration, "PT1H").unwrap();
        let h2 = parse_duration(XsdDatatype::DayTimeDuration, "PT2H").unwrap();
        assert_eq!(cmp_duration(&h1, &h2), Some(Ordering::Less));
    }

    #[test]
    fn duration_canonical() {
        assert_eq!(
            parse_duration(XsdDatatype::Duration, "P1Y2M3DT4H5M6S")
                .unwrap()
                .canonical_lexical(),
            "P1Y2M3DT4H5M6S"
        );
        assert_eq!(
            parse_duration(XsdDatatype::Duration, "PT0S")
                .unwrap()
                .canonical_lexical(),
            "PT0S"
        );
        assert_eq!(
            parse_duration(XsdDatatype::DayTimeDuration, "PT1.5S")
                .unwrap()
                .canonical_lexical(),
            "PT1.5S"
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_datetime("2024-01-01").is_err()); // no time
        assert!(parse_date("2024-13-01").is_err()); // month 13
        assert!(parse_time("12:00").is_err()); // no seconds
        assert!(parse_duration(XsdDatatype::Duration, "1Y").is_err()); // no P
        assert!(parse_datetime("2024-01-01T12:00:00+15:00").is_err()); // tz > 14h
    }
}
