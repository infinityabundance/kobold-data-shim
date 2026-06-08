//! KOBOLD.RECON.2 — declared transformed-record reconciliation.
//!
//! **Doctrine.** KOBOLD.RECON.2 admits only declared transformed-record reconciliation: it proves
//! before/after bytes, decoded values, and audit deltas for **named sealed transforms**, while
//! Procedure Division execution, production write-back, file rewrite parity, ledger acceptance, and
//! business truth remain non-claims.
//!
//! A transform is one of the **sealed** byte courts applied to one field of a record:
//! `SET condition-name TO TRUE` (`GNURUST.12`) or `ADD`/`SUBTRACT` a declared amount (`GNURUST.7`).
//! The court captures the input bytes, applies the declared transform, captures the output bytes, and
//! decodes both — proving *read truth* and *transform truth* and refusing everything above them.

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, parse_program, CopyResolver, Encoding, ShimError};
use gnucobol_rs::{
    build_field, cob_arith, set_88_true, FieldAttr, Op, Round, Usage, COB_FLAG_HAVE_SIGN,
    COB_TYPE_NUMERIC_DISPLAY,
};

/// A declared transform operation over one field — each a sealed `gnucobol-rs` court.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransformOp {
    /// `SET condition-name TO TRUE` (`GNURUST.12`).
    SetTrue,
    /// `ADD amount TO field` (`GNURUST.7`).
    Add,
    /// `SUBTRACT amount FROM field` (`GNURUST.7`).
    Subtract,
}

/// One declared transform: an op, the target name (a condition for `SetTrue`, else a field), and an
/// optional decimal amount literal for `Add`/`Subtract`.
pub struct Transform<'a> {
    pub op: TransformOp,
    pub target: &'a str,
    pub amount: Option<&'a str>,
}

/// The result of a declared transformed-record reconciliation.
pub struct ReconTransformResult {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub applied: Vec<String>,
    pub findings: Vec<(String, String)>,
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

/// Encode a decimal `amount` literal into a DISPLAY operand whose scale matches `field` (so `cob_arith`
/// aligns). Reuses the sealed `cob_move`/`build_field`.
fn amount_bytes(amount: &str, field: &FieldAttr) -> (Vec<u8>, FieldAttr) {
    let digits = field.digits.max(1);
    let scale = field.scale.max(0);
    let pic = if scale > 0 {
        format!("S9({})V9({scale})", (digits as i16 - scale).max(1))
    } else {
        format!("S9({digits})")
    };
    let pf = build_field(&pic, Usage::Display, false, false).expect("amount pic");
    let neg = amount.starts_with('-');
    let t = amount.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    let have = fp.len() as i16;
    if pf.attr.scale > have {
        d.resize(d.len() + (pf.attr.scale - have) as usize, 0);
    } else if pf.attr.scale < have {
        d.truncate(d.len() - (have - pf.attr.scale) as usize);
    }
    while d.len() < pf.attr.digits as usize {
        d.insert(0, 0);
    }
    let extra = d.len().saturating_sub(pf.attr.digits as usize);
    d.drain(0..extra);
    let mut bytes: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    if neg {
        if let Some(l) = bytes.last_mut() {
            *l |= 0x40;
        }
    }
    let attr = FieldAttr {
        field_type: COB_TYPE_NUMERIC_DISPLAY,
        digits: pf.attr.digits,
        scale: pf.attr.scale,
        flags: COB_FLAG_HAVE_SIGN,
    };
    (bytes, attr)
}

/// Apply a sequence of declared transforms to `input`, proving before/after bytes + decoded deltas.
pub fn reconcile_transform(
    copybook: &str,
    input: &[u8],
    transforms: &[Transform],
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<ReconTransformResult, ShimError> {
    let prog = parse_program(copybook, resolver)?;
    let before = input.to_vec();
    let mut after = input.to_vec();
    let mut applied: Vec<String> = Vec::new();
    let mut findings: Vec<(String, String)> = Vec::new();
    let mut courts: Vec<&str> = Vec::new();

    for t in transforms {
        match t.op {
            TransformOp::SetTrue => {
                let Some((parent, cond)) = prog.conditions.iter().find(|(_, c)| c.name == t.target)
                else {
                    findings.push((
                        "KOBOLD-RECON2-UNDECLARED-CONDITION".into(),
                        format!("no LEVEL-88 named {:?}", t.target),
                    ));
                    continue;
                };
                let (Some(l), Some((attr, _))) = (
                    prog.laid.iter().find(|l| &l.name == parent),
                    prog.attrs.get(parent),
                ) else {
                    findings.push((
                        "KOBOLD-RECON2-NO-PARENT".into(),
                        format!("condition {:?} parent not laid out", t.target),
                    ));
                    continue;
                };
                match set_88_true(attr, l.size, cond) {
                    Ok(bytes) if bytes.len() == l.size => {
                        after[l.offset..l.offset + l.size].copy_from_slice(&bytes);
                        applied.push(format!("SET {} TO TRUE (GNURUST.12)", t.target));
                        courts.push("SET-88-TRUE (GNURUST.12)");
                    }
                    _ => findings.push((
                        "KOBOLD-RECON2-SET-FAILED".into(),
                        format!("SET {} TO TRUE failed closed", t.target),
                    )),
                }
            }
            TransformOp::Add | TransformOp::Subtract => {
                let Some(amt) = t.amount else {
                    findings.push((
                        "KOBOLD-RECON2-NO-AMOUNT".into(),
                        format!("{:?} needs an amount", t.target),
                    ));
                    continue;
                };
                let (Some(l), Some((attr, cat))) = (
                    prog.laid.iter().find(|l| l.name == t.target),
                    prog.attrs.get(t.target),
                ) else {
                    findings.push((
                        "KOBOLD-RECON2-UNDECLARED-FIELD".into(),
                        format!("no field named {:?}", t.target),
                    ));
                    continue;
                };
                if *cat != "numeric" {
                    findings.push((
                        "KOBOLD-RECON2-NOT-NUMERIC".into(),
                        format!("{:?} is not a numeric field", t.target),
                    ));
                    continue;
                }
                let (bb, battr) = amount_bytes(amt, attr);
                let (op, verb, prep) = if t.op == TransformOp::Add {
                    (Op::Add, "ADD", "TO")
                } else {
                    (Op::Subtract, "SUBTRACT", "FROM")
                };
                match cob_arith(
                    op,
                    &after[l.offset..l.offset + l.size],
                    attr,
                    &bb,
                    &battr,
                    Round::Truncate,
                ) {
                    Ok(bytes) if bytes.len() == l.size => {
                        after[l.offset..l.offset + l.size].copy_from_slice(&bytes);
                        applied.push(format!("{verb} {amt} {prep} {} (GNURUST.7)", t.target));
                        courts.push("ADD/SUBTRACT (GNURUST.7)");
                    }
                    Ok(_) => findings.push((
                        "KOBOLD-RECON2-SIZE".into(),
                        format!("{:?} result size mismatch", t.target),
                    )),
                    Err(e) => findings.push((
                        "KOBOLD-RECON2-ARITH-FAILED".into(),
                        format!("{verb} on {:?} failed closed: {e}", t.target),
                    )),
                }
            }
        }
    }

    // before/after decode for the delta
    let bd = decode_record_encoded(copybook, &before, resolver, encoding)?;
    let ad = decode_record_encoded(copybook, &after, resolver, encoding)?;
    let mut delta = String::new();
    let mut first = true;
    for (b, a) in bd.fields.iter().zip(ad.fields.iter()) {
        if b.value != a.value {
            if !first {
                delta.push(',');
            }
            first = false;
            delta.push_str(&format!(
                "{}:{{\"before\":{},\"after\":{}}}",
                jstr(&b.name),
                jstr(&b.value),
                jstr(&a.value)
            ));
        }
    }
    courts.sort_unstable();
    courts.dedup();
    let court_json = courts.iter().map(|c| jstr(c)).collect::<Vec<_>>().join(",");
    let applied_json = applied
        .iter()
        .map(|a| jstr(a))
        .collect::<Vec<_>>()
        .join(",");
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
            "{{\"schema\":\"kobold-recon2-forensic-casefile-v1\",\"court\":\"KOBOLD.RECON.2\",",
            "\"input_sha256\":{},\"output_sha256\":{},\"bytes_changed\":{},",
            "\"transform_manifest\":{{\"applied\":[{}],\"sealed_courts\":[{}]}},",
            "\"audit_delta\":{{{}}},",
            "\"read_truth\":{{\"claimed\":true}},",
            "\"transform_truth\":{{\"claimed\":true,\"scope\":\"declared sealed transform only\"}},",
            "\"write_back_truth\":{{\"claimed\":false}},\"posting_truth\":{{\"claimed\":false}},",
            "\"ledger_truth\":{{\"claimed\":false}},\"business_truth\":{{\"claimed\":false}},",
            "\"negative_capabilities\":[\"NEG.RECON.PROCEDURE_DIVISION\",\"NEG.RECON.WRITE_BACK_PRODUCTION\",",
            "\"NEG.RECON.BUSINESS_TRUTH\",\"NEG.RECON.UNDECLARED_TRANSFORM\",\"NEG.RECON.SIDE_EFFECTS\",",
            "\"NEG.RECON.FILE_REWRITE_PARITY\",\"NEG.RECON.LEDGER_ACCEPTANCE\"],",
            "\"findings\":[{}]}}\n"
        ),
        jstr(&sha256_hex(&before)),
        jstr(&sha256_hex(&after)),
        before != after,
        applied_json,
        court_json,
        delta,
        find_json,
    );

    Ok(ReconTransformResult {
        before,
        after,
        applied,
        findings,
        casefile_json,
    })
}
