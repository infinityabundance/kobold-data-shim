//! KOBOLD.DB2HOST.1 — declared Db2 host-variable null/truncation indicator manifest.
//!
//! **Doctrine.** A field can decode perfectly and still be **semantically NULL** at the database
//! boundary. KOBOLD.DB2HOST.1 admits only *declared* indicator evidence: a decoded value is marked
//! null or truncation-evidence **only** when a declared indicator field is paired with it. The decoded
//! bytes are always preserved. It does **not** claim SQL execution, Db2 precompiler behavior, SQLCA
//! interpretation, DBRM/package identity, or database truth — those remain non-claims.
//!
//! Db2 host-variable convention (admitted as the indicator semantics, not executed): the indicator is a
//! `PIC S9(4) COMP-5`; **negative → null**, **zero → present**, **positive → truncation evidence**.

use crate::COB_TYPE_NUMERIC_BINARY;
use crate::{decode_record_encoded, parse_program, CopyResolver, Encoding, ShimError};

/// A declared value/indicator pairing.
pub struct IndicatorPair<'a> {
    pub value_field: &'a str,
    pub indicator_field: &'a str,
}

/// The declared indicator manifest. A field with no declared pair gets **no** null-state claim
/// (`missing_indicator_policy = no_null_claim`); a declared indicator that is absent or the wrong usage
/// **fails closed** (`unknown_indicator_policy = fail_closed`).
pub struct IndicatorManifest<'a> {
    pub pairs: &'a [IndicatorPair<'a>],
}

/// The declared null/truncation state for one value/indicator pair (decoded bytes always preserved).
pub struct NullState {
    pub value_field: String,
    pub indicator_field: String,
    pub indicator_raw: Option<i64>,
    pub semantic_null: bool,
    pub truncation_evidence: bool,
    pub present: bool,
    pub decoded_bytes_preserved: bool,
}

/// The result of applying a Db2 host-variable indicator manifest to one record.
pub struct Db2HostResult {
    pub states: Vec<NullState>,
    pub findings: Vec<(String, String)>,
    pub audit_json: String,
    pub casefile_json: String,
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

/// Apply a declared Db2 host-variable indicator manifest to one record. Decoded bytes are preserved;
/// null/truncation state is assigned only through the declared indicator. Fails closed on a missing or
/// wrong-usage indicator.
pub fn db2host_evaluate(
    copybook: &str,
    record: &[u8],
    manifest: &IndicatorManifest,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<Db2HostResult, ShimError> {
    let prog = parse_program(copybook, resolver)?;
    let rec = decode_record_encoded(copybook, record, resolver, encoding)?;
    let value_of = |name: &str| {
        rec.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.value.clone())
    };
    let raw_of = |name: &str| {
        rec.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.raw_hex.clone())
    };

    let mut states: Vec<NullState> = Vec::new();
    let mut findings: Vec<(String, String)> = Vec::new();

    for pair in manifest.pairs {
        // The value field must exist (it carries the preserved bytes).
        if value_of(pair.value_field).is_none() {
            findings.push((
                "KOBOLD-DB2-MISSING-VALUE-FIELD".into(),
                format!(
                    "declared value field {:?} not in copybook (fail closed)",
                    pair.value_field
                ),
            ));
            continue;
        }
        // A declared indicator that is absent fails closed (never a silent "present").
        let Some(ind_val) = value_of(pair.indicator_field) else {
            findings.push((
                "KOBOLD-DB2-MISSING-INDICATOR".into(),
                format!(
                    "declared indicator {:?} not in copybook (fail closed)",
                    pair.indicator_field
                ),
            ));
            continue;
        };
        // The indicator must be a binary host variable (S9(4) COMP-5) -- wrong usage fails closed.
        let ind_type = prog
            .attrs
            .get(pair.indicator_field)
            .map(|(a, _)| a.field_type);
        if ind_type != Some(COB_TYPE_NUMERIC_BINARY) {
            findings.push((
                "KOBOLD-DB2-WRONG-INDICATOR-USAGE".into(),
                format!(
                    "indicator {:?} is not a binary (S9(4) COMP-5) host variable (fail closed)",
                    pair.indicator_field
                ),
            ));
            continue;
        }
        let raw: Option<i64> = ind_val.trim().parse().ok();
        let (mut semantic_null, mut truncation_evidence, mut present) = (false, false, false);
        match raw {
            Some(n) if n < 0 => semantic_null = true,
            Some(0) => present = true,
            Some(_) => truncation_evidence = true,
            None => findings.push((
                "KOBOLD-DB2-DIRTY-INDICATOR".into(),
                format!(
                    "indicator {:?} not a clean integer (fail closed)",
                    pair.indicator_field
                ),
            )),
        }
        states.push(NullState {
            value_field: pair.value_field.to_string(),
            indicator_field: pair.indicator_field.to_string(),
            indicator_raw: raw,
            semantic_null,
            truncation_evidence,
            present,
            decoded_bytes_preserved: true,
        });
    }

    // db2_host audit block
    let blocks = states
        .iter()
        .map(|s| {
            format!(
                "{}:{{\"value_field\":{},\"indicator_field\":{},\"indicator_raw_value\":{},\"semantic_null\":{},\"truncation_evidence\":{},\"present\":{},\"decoded_bytes_preserved\":{},\"raw_hex\":{},\"claim\":\"KOBOLD.DB2HOST.1\"}}",
                jstr(&s.value_field),
                jstr(&s.value_field),
                jstr(&s.indicator_field),
                s.indicator_raw.map(|n| n.to_string()).unwrap_or("null".into()),
                s.semantic_null,
                s.truncation_evidence,
                s.present,
                s.decoded_bytes_preserved,
                jstr(&raw_of(&s.value_field).unwrap_or_default()),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let audit_json = format!("{{\"db2_host\":{{{blocks}}}}}");

    let find_json = findings
        .iter()
        .map(|(r, m)| {
            format!(
                "{{\"ruleId\":{},\"level\":\"warning\",\"message\":{}}}",
                jstr(r),
                jstr(m)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-db2host-forensic-casefile-v1\",\"court\":\"KOBOLD.DB2HOST.1\",",
            "\"byte_truth\":{{\"proven\":true}},\"record_truth\":{{\"proven\":true}},",
            "\"database_truth\":{{\"claimed\":false,\"semantic_null_via\":\"declared indicator manifest only\"}},",
            "\"db2_host\":{{{}}},",
            "\"negative_capabilities\":[\"NEG.DB2.NULL.INDICATOR\",\"NEG.DB2.HOST_VALUE_NOT_DATABASE_VALUE\",",
            "\"NEG.DB2.SQLCA_NOT_INTERPRETED\",\"NEG.SQL.PRECOMPILER\",\"NEG.DB2.PACKAGE_NOT_CLAIMED\",",
            "\"NEG.DB2.DATABASE_TRUTH_NOT_CLAIMED\"],\"findings\":[{}]}}\n"
        ),
        blocks, find_json
    );

    Ok(Db2HostResult {
        states,
        findings,
        audit_json,
        casefile_json,
    })
}
