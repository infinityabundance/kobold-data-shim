//! KOBOLD.DB2HOST.1 acceptance: a decoded field is marked semantic-null / truncation-evidence ONLY via
//! a declared indicator pairing (S9(4) COMP-5: negative=null, zero=present, positive=truncation). Decoded
//! bytes are always preserved; a missing or wrong-usage indicator fails closed; database truth is never
//! claimed.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::{db2host_evaluate, Encoding, IndicatorManifest, IndicatorPair, NoCopy};

/// Encode a value into `usage` storage via the sealed gnucobol-rs court (COMP-3 / COMP-5).
fn enc(pic: &str, usage: Usage, value: &str) -> Vec<u8> {
    let pf = build_field(pic, usage, false, false).unwrap();
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

const CB: &str = "       01 REC.\n           05 CUSTOMER-LIMIT PIC S9(7)V99 COMP-3.\n           05 CUSTOMER-LIMIT-IND PIC S9(4) COMP-5.\n";

fn record(limit: &str, ind: &str) -> Vec<u8> {
    let mut r = enc("S9(7)V99", Usage::Comp3, limit);
    r.extend(enc("S9(4)", Usage::Comp5, ind));
    r
}

fn manifest() -> IndicatorManifest<'static> {
    let pairs: &'static [IndicatorPair<'static>] = Box::leak(Box::new([IndicatorPair {
        value_field: "CUSTOMER-LIMIT",
        indicator_field: "CUSTOMER-LIMIT-IND",
    }]));
    IndicatorManifest { pairs }
}

#[test]
fn indicator_negative_marks_semantic_null() {
    let r = db2host_evaluate(
        CB,
        &record("1000.00", "-1"),
        &manifest(),
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    let s = &r.states[0];
    assert!(s.semantic_null && !s.present && !s.truncation_evidence);
    assert!(
        s.decoded_bytes_preserved,
        "decoded bytes preserved even when null"
    );
    assert!(
        r.audit_json.contains("\"semantic_null\":true")
            && r.audit_json.contains("\"indicator_raw_value\":-1")
    );
    assert!(r
        .casefile_json
        .contains("\"database_truth\":{\"claimed\":false"));
}

#[test]
fn indicator_zero_marks_present() {
    let r = db2host_evaluate(
        CB,
        &record("500.00", "0"),
        &manifest(),
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    let s = &r.states[0];
    assert!(s.present && !s.semantic_null && !s.truncation_evidence);
}

#[test]
fn indicator_positive_is_truncation_evidence() {
    let r = db2host_evaluate(
        CB,
        &record("250.00", "5"),
        &manifest(),
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    let s = &r.states[0];
    assert!(
        s.truncation_evidence && !s.present && !s.semantic_null,
        "positive indicator is truncation evidence, not ordinary present"
    );
}

#[test]
fn missing_declared_indicator_fails_closed() {
    // copybook without the indicator field, but the manifest declares one -> fail closed (no null claim).
    let cb_no_ind = "       01 REC.\n           05 CUSTOMER-LIMIT PIC S9(7)V99 COMP-3.\n";
    let r = db2host_evaluate(
        cb_no_ind,
        &enc("S9(7)V99", Usage::Comp3, "1.00"),
        &manifest(),
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    assert!(r
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-DB2-MISSING-INDICATOR"));
    assert!(
        r.states.is_empty(),
        "no null-state claim without the declared indicator"
    );
}

#[test]
fn wrong_indicator_usage_fails_closed() {
    // indicator declared as COMP-3 (not the required binary S9(4) COMP-5) -> fail closed.
    let cb = "       01 REC.\n           05 CUSTOMER-LIMIT PIC S9(7)V99 COMP-3.\n           05 CUSTOMER-LIMIT-IND PIC S9(4) COMP-3.\n";
    let mut rec = enc("S9(7)V99", Usage::Comp3, "1.00");
    rec.extend(enc("S9(4)", Usage::Comp3, "-1"));
    let r = db2host_evaluate(cb, &rec, &manifest(), &NoCopy, Encoding::Ascii).unwrap();
    assert!(
        r.findings
            .iter()
            .any(|(rule, _)| rule == "KOBOLD-DB2-WRONG-INDICATOR-USAGE"),
        "wrong indicator usage must fail closed; got {:?}",
        r.findings
    );
}
