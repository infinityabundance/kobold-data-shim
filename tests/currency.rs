//! KOBOLD.CURRENCY.PROFILE.1 acceptance: a declared amount field is checked against an explicit scale +
//! optional currency-code field (evidence only); V99 alone is no money claim; a rate is not admitted as
//! money; sign is not polarity; FX/legal-tender/rounding/business meaning all refused.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::banking::NumericRole;
use kobold_data_shim::{
    currency_validate, CurrencyFieldProfile, CurrencyProfile, Encoding, NoCopy,
};

const CB: &str = "       01 REC.\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n           05 CCY PIC X(3).\n           05 RATE PIC 9(3)V99.\n           05 UNTOUCHED PIC 9(5)V99.\n";

fn comp3(value: &str) -> Vec<u8> {
    let pf = build_field("S9(7)V99", Usage::Comp3, false, false).unwrap();
    let (ip, fp) = value.split_once('.').unwrap_or((value, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    if pf.attr.scale > fp.len() as i16 {
        d.resize(d.len() + (pf.attr.scale - fp.len() as i16) as usize, 0);
    }
    while d.len() < pf.attr.digits as usize {
        d.insert(0, 0);
    }
    let extra = d.len().saturating_sub(pf.attr.digits as usize);
    d.drain(0..extra);
    let src: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    let sa = FieldAttr {
        field_type: COB_TYPE_NUMERIC_DISPLAY,
        digits: pf.attr.digits,
        scale: pf.attr.scale,
        flags: COB_FLAG_HAVE_SIGN,
    };
    let mut out = vec![0u8; pf.size];
    cob_move(&src, &sa, &mut out, &pf.attr).unwrap();
    out
}
fn rec() -> Vec<u8> {
    let mut r = comp3("12345.67"); // AMOUNT (scale 2)
    r.extend_from_slice(b"USD"); // CCY
    r.extend_from_slice(b"01250"); // RATE 9(3)V99 = 012.50
    r.extend_from_slice(b"0010000"); // UNTOUCHED 9(5)V99 = 00100.00
    r
}

#[test]
fn amount_validates_with_code_evidence_only() {
    static F: &[CurrencyFieldProfile] = &[CurrencyFieldProfile {
        field: "AMOUNT",
        numeric_role: NumericRole::Amount,
        currency_code_field: Some("CCY"),
        declared_scale: 2,
        scale_mismatch_is_finding: true,
    }];
    let m = currency_validate(
        CB,
        &rec(),
        &CurrencyProfile { fields: F },
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    assert!(
        m.findings.is_empty(),
        "scale 2 matches; got {:?}",
        m.findings
    );
    assert!(m.manifest_json.contains("\"field\":\"AMOUNT\",\"numeric_role\":\"amount\",\"admitted_as_money\":false,\"declared_scale\":2,\"observed_scale\":2,\"scale_match\":true,\"sign_is_polarity\":false,\"money_meaning_claimed\":false"));
    // currency code is EVIDENCE, never legal tender
    assert!(m
        .manifest_json
        .contains("\"currency_code_evidence\":\"USD\",\"legal_tender_truth\":false"));
    assert!(
        m.casefile_json.contains("\"money_meaning\":false")
            && m.casefile_json.contains("\"fx_conversion\":false")
    );
    assert!(m.casefile_json.contains("NEG.CURRENCY.SIGN_NOT_POLARITY"));
}

#[test]
fn scale_mismatch_and_rate_not_money() {
    static F: &[CurrencyFieldProfile] = &[
        CurrencyFieldProfile {
            field: "AMOUNT",
            numeric_role: NumericRole::Amount,
            currency_code_field: None,
            declared_scale: 3,
            scale_mismatch_is_finding: true,
        },
        CurrencyFieldProfile {
            field: "RATE",
            numeric_role: NumericRole::Rate,
            currency_code_field: None,
            declared_scale: 2,
            scale_mismatch_is_finding: true,
        },
    ];
    let m = currency_validate(
        CB,
        &rec(),
        &CurrencyProfile { fields: F },
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-CURRENCY-SCALE-MISMATCH")); // observed 2 != declared 3
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-CURRENCY-ROLE-NOT-AMOUNT")); // RATE is not money
    assert!(m
        .manifest_json
        .contains("\"field\":\"RATE\",\"numeric_role\":\"rate\",\"admitted_as_money\":false"));
}

#[test]
fn missing_field_and_missing_code_fail_closed() {
    static F: &[CurrencyFieldProfile] = &[
        CurrencyFieldProfile {
            field: "NO-SUCH",
            numeric_role: NumericRole::Amount,
            currency_code_field: None,
            declared_scale: 2,
            scale_mismatch_is_finding: true,
        },
        CurrencyFieldProfile {
            field: "AMOUNT",
            numeric_role: NumericRole::Amount,
            currency_code_field: Some("NO-CCY"),
            declared_scale: 2,
            scale_mismatch_is_finding: true,
        },
    ];
    let m = currency_validate(
        CB,
        &rec(),
        &CurrencyProfile { fields: F },
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-CURRENCY-MISSING-FIELD"));
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-CURRENCY-MISSING-CODE"));
}
