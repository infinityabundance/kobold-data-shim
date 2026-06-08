//! KOBOLD.SENTINEL.PROFILE.1 — declared sentinel-marker evidence.
//!
//! **Doctrine.** SENTINEL.PROFILE.1 admits only **declared** sentinel-marker evidence: LOW-VALUES,
//! HIGH-VALUES, SPACES, ZEROES, EBCDIC blanks, zero-dates, max-dates, and custom markers may be recorded
//! for named fields, but **nullness, date semantics, missingness, business status, account state, and
//! customer meaning remain non-claims** unless admitted by a separate declared profile (e.g. DB2HOST.1 for
//! null, DATE.PROFILE for dates). An undeclared sentinel-looking value is **never inferred**.

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// How a declared sentinel is matched against a decoded field.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum SentinelMatch<'a> {
    /// Match the field's decoded value string exactly.
    DecodedValue(&'a str),
    /// Match the field's raw bytes (hex, case-insensitive).
    RawHex(&'a str),
}

/// One declared sentinel rule for a field: an id + how to match it. Always evidence-only.
pub struct SentinelRule<'a> {
    pub sentinel_id: &'a str,
    pub match_on: SentinelMatch<'a>,
}

/// The declared sentinel profile: `(field_name, rules)` pairs. Nothing is inferred — only declared rules.
pub struct SentinelProfile<'a> {
    pub fields: &'a [(&'a str, &'a [SentinelRule<'a>])],
}

/// The sentinel-scan result.
pub struct SentinelManifest {
    pub manifest_json: String,
    pub casefile_json: String,
    /// `(field, sentinel_id)` for every declared marker that matched.
    pub hits: Vec<(String, String)>,
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

/// Record which DECLARED sentinel markers match — as evidence only, never inferring business meaning.
pub fn sentinel_scan(
    copybook: &str,
    record: &[u8],
    profile: &SentinelProfile,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<SentinelManifest, ShimError> {
    let decoded = decode_record_encoded(copybook, record, resolver, encoding)?;
    let fmap: std::collections::HashMap<&str, (&str, &str)> = decoded
        .fields
        .iter()
        .map(|f| (f.name.as_str(), (f.value.as_str(), f.raw_hex.as_str())))
        .collect();

    let mut hits: Vec<(String, String)> = Vec::new();
    let mut findings: Vec<(String, String)> = Vec::new();
    let mut hits_json: Vec<String> = Vec::new();

    for (field, rules) in profile.fields {
        let Some((value, raw_hex)) = fmap.get(field) else {
            findings.push((
                "KOBOLD-SENTINEL-NO-FIELD".into(),
                format!("declared sentinel field {field:?} not found in the record"),
            ));
            continue;
        };
        for rule in *rules {
            let (matched, matched_on) = match rule.match_on {
                SentinelMatch::DecodedValue(v) => (*value == v, "decoded_value"),
                SentinelMatch::RawHex(h) => (raw_hex.eq_ignore_ascii_case(h), "raw_hex"),
            };
            if matched {
                hits.push((field.to_string(), rule.sentinel_id.to_string()));
                hits_json.push(format!(
                    "{{\"field\":{},\"sentinel_id\":{},\"matched_on\":{},\"decoded_value\":{},\"raw_hex\":{},\"meaning_label\":\"declared_marker_only\",\"business_meaning_claimed\":false}}",
                    jstr(field), jstr(rule.sentinel_id), jstr(matched_on), jstr(value), jstr(raw_hex),
                ));
            }
        }
    }

    let manifest_json = format!(
        concat!(
            "{{\"schema\":\"kobold-sentinel-manifest-v1\",\"court\":\"KOBOLD.SENTINEL.PROFILE.1\",",
            "\"record_sha256\":{},\"hit_count\":{},\"hits\":[{}],",
            "\"undeclared_inference\":false}}"
        ),
        jstr(&sha256_hex(record)),
        hits.len(),
        hits_json.join(","),
    );
    let find_json = findings
        .iter()
        .map(|(r, m)| format!("{{\"ruleId\":{},\"message\":{}}}", jstr(r), jstr(m)))
        .collect::<Vec<_>>()
        .join(",");
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-sentinel-forensic-casefile-v1\",\"court\":\"KOBOLD.SENTINEL.PROFILE.1\",",
            "\"manifest\":{},\"findings\":[{}],",
            "\"truth_layers\":{{\"declared_marker_evidence\":true,\"nullness\":false,\"date_meaning\":false,",
            "\"missingness\":false,\"business_status\":false,\"account_state\":false,\"default_meaning\":false}},",
            "\"negative_capabilities\":[\"NEG.SENTINEL.LOW_VALUES_NOT_NULL\",\"NEG.SENTINEL.HIGH_VALUES_NOT_MAX_DATE\",",
            "\"NEG.SENTINEL.SPACES_NOT_MISSING\",\"NEG.SENTINEL.ZEROES_NOT_ABSENT\",\"NEG.SENTINEL.ZERO_DATE_NOT_DATE\",",
            "\"NEG.SENTINEL.MARKER_NOT_BUSINESS_STATUS\",\"NEG.SENTINEL.UNDECLARED_NOT_INFERRED\"]}}\n"
        ),
        manifest_json, find_json,
    );

    Ok(SentinelManifest {
        manifest_json,
        casefile_json,
        hits,
        findings,
    })
}
