//! # kobold-data-shim
//!
//! A **verifiable** COBOL record-decoding shim for data-migration pipelines. Given a copybook and a
//! raw record dump, it answers *"what did this COBOL record actually mean?"* — byte-exactly —
//! by composing the oracle-proven [`gnucobol_rs`] courts:
//!
//! - `COPY` expansion ([`gnucobol_rs::copybook`], `GNURUST.5`/`6`),
//! - `PIC`→field model ([`gnucobol_rs::pic`], `GNURUST.3`),
//! - record layout / offsets ([`gnucobol_rs::layout`], `GNURUST.4`),
//! - packed/zoned/display decode ([`gnucobol_rs::Decimal`], `GNURUST.2`).
//!
//! Every numeric value this shim emits is decoded by a court that is proven byte-identical to
//! GnuCOBOL 3.2's `libcob` under a differential sweep. Fields outside the sealed subset **fail
//! closed** — they are reported as `unsupported`, never silently guessed (the reconciliation signal
//! that migrations need).
//!
//! ## License
//!
//! This crate's source is **Apache-2.0**. It links `gnucobol-rs`, which is **LGPL-3.0-or-later** —
//! see `NOTICE` for the obligations that places on distributed binaries (relink-ability + notice).

#![forbid(unsafe_code)]

use gnucobol_rs::copybook::expand;
use gnucobol_rs::layout::{lay_out, Item, Odo};

pub use file::{ingest, ExitCode, Ingest, IngestPolicy, PartialRecord, TrailingNewline};
/// Re-exported so callers can implement a custom copybook search path for [`decode_with_resolver`].
pub use gnucobol_rs::copybook::CopyResolver;
use gnucobol_rs::pic::COB_TYPE_ALPHANUMERIC;
use gnucobol_rs::{
    build_field, eval_88, translate_byte, CodePage, CondLit, CondValue, Condition, Decimal,
    FieldAttr, Usage, COB_FLAG_NO_SIGN_NIBBLE, COB_TYPE_NUMERIC_BINARY, COB_TYPE_NUMERIC_DISPLAY,
    COB_TYPE_NUMERIC_PACKED,
};

/// The record's declared character encoding (`KOBOLD.DATA.3`). **Never auto-detected** — the caller
/// states it. `Cp500` decodes *alphanumeric DISPLAY* fields through the sealed `GNURUST.15` table;
/// **binary and packed fields are raw storage domains and are never text-converted.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Encoding {
    /// Native ASCII (the default): DISPLAY bytes are already the logical characters.
    #[default]
    Ascii,
    /// EBCDIC cp500 (`ebcdic500_ascii8bit`): alphanumeric DISPLAY fields are decoded via the sealed
    /// `GNURUST.15` table. Numeric DISPLAY (EBCDIC zoned) is **deferred** and fails closed.
    Cp500,
}

/// The logical (decoded) bytes of a field under `encoding`: for an **alphanumeric** field under
/// [`Encoding::Cp500`], each raw EBCDIC byte is translated to its ASCII byte via the sealed
/// `GNURUST.15` cp500 table; otherwise the raw bytes are returned unchanged (binary/packed/ASCII
/// pass through untouched).
fn logical_bytes<'a>(
    category: &str,
    encoding: Encoding,
    raw: &'a [u8],
) -> std::borrow::Cow<'a, [u8]> {
    match (encoding, category) {
        (Encoding::Cp500, "alphanumeric") => std::borrow::Cow::Owned(
            raw.iter()
                .map(|&b| translate_byte(CodePage::Cp500, b).unwrap_or(b))
                .collect(),
        ),
        _ => std::borrow::Cow::Borrowed(raw),
    }
}
use std::collections::HashMap;

pub mod file;
pub mod operator;
pub mod recon;
pub mod sha256;
pub use operator::{control_totals, explain_field, DirtyMode, STALE_COPYBOOK_RISK};

/// A decoded elementary field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct DecodedField {
    pub name: String,
    pub level: u16,
    pub offset: usize,
    pub size: usize,
    /// `"numeric"`, `"alphanumeric"`, `"edited"`, `"group"`, or `"unsupported"`.
    pub category: &'static str,
    /// The decoded value: a canonical decimal string for numerics, text for alphanumerics, or an
    /// explanatory marker for unsupported/short fields.
    pub value: String,
    /// The raw field bytes as lowercase hex (the audit trail).
    pub raw_hex: String,
    /// For an **edited** DISPLAY field (`GNURUST.16`): the oracle-proven numeric interpretation of the
    /// presentation string (`value`). `None` for non-edited fields. JSON keeps `value` as the edited
    /// presentation string; this is the *interpreted* numeric, recorded as audit evidence — never a
    /// silent replacement.
    pub edited_numeric: Option<String>,
}

/// What went wrong decoding a record against a copybook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShimError {
    Copy(String),
    /// A copybook line could not be parsed into a data item.
    BadItem(String),
    /// The record/copybook produced no `01` record to lay out.
    Layout(String),
}

impl std::fmt::Display for ShimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShimError::Copy(e) => write!(f, "COPY expansion failed: {e}"),
            ShimError::BadItem(l) => write!(f, "could not parse copybook item: {l}"),
            ShimError::Layout(e) => write!(f, "record layout failed: {e}"),
        }
    }
}
impl std::error::Error for ShimError {}

/// A resolver that finds nothing (for copybooks with no nested `COPY`).
pub struct NoCopy;
impl CopyResolver for NoCopy {
    fn resolve(&self, _name: &str) -> Option<String> {
        None
    }
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

/// Parse one COBOL data-item line into a layout [`Item`], plus the elementary field's
/// `(usage, sign_separate, sign_leading)` for decoding.
fn parse_item(decl: &str) -> Option<(Item, Usage, bool, bool)> {
    // Strip the clause-terminating period(s) so a PIC token isn't captured as e.g. "9(3).".
    let decl = decl.trim_end().trim_end_matches('.');
    let toks: Vec<String> = decl
        .split_whitespace()
        .map(|s| s.to_ascii_uppercase())
        .collect();
    if toks.len() < 2 {
        return None;
    }
    let level: u16 = toks[0].parse().ok()?;
    if level == 88 {
        return None; // a LEVEL-88 condition, handled separately (not a storage item)
    }
    let name = toks[1].clone();
    let (mut pic, mut usage, mut occurs, mut redefines, mut sep, mut lead, mut odo) =
        (None, Usage::Display, None, None, false, false, None);
    let mut i = 2;
    while i < toks.len() {
        match toks[i].as_str() {
            "PIC" | "PICTURE" => {
                if i + 1 < toks.len() {
                    pic = Some(toks[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            "COMP-3" | "PACKED-DECIMAL" | "COMPUTATIONAL-3" => usage = Usage::Comp3,
            "COMP" | "BINARY" | "COMPUTATIONAL" => usage = Usage::Comp,
            "COMP-5" => usage = Usage::Comp5,
            "COMP-X" => usage = Usage::CompX,
            "COMP-6" | "COMPUTATIONAL-6" => usage = Usage::Comp6,
            "DISPLAY" => usage = Usage::Display,
            "OCCURS" => {
                // "OCCURS min TO max TIMES DEPENDING ON item" (ODO) or "OCCURS n TIMES" (fixed).
                if i + 3 < toks.len() && toks[i + 2] == "TO" {
                    let min = toks[i + 1].parse().unwrap_or(0);
                    let max = toks[i + 3].parse().unwrap_or(0);
                    let mut dep = String::new();
                    let mut k = i + 4;
                    while k + 2 < toks.len() {
                        if toks[k] == "DEPENDING" && toks[k + 1] == "ON" {
                            dep = toks[k + 2].clone();
                            break;
                        }
                        k += 1;
                    }
                    odo = Some(Odo {
                        min,
                        max,
                        depending_on: dep,
                    });
                    i += 4;
                    continue;
                } else if i + 1 < toks.len() {
                    occurs = toks[i + 1].parse().ok();
                    i += 2;
                    continue;
                }
            }
            "REDEFINES" => {
                if i + 1 < toks.len() {
                    redefines = Some(toks[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            "SEPARATE" => sep = true,
            "LEADING" => lead = true,
            _ => {}
        }
        i += 1;
    }
    let item = Item {
        level,
        name,
        pic: pic.map(|p| (p, usage, sep, lead)),
        occurs,
        redefines,
        odo,
    };
    Some((item, usage, sep, lead))
}

/// Parse a `88 NAME VALUE …` condition line into `(name, values)`. Handles `"lit"` / numeric
/// literals, multiple values, and `THRU` ranges. Returns `None` for non-88 lines.
fn parse_88(decl: &str) -> Option<(String, Vec<CondValue>)> {
    let decl = decl.trim_end().trim_end_matches('.');
    let mut words = decl.split_whitespace();
    if words.next()? != "88" {
        return None;
    }
    let name = words.next()?.to_ascii_uppercase();
    if !words.next()?.eq_ignore_ascii_case("VALUE") {
        return None;
    }
    // Tokenize the remaining text into literals and the THRU keyword (quotes preserved).
    let rest: String = decl
        .split_once("VALUE")
        .or_else(|| decl.split_once("value"))
        .map(|(_, r)| r.to_string())
        .unwrap_or_default();
    let lits = tokenize_values(&rest);
    let mut values = Vec::new();
    let mut i = 0;
    while i < lits.len() {
        if i + 2 < lits.len() && lits[i + 1].eq_ignore_ascii_case("THRU") {
            values.push(CondValue::Range(make_lit(&lits[i]), make_lit(&lits[i + 2])));
            i += 3;
        } else {
            values.push(CondValue::Lit(make_lit(&lits[i])));
            i += 1;
        }
    }
    if values.is_empty() {
        return None;
    }
    Some((name, values))
}

/// Split a VALUE clause body into literal/keyword tokens (a quoted string is one token).
fn tokenize_values(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' || c == '\'' {
            let q = c;
            chars.next();
            let mut lit = String::from("\"");
            for ch in chars.by_ref() {
                if ch == q {
                    break;
                }
                lit.push(ch);
            }
            lit.push('"');
            out.push(lit);
        } else {
            let mut tok = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    break;
                }
                tok.push(ch);
                chars.next();
            }
            out.push(tok);
        }
    }
    out
}

/// A token like `"A"` becomes an alphanumeric literal; everything else is numeric.
fn make_lit(tok: &str) -> CondLit {
    if let Some(inner) = tok.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        CondLit::Alpha(inner.to_string())
    } else {
        CondLit::Num(tok.to_string())
    }
}

/// A decoded LEVEL-88 condition: its truth (or an error marker) for the current record.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DecodedCondition {
    pub name: String,
    pub parent: String,
    /// `Some(true/false)` if evaluated, `None` if the parent/values were unsupported.
    pub value: Option<bool>,
}

/// A fully decoded record: its fields and condition-name truths.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DecodedRecord {
    pub fields: Vec<DecodedField>,
    pub conditions: Vec<DecodedCondition>,
}

impl DecodedRecord {
    /// Count of fields outside the sealed subset (the reconciliation signal).
    pub fn unsupported(&self) -> usize {
        self.fields
            .iter()
            .filter(|f| f.category == "unsupported")
            .count()
    }
}

/// A parsed program: the laid-out items, per-field attrs/category, and LEVEL-88 conditions keyed by
/// parent field. Shared by the field decoder and the condition evaluator.
/// Per-field provenance + declaration metadata for the operator `explain` evidence (`KOBOLD.OPERATOR.1`).
#[derive(Debug, Clone, Default)]
pub(crate) struct FieldMeta {
    pub pic: String,
    pub usage: String,
    pub source_file: String,
    pub source_line: usize,
}

struct Program {
    laid: Vec<gnucobol_rs::Laid>,
    attrs: HashMap<String, (FieldAttr, &'static str)>,
    conditions: Vec<(String, Condition)>,
    meta: HashMap<String, FieldMeta>,
    /// True if expansion spliced more than one source file (a `COPY` was used).
    used_copy: bool,
}

fn usage_label(u: Usage) -> &'static str {
    match u {
        Usage::Display => "DISPLAY",
        Usage::Comp3 => "COMP-3",
        Usage::Comp => "COMP",
        Usage::Comp5 => "COMP-5",
        Usage::CompX => "COMP-X",
        Usage::Comp6 => "COMP-6",
        _ => "UNKNOWN-USAGE",
    }
}

fn parse_program(copybook: &str, resolver: &impl CopyResolver) -> Result<Program, ShimError> {
    let expanded = expand(copybook, resolver).map_err(|e| ShimError::Copy(e.to_string()))?;
    let mut items = Vec::new();
    let mut attrs: HashMap<String, (FieldAttr, &'static str)> = HashMap::new();
    let mut conditions: Vec<(String, Condition)> = Vec::new();
    let mut meta: HashMap<String, FieldMeta> = HashMap::new();
    let mut last_parent: Option<String> = None;

    for (idx, line) in expanded.lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // A LEVEL-88 condition attaches to the most recent elementary field.
        if let Some((cname, values)) = parse_88(line) {
            if let Some(parent) = &last_parent {
                conditions.push((
                    parent.clone(),
                    Condition {
                        name: cname,
                        values,
                    },
                ));
            }
            continue;
        }
        let Some((item, usage, sep, lead)) = parse_item(line) else {
            continue; // tolerate non-item lines (DIVISION headers etc.)
        };
        if let Some((ref pic, _, _, _)) = item.pic {
            // Signed COMP-6 is NOT admitted (KOBOLD.DATA.6 / GNURUST.18): GnuCOBOL silently converts
            // `S9(n) COMP-6` to COMP-3, so we fail closed rather than treat it as unsigned COMP-6.
            let signed_comp6 =
                usage == Usage::Comp6 && pic.trim_start().to_ascii_uppercase().starts_with('S');
            let built = if signed_comp6 {
                None
            } else {
                build_field(pic, usage, sep, lead).ok()
            };
            if let Some(pf) = built {
                let cat = match pf.attr.field_type {
                    COB_TYPE_NUMERIC_DISPLAY
                    | COB_TYPE_NUMERIC_PACKED
                    | COB_TYPE_NUMERIC_BINARY => "numeric",
                    COB_TYPE_ALPHANUMERIC => "alphanumeric",
                    _ => "unsupported",
                };
                attrs.insert(item.name.clone(), (pf.attr, cat));
                let prov = expanded.provenance.get(idx);
                meta.insert(
                    item.name.clone(),
                    FieldMeta {
                        pic: pic.clone(),
                        usage: usage_label(usage).to_string(),
                        source_file: prov.map(|p| p.file.clone()).unwrap_or_default(),
                        source_line: prov.map(|p| p.line).unwrap_or(0),
                    },
                );
            } else if usage == Usage::Display && gnucobol_rs::edited_size(pic).is_ok() {
                // An edited DISPLAY picture (GNURUST.16): decoded by the `edited` court, not `pic`.
                // (An edited PIC with a binary/packed USAGE is the wrong domain → falls through to
                // "unsupported" below, since it only reaches here when usage == Display.)
                attrs.insert(
                    item.name.clone(),
                    (
                        FieldAttr {
                            field_type: 0,
                            digits: 0,
                            scale: 0,
                            flags: 0,
                        },
                        "edited",
                    ),
                );
                let prov = expanded.provenance.get(idx);
                meta.insert(
                    item.name.clone(),
                    FieldMeta {
                        pic: pic.clone(),
                        usage: "DISPLAY (edited)".to_string(),
                        source_file: prov.map(|p| p.file.clone()).unwrap_or_default(),
                        source_line: prov.map(|p| p.line).unwrap_or(0),
                    },
                );
            } else {
                attrs.insert(
                    item.name.clone(),
                    (
                        FieldAttr {
                            field_type: 0,
                            digits: 0,
                            scale: 0,
                            flags: 0,
                        },
                        "unsupported",
                    ),
                );
            }
            last_parent = Some(item.name.clone());
        }
        items.push(item);
    }
    let laid = lay_out(&items).map_err(|e| ShimError::Layout(e.to_string()))?;
    let mut files: Vec<&str> = expanded
        .provenance
        .iter()
        .map(|p| p.file.as_str())
        .collect();
    files.sort_unstable();
    files.dedup();
    let used_copy = files.len() > 1;
    Ok(Program {
        laid,
        attrs,
        conditions,
        meta,
        used_copy,
    })
}

fn decode_fields(prog: &Program, record: &[u8], encoding: Encoding) -> Vec<DecodedField> {
    let mut out = Vec::new();
    for l in &prog.laid {
        let slice = record.get(l.offset..l.offset + l.size);
        let (category, value, raw, edited_numeric) = match (prog.attrs.get(&l.name), slice) {
            (None, _) => ("group", String::from("(group)"), String::new(), None),
            (Some((_, "unsupported")), Some(bytes)) => (
                "unsupported",
                String::from("(unsupported PIC/usage)"),
                hex(bytes),
                None,
            ),
            // Numeric DISPLAY (zoned) under EBCDIC decodes through GNURUST.17 (KOBOLD.DATA.5): cp500
            // translate + the cob_get_sign_ebcdic overpunch sign. Binary/packed stay RAW (next arm).
            (Some((attr, "numeric")), Some(bytes))
                if encoding == Encoding::Cp500 && attr.field_type == COB_TYPE_NUMERIC_DISPLAY =>
            {
                let d = Decimal::from_ebcdic_zoned(bytes, attr);
                ("numeric", format_decimal(&d), hex(bytes), None)
            }
            (Some((attr, "numeric")), Some(bytes)) => {
                // Binary/packed are RAW storage domains — never text-converted (passthrough).
                let d = match attr.field_type {
                    COB_TYPE_NUMERIC_PACKED => Decimal::from_packed(bytes, attr),
                    COB_TYPE_NUMERIC_BINARY => Decimal::from_binary(bytes, attr),
                    _ => Decimal::from_display(bytes, attr),
                };
                ("numeric", format_decimal(&d), hex(bytes), None)
            }
            // Edited DISPLAY field (GNURUST.16): JSON keeps the presentation string; the oracle-proven
            // numeric goes to `edited_numeric` (audit), never silently replacing the text. Edited under
            // cp500 is deferred (the decode table is ASCII) → fail closed.
            (Some((_, "edited")), Some(bytes)) if encoding == Encoding::Cp500 => (
                "unsupported",
                String::from("(edited picture under EBCDIC: deferred)"),
                hex(bytes),
                None,
            ),
            (Some((_, "edited")), Some(bytes)) => {
                let pic = prog.meta.get(&l.name).map(|m| m.pic.as_str()).unwrap_or("");
                match gnucobol_rs::decode_edited(pic, bytes) {
                    Ok(d) => {
                        let num = d.numeric_value.as_ref().map(format_decimal);
                        ("edited", d.raw_text.trim_end().to_string(), hex(bytes), num)
                    }
                    Err(_) => (
                        "unsupported",
                        String::from("(edited decode failed)"),
                        hex(bytes),
                        None,
                    ),
                }
            }
            (Some((_, "alphanumeric")), Some(bytes)) => (
                "alphanumeric",
                String::from_utf8_lossy(&logical_bytes("alphanumeric", encoding, bytes))
                    .trim_end()
                    .to_string(),
                hex(bytes),
                None,
            ),
            (Some(_), None) => (
                "unsupported",
                String::from("(record too short for field)"),
                String::new(),
                None,
            ),
            (Some((_, other)), Some(bytes)) => (*other, String::new(), hex(bytes), None),
        };
        out.push(DecodedField {
            name: l.name.clone(),
            level: l.level,
            offset: l.offset,
            size: l.size,
            category,
            value,
            raw_hex: raw,
            edited_numeric,
        });
    }
    out
}

fn eval_conditions(prog: &Program, record: &[u8], encoding: Encoding) -> Vec<DecodedCondition> {
    let mut out = Vec::new();
    for (parent, cond) in &prog.conditions {
        let value = prog.laid.iter().find(|l| &l.name == parent).and_then(|l| {
            let (attr, cat) = prog.attrs.get(parent)?;
            let bytes = record.get(l.offset..l.offset + l.size)?;
            // 88 literals in the copybook are ASCII, so under cp500 the parent's alphanumeric bytes
            // are decoded to ASCII *before* the predicate runs (raw EBCDIC would never match).
            let logical = logical_bytes(cat, encoding, bytes);
            eval_88(attr, &logical, cond).ok()
        });
        out.push(DecodedCondition {
            name: cond.name.clone(),
            parent: parent.clone(),
            value,
        });
    }
    out
}

/// Decode `record` against a `copybook` (data-division item lines, possibly with `COPY`), using
/// `resolver` to find copybooks. Returns one [`DecodedField`] per named item (groups included).
pub fn decode_with_resolver(
    copybook: &str,
    record: &[u8],
    resolver: &impl CopyResolver,
) -> Result<Vec<DecodedField>, ShimError> {
    let prog = parse_program(copybook, resolver)?;
    Ok(decode_fields(&prog, record, Encoding::Ascii))
}

/// Decode a record into its fields **and** its LEVEL-88 condition-name truths (`eval_88`), as native
/// ASCII. For EBCDIC, use [`decode_record_encoded`].
pub fn decode_record(
    copybook: &str,
    record: &[u8],
    resolver: &impl CopyResolver,
) -> Result<DecodedRecord, ShimError> {
    decode_record_encoded(copybook, record, resolver, Encoding::Ascii)
}

/// Decode a record under an explicit [`Encoding`] (`KOBOLD.DATA.3`). Under [`Encoding::Cp500`],
/// alphanumeric DISPLAY fields (and the parent bytes feeding `eval_88`) are decoded through the
/// sealed `GNURUST.15` cp500 table; binary/packed fields pass through as raw storage.
pub fn decode_record_encoded(
    copybook: &str,
    record: &[u8],
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<DecodedRecord, ShimError> {
    let prog = parse_program(copybook, resolver)?;
    Ok(DecodedRecord {
        fields: decode_fields(&prog, record, encoding),
        conditions: eval_conditions(&prog, record, encoding),
    })
}

/// Decode a buffer of fixed-length records into one [`DecodedRecord`] per record (fields +
/// conditions). A higher-level iterator over `data.chunks(record_len)`; a trailing partial record is
/// decoded as-is (its short fields are reported `unsupported`, never guessed).
pub fn decode_all(
    copybook: &str,
    data: &[u8],
    record_len: usize,
    resolver: &impl CopyResolver,
) -> Result<Vec<DecodedRecord>, ShimError> {
    if record_len == 0 {
        return Err(ShimError::Layout("record_len must be > 0".into()));
    }
    data.chunks(record_len)
        .map(|chunk| decode_record(copybook, chunk, resolver))
        .collect()
}

/// Decode a record against a copybook with no nested `COPY`.
pub fn decode(copybook: &str, record: &[u8]) -> Result<Vec<DecodedField>, ShimError> {
    decode_with_resolver(copybook, record, &NoCopy)
}

/// Render a [`Decimal`] as a canonical signed decimal string (e.g. `-12.34`).
fn format_decimal(d: &Decimal) -> String {
    let mut digits: String = d.digits.iter().map(|x| (b'0' + x) as char).collect();
    let scale = d.scale.max(0) as usize;
    while digits.len() <= scale {
        digits.insert(0, '0');
    }
    let split = digits.len() - scale;
    let mut s = String::new();
    if d.negative {
        s.push('-');
    }
    // strip leading zeros in the integer part (keep one)
    let int_part = digits[..split].trim_start_matches('0');
    s.push_str(if int_part.is_empty() { "0" } else { int_part });
    if scale > 0 {
        s.push('.');
        s.push_str(&digits[split..]);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_record() {
        // 01 CUST. 05 CUST-ID PIC 9(3). 05 CUST-NAME PIC X(4). 05 CUST-BAL PIC S9(3)V99 COMP-3.
        let cb = "01 CUST.\n05 CUST-ID PIC 9(3).\n05 CUST-NAME PIC X(4).\n05 CUST-BAL PIC S9(3)V99 COMP-3.";
        // record: "042" + "ANNA" + COMP-3(-12.34 as S9(3)V99 = 01234d)
        let mut rec = Vec::new();
        rec.extend_from_slice(b"042");
        rec.extend_from_slice(b"ANNA");
        rec.extend_from_slice(&[0x01, 0x23, 0x4d]);
        let fields = decode(cb, &rec).unwrap();
        let by = |n: &str| fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by("CUST-ID").value, "42");
        assert_eq!(by("CUST-NAME").value, "ANNA");
        assert_eq!(by("CUST-BAL").value, "-12.34");
        assert_eq!(by("CUST-BAL").raw_hex, "01234d");
        assert_eq!(by("CUST").category, "group");
    }
}
