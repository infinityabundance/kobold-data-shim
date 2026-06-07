//! End-to-end fixed-record reconciliation (`KOBOLD.RECON.1`).
//!
//! `reconcile` decodes a buffer of fixed-length COBOL records against a copybook into stable JSONL
//! (fields + LEVEL-88 conditions + per-record audit) plus a batch `audit.json` and an explicit
//! `unsupported.json`. Everything is produced from the **sealed `gnucobol-rs` courts only**; any
//! COBOL surface outside them is emitted as explicit non-decoded evidence, never guessed.
//!
//! **Doctrine.** KOBOLD.RECON.1 admits only end-to-end fixed-record reconciliation over sealed
//! `gnucobol-rs` courts: copybook expansion, layout, decoded field bytes, LEVEL-88 predicates, and
//! audit hashes are proven together, while unsupported COBOL surfaces are emitted as explicit
//! non-decoded evidence rather than guessed.

use crate::sha256::sha256_hex;
use crate::{CopyResolver, ShimError};

/// Crate version, for the audit receipt.
pub const SHIM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The reconciliation outputs (all byte-stable for a given input).
pub struct ReconResult {
    /// One JSON object per record, newline-separated (JSONL).
    pub jsonl: String,
    /// The batch audit receipt (`kobold-recon-receipt-v1`).
    pub audit_json: String,
    /// The explicit list of unsupported fields/conditions (non-decoded evidence).
    pub unsupported_json: String,
    pub record_count: usize,
    pub unsupported_count: usize,
}

fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Reconcile a fixed-record buffer. `fixture` names the dataset in the audit; `gnucobol_rs_version`
/// records the pinned kernel version. A trailing partial record is reported as unsupported.
pub fn reconcile(
    fixture: &str,
    copybook: &str,
    data: &[u8],
    record_len: usize,
    gnucobol_rs_version: &str,
    resolver: &impl CopyResolver,
) -> Result<ReconResult, ShimError> {
    reconcile_encoded(
        fixture,
        copybook,
        data,
        record_len,
        gnucobol_rs_version,
        resolver,
        crate::Encoding::Ascii,
    )
}

/// Reconcile under an explicit [`crate::Encoding`] (`KOBOLD.DATA.3`). Under `Cp500`, alphanumeric
/// DISPLAY fields are decoded via the sealed `GNURUST.15` table; binary/packed fields pass through as
/// raw storage. The encoding (and its non-claims) are recorded in the audit; it is never auto-detected.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_encoded(
    fixture: &str,
    copybook: &str,
    data: &[u8],
    record_len: usize,
    gnucobol_rs_version: &str,
    resolver: &impl CopyResolver,
    encoding: crate::Encoding,
) -> Result<ReconResult, ShimError> {
    if record_len == 0 {
        return Err(ShimError::Layout("record_len must be > 0".into()));
    }
    // Refuse silent JSON key collisions before producing any output (KOBOLD.OPERATOR.1).
    crate::operator::check_copybook_collisions(copybook, resolver)?;
    let enc_note = match encoding {
        crate::Encoding::Cp500 => ",\"encoding\":{\"record_default\":\"cp500\",\"source\":\"gnucobol-3.2:ebcdic500_ascii8bit.ttbl\",\"auto_detected\":false,\"binary_fields_passthrough\":true,\"packed_fields_passthrough\":true,\"mixed_encoding_claim\":false}",
        crate::Encoding::Ascii => "",
    };

    let expanded =
        gnucobol_rs::expand(copybook, resolver).map_err(|e| ShimError::Copy(e.to_string()))?;
    let expanded_text = expanded.text();
    // Byte-domain note: flag binary usages so the audit records the endian assumption explicitly.
    let up = expanded_text.to_ascii_uppercase();
    let has_binary = up.contains("COMP-5")
        || up.contains("COMP-X")
        || up.contains(" COMP.")
        || up.contains(" COMP\n")
        || up.contains("BINARY")
        || up.contains("COMPUTATIONAL");
    let binary_note = if has_binary {
        ",\"binary_byteorder\":\"big-endian (COMP/COMP-X), native-little-endian (COMP-5); admitted GnuCOBOL 3.2 binary-size 1-2-4-8\""
    } else {
        ""
    };

    let mut jsonl = String::new();
    let mut unsupported_items: Vec<String> = Vec::new();
    let mut record_count = 0usize;
    let mut field_count = 0usize;
    let mut condition_count = 0usize;
    let mut unsupported_count = 0usize;
    let mut layout_sig = String::new();

    for (index, chunk) in data.chunks(record_len).enumerate() {
        record_count += 1;
        let rec = crate::decode_record_encoded(copybook, chunk, resolver, encoding)?;
        if index == 0 {
            // The layout is identical for every record; sign it once for the audit.
            for f in &rec.fields {
                layout_sig.push_str(&format!("{}:{}:{};", f.name, f.offset, f.size));
            }
            field_count = rec.fields.iter().filter(|f| f.category != "group").count();
            condition_count = rec.conditions.len();
        }

        let mut obj = format!("{{\"record_index\":{index},\"fields\":{{");
        let mut first = true;
        for f in &rec.fields {
            match f.category {
                "numeric" | "alphanumeric" => {
                    if !first {
                        obj.push(',');
                    }
                    first = false;
                    obj.push_str(&format!("{}:{}", jstr(&f.name), jstr(&f.value)));
                }
                "unsupported" => {
                    if index == 0 {
                        unsupported_items.push(format!("field:{}", f.name));
                    }
                    unsupported_count += 1;
                }
                _ => {} // group: not a value field
            }
        }
        obj.push_str("},\"conditions\":{");
        let mut cfirst = true;
        for c in &rec.conditions {
            match c.value {
                Some(b) => {
                    if !cfirst {
                        obj.push(',');
                    }
                    cfirst = false;
                    obj.push_str(&format!("{}:{}", jstr(&c.name), b));
                }
                None => {
                    if index == 0 {
                        unsupported_items.push(format!("condition:{}", c.name));
                    }
                    unsupported_count += 1;
                }
            }
        }
        let rec_sha = sha256_hex(chunk);
        obj.push_str(&format!(
            "}},\"audit\":{{\"raw_offset\":{},\"raw_len\":{},\"record_sha256\":{}}}}}",
            index * record_len,
            chunk.len(),
            jstr(&rec_sha)
        ));
        jsonl.push_str(&obj);
        jsonl.push('\n');
    }

    let mut unsupported_json = String::from("{\"unsupported\":[");
    for (i, u) in unsupported_items.iter().enumerate() {
        if i > 0 {
            unsupported_json.push(',');
        }
        unsupported_json.push_str(&jstr(u));
    }
    unsupported_json.push_str(&format!("],\"unsupported_count\":{unsupported_count}}}\n"));

    let decode_output_sha256 = sha256_hex(jsonl.as_bytes());
    let audit_json = format!(
        concat!(
            "{{\"schema\":\"kobold-recon-receipt-v1\",",
            "\"fixture\":{},",
            "\"raw_input_sha256\":{},",
            "\"copybook_sha256\":{},",
            "\"expanded_copybook_sha256\":{},",
            "\"layout_hash\":{},",
            "\"record_count\":{},",
            "\"field_count\":{},",
            "\"condition_count\":{},",
            "\"unsupported_count\":{},",
            "\"gnucobol_rs_version\":{},",
            "\"kobold_data_shim_version\":{},",
            "\"decode_output_sha256\":{}{}{},",
            "\"stale_copybook_risk\":{},",
            "\"byte_stable_replay\":true}}\n"
        ),
        jstr(fixture),
        jstr(&sha256_hex(data)),
        jstr(&sha256_hex(copybook.as_bytes())),
        jstr(&sha256_hex(expanded_text.as_bytes())),
        jstr(&sha256_hex(layout_sig.as_bytes())),
        record_count,
        field_count,
        condition_count,
        unsupported_count,
        jstr(gnucobol_rs_version),
        jstr(SHIM_VERSION),
        jstr(&decode_output_sha256),
        binary_note,
        enc_note,
        jstr(crate::operator::STALE_COPYBOOK_RISK),
    );

    Ok(ReconResult {
        jsonl,
        audit_json,
        unsupported_json,
        record_count,
        unsupported_count,
    })
}
