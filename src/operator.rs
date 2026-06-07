//! Operator trust layer (`KOBOLD.OPERATOR.1`): make every decoded field **accountable**.
//!
//! **Doctrine.** KOBOLD.OPERATOR.1 makes every decoded field accountable: each value must be
//! explainable from source provenance, raw bytes, sealed courts, and receipt hashes, while dirty or
//! unsupported data remains preserved evidence rather than coerced output.
//!
//! Three operator views, all built from the same sealed courts:
//! - [`explain_field`] — "why do you say this field means that?" (provenance, bytes, courts, value).
//! - [`control_totals`] — record count, per-field numeric sums, condition counts, dirty/unsupported.
//! - [`DirtyMode`] — `Evidence` preserves invalid bytes as marked evidence; `Strict` errors out.

use crate::sha256::sha256_hex;
use crate::{
    decode_fields, eval_conditions, parse_program, CopyResolver, Encoding, ShimError,
    COB_FLAG_NO_SIGN_NIBBLE, COB_TYPE_NUMERIC_PACKED,
};

/// How to treat field bytes that are invalid for their declared type (a *dirty-data* policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DirtyMode {
    /// Preserve the raw bytes and mark the field invalid — never coerce (the migration-safe default).
    #[default]
    Evidence,
    /// Reject the record: any invalid field is a hard error.
    Strict,
}

/// The stale-copybook risk statement carried in operator receipts. Decoding proves what *this*
/// copybook says the bytes mean — not that production wrote them with this copybook.
pub const STALE_COPYBOOK_RISK: &str =
    "Decoding proves what the ADMITTED copybook says these bytes mean; it does not prove the copybook \
     is the one production used, nor that the data is current, complete, lawful, or business-valid. \
     Verify with source provenance and control totals across record families.";

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

/// Is a byte slice a *valid* value for its declared field type? (Dirty-data detection — never coerces.)
fn field_valid(field_type: u16, flags: u16, category: &str, bytes: &[u8]) -> bool {
    match category {
        "alphanumeric" => true, // any bytes are text
        "numeric" => match field_type {
            // COMP-6 (PACKED + NO_SIGN_NIBBLE) has no sign nibble — every nibble is a digit.
            t if t == COB_TYPE_NUMERIC_PACKED && flags & COB_FLAG_NO_SIGN_NIBBLE != 0 => {
                comp6_valid(bytes)
            }
            t if t == COB_TYPE_NUMERIC_PACKED => packed_valid(bytes),
            0x11 => true, // binary: any byte pattern is a valid two's-complement value
            _ => display_num_valid(bytes), // zoned/display
        },
        _ => false, // group / unsupported are not decoded values
    }
}

/// COMP-6 (unsigned packed, `GNURUST.18`): every nibble is a digit 0..9 — no sign nibble.
fn comp6_valid(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|&b| (b >> 4) <= 9 && (b & 0x0F) <= 9)
}

/// COMP-3: every nibble is a digit except the final (sign) nibble, which must be a sign code.
fn packed_valid(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let last = bytes.len() - 1;
    for (i, &b) in bytes.iter().enumerate() {
        let (hi, lo) = (b >> 4, b & 0x0F);
        if hi > 9 {
            return false;
        }
        if i == last {
            // low nibble of the last byte is the sign: C/D/F (and A/B/E accepted as alternates).
            if lo < 0x0A {
                return false;
            }
        } else if lo > 9 {
            return false;
        }
    }
    true
}

/// Zoned DISPLAY: every byte is an ASCII digit, except the sign position may carry an overpunch.
fn display_num_valid(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let last = bytes.len() - 1;
    for (i, &b) in bytes.iter().enumerate() {
        let digit = b.is_ascii_digit();
        let overpunch = matches!(b, 0x70..=0x79 | b'{' | b'}'); // ASCII negative/zero overpunch
        if i == last {
            if !(digit || overpunch) {
                return false;
            }
        } else if !digit {
            return false;
        }
    }
    true
}

/// The sealed courts that produced a field's value (for the `explain` evidence trail).
fn sealed_courts(
    field_type: u16,
    category: &str,
    used_copy: bool,
    has_conds: bool,
    encoding: Encoding,
) -> Vec<&'static str> {
    let mut c = vec!["PIC (GNURUST.3)", "LAYOUT (GNURUST.4)"];
    if used_copy {
        c.push("COPY/REPLACING (GNURUST.5/6)");
    }
    match (category, field_type) {
        ("numeric", t) if t == COB_TYPE_NUMERIC_PACKED => c.push("COMP-3 MOVE (GNURUST.2)"),
        ("numeric", 0x11) => c.push("binary storage (GNURUST.14)"),
        ("numeric", _) => c.push("display MOVE (GNURUST.2)"),
        ("alphanumeric", _) => {
            if encoding == Encoding::Cp500 {
                c.push("EBCDIC cp500 decode (GNURUST.15)");
            } else {
                c.push("text (ASCII)");
            }
        }
        _ => {}
    }
    if has_conds {
        c.push("LEVEL-88 (GNURUST.11)");
    }
    c
}

/// Explain one decoded field for one record: provenance, bytes, sealed courts, decoded value,
/// dependent conditions, record hash, and the explicit non-claims. Returns a JSON object.
pub fn explain_field(
    copybook: &str,
    record: &[u8],
    field: &str,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<String, ShimError> {
    let prog = parse_program(copybook, resolver)?;
    check_collisions(&prog)?;
    let laid = prog
        .laid
        .iter()
        .find(|l| l.name == field)
        .ok_or_else(|| ShimError::BadItem(format!("no field named {field}")))?;
    let fields = decode_fields(&prog, record, encoding);
    let df = fields
        .iter()
        .find(|f| f.name == field)
        .ok_or_else(|| ShimError::BadItem(format!("field {field} not decoded")))?;
    let conds = eval_conditions(&prog, record, encoding);
    let deps: Vec<_> = conds.iter().filter(|c| c.parent == field).collect();
    let meta = prog.meta.get(field);
    let (ft, fl, cat) = prog
        .attrs
        .get(field)
        .map(|(a, c)| (a.field_type, a.flags, *c))
        .unwrap_or((0, 0, "group"));
    let slice = record.get(laid.offset..laid.offset + laid.size);
    let valid = slice.map(|b| field_valid(ft, fl, cat, b)).unwrap_or(false);
    let courts = sealed_courts(ft, cat, prog.used_copy, !deps.is_empty(), encoding);

    let mut o = String::from("{");
    o.push_str(&format!("\"field\":{},", jstr(field)));
    if let Some(m) = meta {
        o.push_str(&format!(
            "\"provenance\":{{\"copybook\":{},\"line\":{}}},\"usage\":{},\"pic\":{},",
            jstr(&m.source_file),
            m.source_line,
            jstr(&m.usage),
            jstr(&m.pic)
        ));
    }
    o.push_str(&format!(
        "\"level\":{},\"offset\":{},\"size\":{},\"category\":{},",
        laid.level,
        laid.offset,
        laid.size,
        jstr(cat)
    ));
    o.push_str(&format!("\"raw_bytes\":{},", jstr(&df.raw_hex)));
    o.push_str(&format!("\"decoded_value\":{},", jstr(&df.value)));
    o.push_str(&format!("\"valid\":{valid},"));
    o.push_str("\"sealed_courts\":[");
    for (i, c) in courts.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str(&jstr(c));
    }
    o.push_str("],\"dependent_conditions\":{");
    for (i, c) in deps.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        match c.value {
            Some(b) => o.push_str(&format!("{}:{}", jstr(&c.name), b)),
            None => o.push_str(&format!("{}:null", jstr(&c.name))),
        }
    }
    let rec_sha = sha256_hex(record);
    o.push_str(&format!("}},\"record_sha256\":{},", jstr(&rec_sha)));
    o.push_str(&format!(
        "\"encoding\":{},\"non_claims\":[\"no business-truth claim\",\"no arithmetic transformation applied\",{}],",
        jstr(if encoding == Encoding::Cp500 { "cp500" } else { "ascii" }),
        jstr("decoded only through sealed gnucobol-rs courts")
    ));
    o.push_str(&format!(
        "\"stale_copybook_risk\":{}}}\n",
        jstr(STALE_COPYBOOK_RISK)
    ));
    Ok(o)
}

fn val_to_scaled(s: &str) -> Option<(i128, i32)> {
    let neg = s.starts_with('-');
    let t = s.trim_start_matches(['-', '+']);
    let (i, f) = t.split_once('.').unwrap_or((t, ""));
    if i.bytes().chain(f.bytes()).any(|b| !b.is_ascii_digit()) {
        return None;
    }
    let mut v: i128 = 0;
    for b in i.bytes().chain(f.bytes()) {
        v = v.checked_mul(10)?.checked_add((b - b'0') as i128)?;
    }
    Some((if neg { -v } else { v }, f.len() as i32))
}

/// Control totals over a fixed-record buffer: record count, per numeric-field sums, condition
/// true-counts, and dirty/unsupported counts. Returns a JSON object — the accounting an operator
/// reconciles before trusting the decode.
pub fn control_totals(
    copybook: &str,
    data: &[u8],
    record_len: usize,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<String, ShimError> {
    if record_len == 0 {
        return Err(ShimError::Layout("record_len must be > 0".into()));
    }
    let prog = parse_program(copybook, resolver)?;
    check_collisions(&prog)?;
    use std::collections::BTreeMap;
    let mut sums: BTreeMap<String, (i128, i32)> = BTreeMap::new();
    let mut cond_true: BTreeMap<String, u64> = BTreeMap::new();
    let mut record_count = 0u64;
    let mut invalid = 0u64;
    let mut unsupported = 0u64;
    for chunk in data.chunks(record_len) {
        record_count += 1;
        for f in decode_fields(&prog, chunk, encoding) {
            match f.category {
                "numeric" => {
                    let ft = prog
                        .attrs
                        .get(&f.name)
                        .map(|(a, _)| (a.field_type, a.flags))
                        .unwrap_or((0, 0));
                    let slice = chunk.get(f.offset..f.offset + f.size);
                    if !slice
                        .map(|b| field_valid(ft.0, ft.1, "numeric", b))
                        .unwrap_or(false)
                    {
                        invalid += 1;
                        continue;
                    }
                    if let Some((v, sc)) = val_to_scaled(&f.value) {
                        let e = sums.entry(f.name.clone()).or_insert((0, sc));
                        e.0 = e.0.saturating_add(v);
                        e.1 = e.1.max(sc);
                    }
                }
                "unsupported" => unsupported += 1,
                _ => {}
            }
        }
        for c in eval_conditions(&prog, chunk, encoding) {
            if c.value == Some(true) {
                *cond_true.entry(c.name.clone()).or_insert(0) += 1;
            }
        }
    }
    let fmt_sum = |v: i128, sc: i32| -> String {
        if sc <= 0 {
            return v.to_string();
        }
        let div = 10i128.pow(sc as u32);
        let neg = v < 0;
        let a = v.unsigned_abs();
        format!(
            "{}{}.{:0w$}",
            if neg { "-" } else { "" },
            a / div as u128,
            a % div as u128,
            w = sc as usize
        )
    };
    let mut o = String::from("{");
    o.push_str(&format!(
        "\"record_count\":{record_count},\"field_sums\":{{"
    ));
    for (i, (k, (v, sc))) in sums.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str(&format!("{}:{}", jstr(k), jstr(&fmt_sum(*v, *sc))));
    }
    o.push_str("},\"condition_true_counts\":{");
    for (i, (k, n)) in cond_true.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str(&format!("{}:{}", jstr(k), n));
    }
    o.push_str(&format!(
        "}},\"invalid_field_count\":{invalid},\"unsupported_field_count\":{unsupported},\"encoding\":{},",
        jstr(if encoding == Encoding::Cp500 { "cp500" } else { "ascii" })
    ));
    o.push_str(&format!(
        "\"stale_copybook_risk\":{}}}\n",
        jstr(STALE_COPYBOOK_RISK)
    ));
    Ok(o)
}

/// Decode every record to JSONL with a per-record `invalid_fields` list (dirty-data evidence). In
/// [`DirtyMode::Strict`], the first invalid field is a hard error; in [`DirtyMode::Evidence`] (the
/// default) invalid fields are preserved with their raw bytes and listed, never coerced away.
pub fn decode_records_json(
    copybook: &str,
    data: &[u8],
    record_len: usize,
    resolver: &impl CopyResolver,
    encoding: Encoding,
    mode: DirtyMode,
) -> Result<String, ShimError> {
    if record_len == 0 {
        return Err(ShimError::Layout("record_len must be > 0".into()));
    }
    let prog = parse_program(copybook, resolver)?;
    check_collisions(&prog)?;
    let mut out = String::new();
    for (idx, chunk) in data.chunks(record_len).enumerate() {
        let fields = decode_fields(&prog, chunk, encoding);
        let mut invalid: Vec<String> = Vec::new();
        let mut obj = format!("{{\"record_index\":{idx},\"fields\":{{");
        let mut first = true;
        for f in &fields {
            if f.category != "numeric" && f.category != "alphanumeric" {
                continue;
            }
            let ft = prog
                .attrs
                .get(&f.name)
                .map(|(a, _)| (a.field_type, a.flags))
                .unwrap_or((0, 0));
            let valid = chunk
                .get(f.offset..f.offset + f.size)
                .map(|b| field_valid(ft.0, ft.1, f.category, b))
                .unwrap_or(false);
            if !valid {
                invalid.push(f.name.clone());
                if mode == DirtyMode::Strict {
                    return Err(ShimError::BadItem(format!(
                        "dirty data (strict mode): field {} in record {idx} has invalid bytes {}",
                        f.name, f.raw_hex
                    )));
                }
            }
            if !first {
                obj.push(',');
            }
            first = false;
            obj.push_str(&format!("{}:{}", jstr(&f.name), jstr(&f.value)));
        }
        obj.push_str("},\"invalid_fields\":[");
        for (i, n) in invalid.iter().enumerate() {
            if i > 0 {
                obj.push(',');
            }
            obj.push_str(&jstr(n));
        }
        obj.push_str("]}\n");
        out.push_str(&obj);
    }
    Ok(out)
}

/// Parse `copybook` and refuse silent JSON key collisions (public entry for the reconcile path).
pub fn check_copybook_collisions(
    copybook: &str,
    resolver: &impl CopyResolver,
) -> Result<(), ShimError> {
    check_collisions(&parse_program(copybook, resolver)?)
}

/// Refuse silent JSON key collisions: two elementary fields decoding to the same name would clobber
/// each other in a flat JSON object. Exact COBOL names are preserved; ambiguity is surfaced, not hidden.
pub(crate) fn check_collisions(prog: &crate::Program) -> Result<(), ShimError> {
    use std::collections::HashMap;
    let mut seen: HashMap<&str, u32> = HashMap::new();
    for l in &prog.laid {
        if prog
            .attrs
            .get(&l.name)
            .map(|(_, c)| *c != "group")
            .unwrap_or(false)
        {
            *seen.entry(l.name.as_str()).or_insert(0) += 1;
        }
    }
    let dups: Vec<&str> = seen
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(k, _)| *k)
        .collect();
    if !dups.is_empty() {
        return Err(ShimError::BadItem(format!(
            "JSON key collision: duplicate field name(s) {dups:?} — qualify or rename (flattening is opt-in, never silent)"
        )));
    }
    Ok(())
}
