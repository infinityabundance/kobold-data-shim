//! KOBOLD.DATE.PROFILE.1 — declared date-format evidence.
//!
//! **Doctrine.** DATE.PROFILE.1 admits only **declared** date-format evidence: a named field may be
//! validated against an explicit profile such as `YYYYMMDD`, while PIC shape, zero/high sentinels (handled
//! by `SENTINEL.PROFILE.1`), Y2K windows, business calendars, settlement/maturity meaning, currentness, and
//! date arithmetic remain non-claims unless separately admitted. The strongest claim is `format_valid_only`.

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// A declared date format. Validation is on the field's **raw digit string** (leading zeros preserved).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DateFormat {
    /// 8 digits: YYYY MM DD, gregorian calendar validity.
    Yyyymmdd,
    /// 5 digits: YY DDD, day-of-year 1..=366.
    Yyddd,
}

/// A declared date-field profile.
pub struct DateFieldProfile<'a> {
    pub field: &'a str,
    pub format: DateFormat,
    /// Require the value to be a declared sentinel (via SENTINEL.PROFILE.1) when it is not a valid date.
    pub require_sentinel_profile: bool,
}

/// The declared date profile.
pub struct DateProfile<'a> {
    pub fields: &'a [DateFieldProfile<'a>],
}

/// The date-validation result.
pub struct DateManifest {
    pub manifest_json: String,
    pub casefile_json: String,
    /// `(field, status)` where status ∈ valid|invalid_format|invalid_calendar|declared_sentinel.
    pub statuses: Vec<(String, String)>,
    pub findings: Vec<(String, String)>,
}

fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|k| u8::from_str_radix(&s[k * 2..k * 2 + 2], 16).unwrap_or(0))
        .collect()
}
fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Validate a raw digit string against a format. Returns the per-field status.
fn validate(raw: &str, fmt: DateFormat) -> &'static str {
    let (need, all_digit) = match fmt {
        DateFormat::Yyyymmdd => (8, raw.len() == 8),
        DateFormat::Yyddd => (5, raw.len() == 5),
    };
    if !all_digit || raw.bytes().any(|b| !b.is_ascii_digit()) || raw.len() != need {
        return "invalid_format";
    }
    match fmt {
        DateFormat::Yyyymmdd => {
            let y: u32 = raw[0..4].parse().unwrap_or(0);
            let m: u32 = raw[4..6].parse().unwrap_or(0);
            let d: u32 = raw[6..8].parse().unwrap_or(0);
            if (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) {
                "valid"
            } else {
                "invalid_calendar"
            }
        }
        DateFormat::Yyddd => {
            let y: u32 = raw[0..2].parse().unwrap_or(0);
            let ddd: u32 = raw[2..5].parse().unwrap_or(0);
            let max = if is_leap(2000 + y) { 366 } else { 365 };
            if (1..=max).contains(&ddd) {
                "valid"
            } else {
                "invalid_calendar"
            }
        }
    }
}

/// Validate declared date fields against their declared formats. `sentinel_hits` are `(field, sentinel_id)`
/// pairs from a prior `SENTINEL.PROFILE.1` scan — a field whose value is a declared sentinel is NOT validated
/// as a date (sentinel handling is delegated).
pub fn date_validate(
    copybook: &str,
    record: &[u8],
    profile: &DateProfile,
    sentinel_hits: &[(String, String)],
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<DateManifest, ShimError> {
    let decoded = decode_record_encoded(copybook, record, resolver, encoding)?;
    let fmap: std::collections::HashMap<&str, &str> = decoded
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.raw_hex.as_str()))
        .collect();

    let mut statuses: Vec<(String, String)> = Vec::new();
    let mut findings: Vec<(String, String)> = Vec::new();
    let mut entries: Vec<String> = Vec::new();

    for fp in profile.fields {
        let is_sentinel = sentinel_hits.iter().any(|(f, _)| f == fp.field);
        let Some(raw_hex) = fmap.get(fp.field) else {
            findings.push((
                "KOBOLD-DATE-MISSING-FIELD".into(),
                format!("declared date field {:?} not found", fp.field),
            ));
            continue;
        };
        // validate the RAW digit string (leading zeros preserved), not the normalized decoded value
        let raw = String::from_utf8_lossy(&unhex(raw_hex)).to_string();
        let status = if is_sentinel {
            "declared_sentinel"
        } else {
            validate(&raw, fp.format)
        };
        match status {
            "invalid_format" => findings.push((
                "KOBOLD-DATE-INVALID-FORMAT".into(),
                format!("{:?}={:?} not the declared format", fp.field, raw),
            )),
            "invalid_calendar" => {
                findings.push((
                    "KOBOLD-DATE-INVALID-CALENDAR-DATE".into(),
                    format!("{:?}={:?} is not a valid calendar date", fp.field, raw),
                ));
                if fp.require_sentinel_profile {
                    findings.push(("KOBOLD-DATE-SENTINEL-UNDECLARED".into(),
                        format!("{:?}={:?} is not a valid date and is not a declared sentinel — declare it in SENTINEL.PROFILE.1", fp.field, raw)));
                }
            }
            _ => {}
        }
        let claim = if status == "valid" {
            "format_valid_only"
        } else {
            "none"
        };
        entries.push(format!(
            "{{\"field\":{},\"format\":{},\"raw\":{},\"status\":{},\"date_meaning_claimed\":{}}}",
            jstr(fp.field),
            jstr(match fp.format {
                DateFormat::Yyyymmdd => "YYYYMMDD",
                DateFormat::Yyddd => "YYDDD",
            }),
            jstr(&raw),
            jstr(status),
            jstr(claim),
        ));
        statuses.push((fp.field.to_string(), status.to_string()));
    }

    let manifest_json = format!(
        "{{\"schema\":\"kobold-date-manifest-v1\",\"court\":\"KOBOLD.DATE.PROFILE.1\",\"record_sha256\":{},\"fields\":[{}]}}",
        jstr(&sha256_hex(record)), entries.join(","),
    );
    let find_json = findings
        .iter()
        .map(|(r, m)| format!("{{\"ruleId\":{},\"message\":{}}}", jstr(r), jstr(m)))
        .collect::<Vec<_>>()
        .join(",");
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-date-forensic-casefile-v1\",\"court\":\"KOBOLD.DATE.PROFILE.1\",",
            "\"manifest\":{},\"findings\":[{}],",
            "\"truth_layers\":{{\"format_valid_evidence\":true,\"business_calendar\":false,\"settlement_date\":false,",
            "\"maturity_date\":false,\"y2k_window\":false,\"currentness\":false,\"date_arithmetic\":false}},",
            "\"negative_capabilities\":[\"NEG.DATE.PIC9_NOT_DATE\",\"NEG.DATE.ZERO_DATE_NOT_NULL\",",
            "\"NEG.DATE.HIGH_DATE_NOT_MAX_DATE\",\"NEG.DATE.Y2K_WINDOW_NOT_INFERRED\",\"NEG.DATE.BUSINESS_CALENDAR_NOT_CLAIMED\",",
            "\"NEG.DATE.SETTLEMENT_DATE_NOT_CLAIMED\",\"NEG.DATE.ARITHMETIC_NOT_CLAIMED\",\"NEG.DATE.CURRENTNESS_NOT_CLAIMED\"]}}\n"
        ),
        manifest_json, find_json,
    );
    Ok(DateManifest {
        manifest_json,
        casefile_json,
        statuses,
        findings,
    })
}
