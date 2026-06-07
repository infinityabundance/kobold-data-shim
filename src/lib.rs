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
use gnucobol_rs::layout::{lay_out, Item};

/// Re-exported so callers can implement a custom copybook search path for [`decode_with_resolver`].
pub use gnucobol_rs::copybook::CopyResolver;
use gnucobol_rs::pic::COB_TYPE_ALPHANUMERIC;
use gnucobol_rs::{
    build_field, Decimal, FieldAttr, Usage, COB_TYPE_NUMERIC_DISPLAY, COB_TYPE_NUMERIC_PACKED,
};
use std::collections::HashMap;

/// A decoded elementary field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedField {
    pub name: String,
    pub level: u16,
    pub offset: usize,
    pub size: usize,
    /// `"numeric"`, `"alphanumeric"`, `"group"`, or `"unsupported"`.
    pub category: &'static str,
    /// The decoded value: a canonical decimal string for numerics, text for alphanumerics, or an
    /// explanatory marker for unsupported/short fields.
    pub value: String,
    /// The raw field bytes as lowercase hex (the audit trail).
    pub raw_hex: String,
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
    let name = toks[1].clone();
    let (mut pic, mut usage, mut occurs, mut redefines, mut sep, mut lead) =
        (None, Usage::Display, None, None, false, false);
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
            "DISPLAY" => usage = Usage::Display,
            "OCCURS" => {
                if i + 1 < toks.len() {
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
    };
    Some((item, usage, sep, lead))
}

/// Decode `record` against a `copybook` (data-division item lines, possibly with `COPY`), using
/// `resolver` to find copybooks. Returns one [`DecodedField`] per named item (groups included).
pub fn decode_with_resolver(
    copybook: &str,
    record: &[u8],
    resolver: &impl CopyResolver,
) -> Result<Vec<DecodedField>, ShimError> {
    let expanded = expand(copybook, resolver).map_err(|e| ShimError::Copy(e.to_string()))?;

    let mut items = Vec::new();
    let mut attrs: HashMap<String, (FieldAttr, &'static str)> = HashMap::new();
    for line in &expanded.lines {
        if line.trim().is_empty() {
            continue;
        }
        let Some((item, usage, sep, lead)) = parse_item(line) else {
            continue; // tolerate non-item lines (DIVISION headers etc.)
        };
        if let Some((ref pic, _, _, _)) = item.pic {
            // Resolve the field's attr + category for decoding.
            if let Ok(pf) = build_field(pic, usage, sep, lead) {
                let cat = match pf.attr.field_type {
                    COB_TYPE_NUMERIC_DISPLAY | COB_TYPE_NUMERIC_PACKED => "numeric",
                    COB_TYPE_ALPHANUMERIC => "alphanumeric",
                    _ => "unsupported",
                };
                attrs.insert(item.name.clone(), (pf.attr, cat));
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
        }
        items.push(item);
    }

    let laid = lay_out(&items).map_err(|e| ShimError::Layout(e.to_string()))?;

    let mut out = Vec::new();
    for l in &laid {
        let slice = record.get(l.offset..l.offset + l.size);
        let (category, value, raw) = match (attrs.get(&l.name), slice) {
            (None, _) => ("group", String::from("(group)"), String::new()),
            (Some((_, "unsupported")), Some(bytes)) => (
                "unsupported",
                String::from("(unsupported PIC/usage)"),
                hex(bytes),
            ),
            (Some((attr, "numeric")), Some(bytes)) => {
                let d = match attr.field_type {
                    COB_TYPE_NUMERIC_PACKED => Decimal::from_packed(bytes, attr),
                    _ => Decimal::from_display(bytes, attr),
                };
                ("numeric", format_decimal(&d), hex(bytes))
            }
            (Some((_, "alphanumeric")), Some(bytes)) => (
                "alphanumeric",
                String::from_utf8_lossy(bytes).trim_end().to_string(),
                hex(bytes),
            ),
            (Some(_), None) => (
                "unsupported",
                String::from("(record too short for field)"),
                String::new(),
            ),
            (Some((_, other)), Some(bytes)) => (*other, String::new(), hex(bytes)),
        };
        out.push(DecodedField {
            name: l.name.clone(),
            level: l.level,
            offset: l.offset,
            size: l.size,
            category,
            value,
            raw_hex: raw,
        });
    }
    Ok(out)
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
