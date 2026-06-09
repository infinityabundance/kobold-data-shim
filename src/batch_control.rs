//! KOBOLD.BATCH.CONTROL.1 — header/trailer control-total validation.
//!
//! **Doctrine.** Batch flat files carry self-checking control fields: a trailer that declares the **record
//! count** and a **control total** (the sum of an amount field over the detail records). BATCH.CONTROL.1
//! decodes the declared count/total fields and compares them to the **observed** detail count and the
//! **observed** sum of a declared amount field — reporting match/mismatch as **evidence**. A match means the
//! declared and observed control fields agree; it does **not** mean the batch is complete, correct, settled,
//! or business-valid. A mismatch is **evidence of a discrepancy, not its cause**. *A batch-control validation
//! proves declared-vs-observed count/total agreement, not batch correctness or settlement finality.*

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// The batch-control validation result.
pub struct BatchControl {
    pub manifest_json: String,
    pub casefile_json: String,
    pub count_match: bool,
    pub total_match: bool,
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

/// Parse a canonical decimal string (e.g. `"175.75"`, `"-5"`, `"100"`) into `(negative, scaled, scale)` where
/// `value = (-1)^negative * scaled * 10^(-scale)`. Returns `None` if it is not a plain decimal.
fn parse_dec(s: &str) -> Option<(bool, i128, u32)> {
    let t = s.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    if ip.is_empty() && fp.is_empty() {
        return None;
    }
    if !ip.bytes().all(|b| b.is_ascii_digit()) || !fp.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let scaled: i128 = format!("{ip}{fp}").parse().ok()?;
    Some((neg, scaled, fp.len() as u32))
}

fn pow10(n: u32) -> i128 {
    (0..n).fold(1i128, |a, _| a * 10)
}

/// A signed decimal value at a fixed scale, for summing/comparison.
fn signed_at_scale(neg: bool, scaled: i128, scale: u32, target: u32) -> i128 {
    let mag = if scale <= target {
        scaled * pow10(target - scale)
    } else {
        scaled / pow10(scale - target)
    };
    if neg {
        -mag
    } else {
        mag
    }
}

/// Render a signed scaled integer at `scale` back to a canonical decimal string.
fn fmt_dec(signed: i128, scale: u32) -> String {
    let sign = if signed < 0 { "-" } else { "" };
    let mag = signed.unsigned_abs();
    if scale == 0 {
        return format!("{sign}{mag}");
    }
    let d = 10u128.pow(scale);
    format!(
        "{sign}{}.{:0>width$}",
        mag / d,
        mag % d,
        width = scale as usize
    )
}

/// Validate a batch's trailer control fields against the observed detail records. `detail_records` are the
/// already-routed detail records (e.g. from `KOBOLD.VARIANT.1`); `amount_field` is summed over them and
/// compared to the trailer's `total_field`, and their count to the trailer's `count_field`.
#[allow(clippy::too_many_arguments)]
pub fn batch_control(
    detail_records: &[&[u8]],
    detail_copybook: &str,
    amount_field: &str,
    trailer_record: &[u8],
    trailer_copybook: &str,
    count_field: &str,
    total_field: &str,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<BatchControl, ShimError> {
    // observed: detail count + sum of the amount field
    let observed_count = detail_records.len();
    let mut amounts: Vec<(bool, i128, u32)> = Vec::new();
    for rec in detail_records {
        let decoded = decode_record_encoded(detail_copybook, rec, resolver, encoding)?;
        if let Some(f) = decoded.fields.iter().find(|f| f.name == amount_field) {
            if let Some(d) = parse_dec(&f.value) {
                amounts.push(d);
            }
        }
    }
    let sum_scale = amounts.iter().map(|(_, _, s)| *s).max().unwrap_or(0);
    let observed_sum: i128 = amounts
        .iter()
        .map(|&(n, sc, s)| signed_at_scale(n, sc, s, sum_scale))
        .sum();
    let observed_total = fmt_dec(observed_sum, sum_scale);

    // declared: trailer count + total fields
    let trailer = decode_record_encoded(trailer_copybook, trailer_record, resolver, encoding)?;
    let declared_count_raw = trailer
        .fields
        .iter()
        .find(|f| f.name == count_field)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let declared_total_raw = trailer
        .fields
        .iter()
        .find(|f| f.name == total_field)
        .map(|f| f.value.clone())
        .unwrap_or_default();

    let count_match = parse_dec(&declared_count_raw)
        .map(|(_, v, _)| v == observed_count as i128)
        .unwrap_or(false);
    let total_match = match (parse_dec(&declared_total_raw), parse_dec(&observed_total)) {
        (Some((dn, ds, dsc)), Some((on, os, osc))) => {
            let t = dsc.max(osc);
            signed_at_scale(dn, ds, dsc, t) == signed_at_scale(on, os, osc, t)
        }
        _ => false,
    };

    let manifest_json = format!(
        concat!(
            "{{\"schema\":\"kobold-batch-control-manifest-v1\",\"court\":\"KOBOLD.BATCH.CONTROL.1\",",
            "\"trailer_sha256\":{},\"count\":{{\"declared\":{},\"observed\":{},\"match\":{}}},",
            "\"total\":{{\"declared\":{},\"observed\":{},\"amount_field\":{},\"match\":{}}}}}"
        ),
        jstr(&sha256_hex(trailer_record)),
        jstr(&declared_count_raw),
        observed_count,
        count_match,
        jstr(&declared_total_raw),
        jstr(&observed_total),
        jstr(amount_field),
        total_match,
    );
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-batch-control-forensic-casefile-v1\",\"court\":\"KOBOLD.BATCH.CONTROL.1\",",
            "\"manifest\":{},\"truth_layers\":{{\"control_field_truth\":true,\"reconciliation_truth\":{},",
            "\"batch_correctness\":false,\"settlement_truth\":false,\"business_meaning\":false}},",
            "\"negative_capabilities\":[\"NEG.BATCH_CONTROL.MATCH_NOT_CORRECTNESS\",",
            "\"NEG.BATCH_CONTROL.MISMATCH_NOT_CAUSE\",\"NEG.BATCH_CONTROL.NO_SETTLEMENT\",",
            "\"NEG.BATCH_CONTROL.REQUIRES_DECLARED_FIELDS\",\"NEG.BATCH_CONTROL.NO_RESTART_CHECKPOINT\",",
            "\"NEG.BATCH_CONTROL.WRITE_BACK_NOT_CLAIMED\"]}}\n"
        ),
        manifest_json,
        count_match && total_match,
    );

    Ok(BatchControl {
        manifest_json,
        casefile_json,
        count_match,
        total_match,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoCopy;

    const DETAIL: &str = "01 D-REC.\n  05 D-TYPE PIC X.\n  05 D-AMT PIC 9(4)V99.";
    const TRAILER: &str =
        "01 T-REC.\n  05 T-TYPE PIC X.\n  05 T-COUNT PIC 9(3).\n  05 T-TOTAL PIC 9(6)V99.";

    // detail amounts: 100.00 + 50.50 + 25.25 = 175.75
    fn details() -> Vec<&'static [u8]> {
        vec![b"D010000", b"D005050", b"D002525"]
    }

    #[test]
    fn matching_count_and_total() {
        // trailer: count 3, total 000175.75
        let trailer = b"T00300017575";
        let r = batch_control(
            &details(),
            DETAIL,
            "D-AMT",
            trailer,
            TRAILER,
            "T-COUNT",
            "T-TOTAL",
            &NoCopy,
            Encoding::Ascii,
        )
        .unwrap();
        assert!(r.count_match, "count should match: {}", r.manifest_json);
        assert!(r.total_match, "total should match: {}", r.manifest_json);
    }

    #[test]
    fn mismatched_count_and_total_are_evidence() {
        // trailer declares count 2 (!=3) and total 000175.74 (!=175.75)
        let trailer = b"T00200017574";
        let r = batch_control(
            &details(),
            DETAIL,
            "D-AMT",
            trailer,
            TRAILER,
            "T-COUNT",
            "T-TOTAL",
            &NoCopy,
            Encoding::Ascii,
        )
        .unwrap();
        assert!(!r.count_match);
        assert!(!r.total_match);
        // reconciliation_truth is false when either side disagrees
        assert!(r.casefile_json.contains("\"reconciliation_truth\":false"));
    }
}
