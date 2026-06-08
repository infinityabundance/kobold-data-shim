//! KOBOLD.CORPUS.2 — adversarial/banking-shaped fixtures proving the court refuses plausible wrongness.
//! Each hostile fixture must produce an expected fail-closed finding / behavior; NONE may silently decode
//! as clean. Buckets: 2A file/container · 2B storage · 2C banking · 2D database · 2E transform. No customer
//! data (all synthetic). This is fail-closed/dirty-evidence breadth — not production representativeness.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::banking::{AccountingProfile, NumericRole, PolarityProfile};
use kobold_data_shim::file::{ingest, IngestPolicy, PartialRecord, TrailingNewline};
use kobold_data_shim::operator::{decode_records_json, DirtyMode};
use kobold_data_shim::{
    db2host_evaluate, decode_record_encoded, reconcile_banking, reconcile_transform, ControlSpec,
    Encoding, ExitCode, IndicatorManifest, IndicatorPair, NoCopy, Transform, TransformOp, Variant,
    VariantSpec,
};

fn comp3(pic: &str, value: &str) -> Vec<u8> {
    enc(pic, Usage::Comp3, value)
}
fn enc(pic: &str, u: Usage, value: &str) -> Vec<u8> {
    let pf = build_field(pic, u, false, false).unwrap();
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    let (have, scale) = (fp.len() as i16, pf.attr.scale);
    if scale > have {
        d.resize(d.len() + (scale - have) as usize, 0);
    }
    while d.len() < pf.attr.digits as usize {
        d.insert(0, 0);
    }
    let extra = d.len().saturating_sub(pf.attr.digits as usize);
    d.drain(0..extra);
    let mut src: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    if neg {
        if let Some(l) = src.last_mut() {
            *l |= 0x40;
        }
    }
    let sattr = FieldAttr {
        field_type: COB_TYPE_NUMERIC_DISPLAY,
        digits: pf.attr.digits,
        scale: pf.attr.scale,
        flags: COB_FLAG_HAVE_SIGN,
    };
    let mut out = vec![0u8; pf.size];
    cob_move(&src, &sattr, &mut out, &pf.attr).unwrap();
    out
}

// --- 2A: file / container hostility ---
#[test]
fn corpus2a_file_container() {
    let strict = |rl| IngestPolicy::strict(rl);
    // short trailing record -> invalid shape (not silently absorbed)
    let short = [b'x'; 55 * 2 + 7];
    assert_eq!(
        ingest(&short, &strict(55)).unwrap_err().exit,
        ExitCode::InvalidInputShape
    );
    // unexpected final newline -> invalid shape
    let mut nl = vec![b'x'; 55];
    nl.push(b'\n');
    assert_eq!(
        ingest(&nl, &strict(55)).unwrap_err().exit,
        ExitCode::InvalidInputShape
    );
    // record_len 0 -> config error
    assert_eq!(
        ingest(b"abc", &strict(0)).unwrap_err().exit,
        ExitCode::IoOrConfigError
    );
    // partial preserved as EVIDENCE (warnings), never clean
    let ev = IngestPolicy {
        record_len: 55,
        trailing_newline: TrailingNewline::Reject,
        partial_record: PartialRecord::Evidence,
    };
    let partial = vec![b'x'; 55 + 9];
    let r = ingest(&partial, &ev).unwrap();
    assert!(r.partial_present && r.verdict == ExitCode::DecodedWithEvidenceWarnings);
}

// --- 2B: storage hostility ---
#[test]
fn corpus2b_storage() {
    // invalid packed sign nibble -> dirty evidence (operator flags it), never coerced
    let cb = "       01 R.\n           05 AMT PIC S9(3)V99 COMP-3.\n";
    let bad = [0x12u8, 0x34, 0x51]; // last nibble 1 = not a valid sign
    let je =
        decode_records_json(cb, &bad, 3, &NoCopy, Encoding::Ascii, DirtyMode::Evidence).unwrap();
    assert!(
        je.contains("\"invalid_fields\":[\"AMT\"]") || je.contains("AMT"),
        "dirty nibble flagged: {je}"
    );
    assert!(!je.contains("\"invalid_fields\":[]"), "must not be clean");
    // signed COMP-6 -> fails closed (GnuCOBOL converts S9 COMP-6 to COMP-3; the shim refuses)
    let cb6 = "       01 R.\n           05 N PIC S9(4) COMP-6.\n";
    let rec = decode_record_encoded(cb6, b"\x12\x34", &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "N").unwrap().category,
        "unsupported"
    );
}

fn bank_specs() -> (VariantSpec<'static>, ControlSpec<'static>) {
    const DTL: &str = "       01 D.\n           05 REC-TYPE PIC X.\n           05 DR-CR-IND PIC X.\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n           05 FILLER PIC X(18).\n";
    const TRL: &str = "       01 T.\n           05 REC-TYPE PIC X.\n           05 TRL-COUNT PIC 9(6).\n           05 TRL-DEBIT PIC S9(9)V99 COMP-3.\n           05 TRL-CREDIT PIC S9(9)V99 COMP-3.\n           05 FILLER PIC X(12).\n";
    let vs: &'static [Variant<'static>] = Box::leak(Box::new([
        Variant {
            discriminator: b"D",
            name: "D",
            copybook: DTL,
        },
        Variant {
            discriminator: b"T",
            name: "T",
            copybook: TRL,
        },
    ]));
    let v = VariantSpec {
        discriminator_offset: 0,
        discriminator_len: 1,
        variants: vs,
    };
    let roles: &'static [(&str, NumericRole)] =
        Box::leak(Box::new([("AMOUNT", NumericRole::Amount)]));
    let c = ControlSpec {
        detail_variant: "D",
        trailer_variant: "T",
        trailer_count_field: "TRL-COUNT",
        trailer_debit_field: "TRL-DEBIT",
        trailer_credit_field: "TRL-CREDIT",
        accounting: AccountingProfile {
            numeric_roles: roles,
            polarity: PolarityProfile {
                amount_field: "AMOUNT",
                source_field: "DR-CR-IND",
                debit_values: Box::leak(Box::new(["D"])),
                credit_values: Box::leak(Box::new(["C"])),
            },
        },
    };
    (v, c)
}
fn bank_detail(ind: u8, amt: &str) -> Vec<u8> {
    let mut d = vec![b'D', ind];
    d.extend(comp3("S9(7)V99", amt));
    d.resize(28, b' ');
    d
}
fn bank_trailer(count: u32, debit: &str, credit: &str) -> Vec<u8> {
    let mut t = vec![b'T'];
    t.extend(format!("{count:06}").into_bytes());
    t.extend(comp3("S9(9)V99", debit));
    t.extend(comp3("S9(9)V99", credit));
    t.resize(28, b' ');
    t
}

// --- 2C: banking truth-boundary hostility ---
#[test]
fn corpus2c_banking() {
    let (v, c) = bank_specs();
    // trailer mismatch -> CONTROL-MISMATCH
    let mut f = bank_detail(b'D', "100.00");
    f.extend(bank_trailer(1, "999.99", "0.00"));
    let r = reconcile_banking(&f, 28, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(
        !r.balanced
            && r.findings
                .iter()
                .any(|(rule, _)| rule == "KOBOLD-BANK-CONTROL-MISMATCH")
    );
    // unknown record type 'Z' -> UNKNOWN-RECORD-TYPE
    let mut z = bank_detail(b'D', "100.00");
    z[0] = b'Z';
    z.extend(bank_trailer(1, "100.00", "0.00"));
    let rz = reconcile_banking(&z, 28, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(rz
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-BANK-UNKNOWN-RECORD-TYPE"));
    // unknown polarity 'X' -> UNKNOWN-POLARITY (not summed by sign)
    let mut x = bank_detail(b'X', "100.00");
    x.extend(bank_trailer(1, "100.00", "0.00"));
    let rx = reconcile_banking(&x, 28, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(rx
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-BANK-UNKNOWN-POLARITY"));
}

// --- 2D: database boundary hostility ---
#[test]
fn corpus2d_database() {
    let cb = "       01 R.\n           05 LIMIT PIC S9(7)V99 COMP-3.\n           05 LIMIT-IND PIC S9(4) COMP-5.\n";
    let man = IndicatorManifest {
        pairs: Box::leak(Box::new([IndicatorPair {
            value_field: "LIMIT",
            indicator_field: "LIMIT-IND",
        }])),
    };
    let rec = |ind: &str| {
        let mut r = comp3("S9(7)V99", "1000.00");
        r.extend(enc("S9(4)", Usage::Comp5, ind));
        r
    };
    // indicator -1 -> semantic_null while bytes decode cleanly
    let n = db2host_evaluate(cb, &rec("-1"), &man, &NoCopy, Encoding::Ascii).unwrap();
    assert!(n.states[0].semantic_null && n.states[0].decoded_bytes_preserved);
    // indicator +5 -> truncation_evidence
    let t = db2host_evaluate(cb, &rec("5"), &man, &NoCopy, Encoding::Ascii).unwrap();
    assert!(t.states[0].truncation_evidence);
    // wrong-usage indicator (manifest points at a non-binary field) -> fail closed
    let cb_bad = "       01 R.\n           05 LIMIT PIC S9(7)V99 COMP-3.\n           05 LIMIT-IND PIC S9(4) COMP-3.\n";
    let mut rb = comp3("S9(7)V99", "1.00");
    rb.extend(enc("S9(4)", Usage::Comp3, "-1"));
    let b = db2host_evaluate(cb_bad, &rb, &man, &NoCopy, Encoding::Ascii).unwrap();
    assert!(b
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-DB2-WRONG-INDICATOR-USAGE"));
}

// --- 2E: transform hostility ---
#[test]
fn corpus2e_transform() {
    let cb = "       01 R.\n           05 ST PIC X.\n               88 ACTIVE VALUE \"A\".\n           05 BAL PIC S9(3)V99 COMP-3.\n";
    let mut input = vec![b'C'];
    input.extend(comp3("S9(3)V99", "100.00"));
    // undeclared condition + field -> fail closed, nothing changes
    let ts = [
        Transform {
            op: TransformOp::SetTrue,
            target: "NOPE",
            amount: None,
        },
        Transform {
            op: TransformOp::Add,
            target: "MISSING",
            amount: Some("1.00"),
        },
    ];
    let r = reconcile_transform(cb, &input, &ts, &NoCopy, Encoding::Ascii).unwrap();
    assert!(r
        .findings
        .iter()
        .any(|(x, _)| x == "KOBOLD-RECON2-UNDECLARED-CONDITION"));
    assert!(r
        .findings
        .iter()
        .any(|(x, _)| x == "KOBOLD-RECON2-UNDECLARED-FIELD"));
    assert_eq!(
        r.before, r.after,
        "undeclared transform changes nothing (no side effects)"
    );
}

// --- identifier: record truth is the numeric value; leading-zero identity needs a declared role ---
#[test]
fn corpus2_identifier_value_is_record_truth_bytes_preserved() {
    // PIC 9(10) "0000004217" decodes to the record-truth numeric value 4217 (what COBOL computes). The
    // leading-zero IDENTIFIER rendering is a declared-role concern (ACCOUNTING numeric_role=identifier /
    // NEG.IDENTIFIER.NUMERIC_COERCION) -- never inferred here. CORPUS.2 proves the original digits are
    // PRESERVED in raw_hex (the identifier is recoverable; nothing is lost) and the value is a STRING.
    let rec = decode_record_encoded(
        "       01 R.\n           05 ACCT-NO PIC 9(10).\n",
        b"0000004217",
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    let af = rec.fields.iter().find(|f| f.name == "ACCT-NO").unwrap();
    assert_eq!(af.value, "4217"); // record-truth numeric (a string, never a JSON number)
    assert_eq!(af.raw_hex, "30303030303034323137"); // raw digits preserved -> leading zeros recoverable
}
