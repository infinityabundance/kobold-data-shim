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
    reconcile_impl(
        fixture,
        copybook,
        data,
        record_len,
        gnucobol_rs_version,
        resolver,
        encoding,
        false,
    )
}

/// `reconcile_encoded` with optional **record-level Rayon** parallelism (`KOBOLD.PERF.1`, `rayon`
/// feature). Output is **byte-identical** to [`reconcile_encoded`] — same JSONL, audit, unsupported
/// ledger, and hashes (order-preserving). Performance is a derived property of preserved evidence.
#[cfg(feature = "rayon")]
#[allow(clippy::too_many_arguments)]
pub fn reconcile_encoded_parallel(
    fixture: &str,
    copybook: &str,
    data: &[u8],
    record_len: usize,
    gnucobol_rs_version: &str,
    resolver: &(impl CopyResolver + Sync),
    encoding: crate::Encoding,
) -> Result<ReconResult, ShimError> {
    reconcile_impl(
        fixture,
        copybook,
        data,
        record_len,
        gnucobol_rs_version,
        resolver,
        encoding,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn reconcile_impl(
    fixture: &str,
    copybook: &str,
    data: &[u8],
    record_len: usize,
    gnucobol_rs_version: &str,
    resolver: &impl CopyResolver,
    encoding: crate::Encoding,
    parallel: bool,
) -> Result<ReconResult, ShimError> {
    if record_len == 0 {
        return Err(ShimError::Layout("record_len must be > 0".into()));
    }
    // Refuse silent JSON key collisions before producing any output (KOBOLD.OPERATOR.1).
    crate::operator::check_copybook_collisions(copybook, resolver)?;
    let enc_note = match encoding {
        crate::Encoding::Cp500 => ",\"encoding\":{\"record_default\":\"cp500\",\"source\":\"gnucobol-3.2:ebcdic500_ascii8bit.ttbl\",\"auto_detected\":false,\"binary_fields_passthrough\":true,\"packed_fields_passthrough\":true,\"mixed_encoding_claim\":false,\"numeric_display\":{\"zoned_sign\":\"GNURUST.17\",\"code_page\":\"cp500\"}}",
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
    // COMP-6 (KOBOLD.DATA.6 / GNURUST.18): unsigned packed-decimal storage. Signed COMP-6 is not
    // admitted (GnuCOBOL converts it to COMP-3) and is surfaced as unsupported, never decoded here.
    let comp6_note = if up.contains("COMP-6") || up.contains("COMPUTATIONAL-6") {
        ",\"comp6\":{\"claim\":\"GNURUST.18\",\"domain\":\"comp6-unsigned-packed\",\"signed_comp6\":\"not-admitted-fail-closed\"}"
    } else {
        ""
    };

    // Parse the program ONCE (the COPY resolver is used only here, sequentially); per-record decode then
    // reuses the shared layout -- faster than re-parsing per record, and the parallel path captures only
    // the Sync `Program` (no resolver), so record-level Rayon needs no extra bound on the public API.
    let prog = crate::parse_program(copybook, resolver)?;
    let chunks: Vec<&[u8]> = data.chunks(record_len).collect();
    let record_count = chunks.len();
    // The layout is identical for every record; sign it once (from the first record) for the audit.
    let mut layout_sig = String::new();
    let mut field_count = 0usize;
    let mut condition_count = 0usize;
    if let Some(first) = chunks.first() {
        let fields0 = crate::decode_fields(&prog, first, encoding);
        for f in &fields0 {
            layout_sig.push_str(&format!("{}:{}:{};", f.name, f.offset, f.size));
        }
        field_count = fields0.iter().filter(|f| f.category != "group").count();
        condition_count = crate::eval_conditions(&prog, first, encoding).len();
    }

    // Per-record JSON objects, in record ORDER. Scalar by default; record-level Rayon under the `rayon`
    // feature (`KOBOLD.PERF.1`) — order-preserving `collect`, so the assembled JSONL/audit is byte-identical.
    let objects = build_objects(&prog, &chunks, record_len, encoding, parallel);
    let mut jsonl = String::new();
    for (obj, _, _) in &objects {
        jsonl.push_str(obj);
        jsonl.push('\n');
    }
    let unsupported_count: usize = objects.iter().map(|(_, c, _)| *c).sum();
    let unsupported_items: Vec<String> = objects
        .first()
        .map(|(_, _, n)| n.clone())
        .unwrap_or_default();

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
            "\"decode_output_sha256\":{}{}{}{},",
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
        comp6_note,
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

/// Build one record's JSON object + its `(unsupported_count, unsupported_names)` — the per-record work,
/// shared by the scalar and Rayon paths so both emit byte-identical evidence.
fn record_object(
    fields: &[crate::DecodedField],
    conditions: &[crate::DecodedCondition],
    index: usize,
    record_len: usize,
    chunk: &[u8],
) -> (String, usize, Vec<String>) {
    let mut obj = format!("{{\"record_index\":{index},\"fields\":{{");
    let mut first = true;
    let mut edited_block = String::new();
    let mut efirst = true;
    let mut unsupported_count = 0usize;
    let mut unsupported_names: Vec<String> = Vec::new();
    for f in fields {
        match f.category {
            "numeric" | "alphanumeric" | "edited" => {
                if !first {
                    obj.push(',');
                }
                first = false;
                obj.push_str(&format!("{}:{}", jstr(&f.name), jstr(&f.value)));
                if f.category == "edited" {
                    if !efirst {
                        edited_block.push(',');
                    }
                    efirst = false;
                    let num = f
                        .edited_numeric
                        .as_deref()
                        .map(jstr)
                        .unwrap_or_else(|| "null".to_string());
                    edited_block.push_str(&format!(
                        "{}:{{\"raw_text\":{},\"numeric_value\":{},\"claim\":\"GNURUST.16\",\"domain\":\"edited-display-decode\"}}",
                        jstr(&f.name),
                        jstr(&f.value),
                        num
                    ));
                }
            }
            "unsupported" => {
                unsupported_names.push(format!("field:{}", f.name));
                unsupported_count += 1;
            }
            _ => {}
        }
    }
    obj.push_str("},\"conditions\":{");
    let mut cfirst = true;
    for c in conditions {
        match c.value {
            Some(b) => {
                if !cfirst {
                    obj.push(',');
                }
                cfirst = false;
                obj.push_str(&format!("{}:{}", jstr(&c.name), b));
            }
            None => {
                unsupported_names.push(format!("condition:{}", c.name));
                unsupported_count += 1;
            }
        }
    }
    let rec_sha = sha256_hex(chunk);
    let edited_audit = if edited_block.is_empty() {
        String::new()
    } else {
        format!(",\"edited\":{{{edited_block}}}")
    };
    obj.push_str(&format!(
        "}},\"audit\":{{\"raw_offset\":{},\"raw_len\":{},\"record_sha256\":{}{}}}}}",
        index * record_len,
        chunk.len(),
        jstr(&rec_sha),
        edited_audit
    ));
    (obj, unsupported_count, unsupported_names)
}

/// Decode + build all record objects in record ORDER. Scalar by default; record-level Rayon under the
/// `rayon` feature (`KOBOLD.PERF.1`). The parallel path captures only the `Sync` `Program` and uses an
/// order-preserving `collect`, so the result is byte-identical to scalar.
fn build_objects(
    prog: &crate::Program,
    chunks: &[&[u8]],
    record_len: usize,
    encoding: crate::Encoding,
    parallel: bool,
) -> Vec<(String, usize, Vec<String>)> {
    let one = |index: usize, chunk: &[u8]| {
        let fields = crate::decode_fields(prog, chunk, encoding);
        let conditions = crate::eval_conditions(prog, chunk, encoding);
        record_object(&fields, &conditions, index, record_len, chunk)
    };
    if parallel {
        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            return chunks
                .par_iter()
                .enumerate()
                .map(|(i, c)| one(i, c))
                .collect();
        }
    }
    chunks.iter().enumerate().map(|(i, c)| one(i, c)).collect()
}
