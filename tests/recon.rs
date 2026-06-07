//! KOBOLD.RECON.1 acceptance: the committed corpus reconciles to byte-stable JSONL + audit, replay
//! is identical, the CLI and library paths agree, and the condition-set fixture round-trips through
//! `eval_88`. The committed `recon/*/expected.jsonl` etc. are the golden outputs (drift = test fail).

use gnucobol_rs::{build_field, eval_88, set_88_true, CondLit, CondValue, Condition, Usage};
use kobold_data_shim::recon::{reconcile, reconcile_encoded};
use kobold_data_shim::CopyResolver;
use kobold_data_shim::Encoding;

struct DirResolver(String);
impl CopyResolver for DirResolver {
    fn resolve(&self, name: &str) -> Option<String> {
        for base in [name.to_string(), name.to_ascii_lowercase()] {
            for ext in ["", ".cpy", ".CPY"] {
                if let Ok(s) = std::fs::read_to_string(format!("{}/{base}{ext}", self.0)) {
                    return Some(s);
                }
            }
        }
        None
    }
}

fn check_family(fixture: &str, dir: &str, copybook: &str, record_len: usize) {
    let cb = std::fs::read_to_string(format!("{dir}/{copybook}")).unwrap();
    let data = std::fs::read(format!("{dir}/input.dat")).unwrap();
    let resolver = DirResolver(dir.to_string());

    let r1 = reconcile(fixture, &cb, &data, record_len, "0.4.1", &resolver).unwrap();
    // byte-stable replay: a second run is identical.
    let r2 = reconcile(fixture, &cb, &data, record_len, "0.4.1", &resolver).unwrap();
    assert_eq!(
        r1.jsonl, r2.jsonl,
        "{fixture}: jsonl not byte-stable across runs"
    );
    assert_eq!(
        r1.audit_json, r2.audit_json,
        "{fixture}: audit not byte-stable"
    );

    // matches the committed golden outputs (the CLI wrote these, so this also proves CLI == lib).
    let golden_jsonl = std::fs::read_to_string(format!("{dir}/expected.jsonl")).unwrap();
    let golden_audit = std::fs::read_to_string(format!("{dir}/audit.json")).unwrap();
    let golden_unsup = std::fs::read_to_string(format!("{dir}/unsupported.json")).unwrap();
    assert_eq!(
        r1.jsonl, golden_jsonl,
        "{fixture}: jsonl drifted from committed golden"
    );
    // The audit embeds tool *versions* (metadata that legitimately changes per release), so the
    // golden comparison is on the SEMANTIC fields: the decode output and layout hashes. The decoded
    // bytes themselves are pinned by the exact `jsonl` comparison above; `decode_output_sha256` is
    // its hash. (`r1.audit_json == r2.audit_json` above already proves per-version byte-stability.)
    let sem = |s: &str| -> String {
        s.split(',')
            .filter(|f| {
                f.contains("decode_output_sha256")
                    || f.contains("layout_hash")
                    || f.contains("raw_input_sha256")
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    assert_eq!(
        sem(&r1.audit_json),
        sem(&golden_audit),
        "{fixture}: audit semantic hashes drifted from committed golden"
    );
    assert_eq!(
        r1.unsupported_json, golden_unsup,
        "{fixture}: unsupported drifted"
    );

    // 100+ records, no silent fallback (no unsupported in this sealed corpus).
    assert!(r1.record_count >= 100, "{fixture}: too few records");
    assert_eq!(
        r1.unsupported_count, 0,
        "{fixture}: unexpected unsupported fields"
    );
}

#[test]
fn corpus_is_byte_stable_and_golden() {
    check_family("account-status-v1", "recon/account", "ACCTREC.cpy", 55);
    check_family("payroll-v1", "recon/payroll", "PAYREC.cpy", 36);
    check_family("insurance-policy-v1", "recon/insurance", "INSREC.cpy", 41);
}

#[test]
fn cp500_family_composes_end_to_end() {
    // KOBOLD.DATA.3: the EBCDIC (cp500) account family decodes byte-stably, 0 unsupported.
    let dir = "recon/account-cp500";
    let cb = std::fs::read_to_string(format!("{dir}/ACCTCP5.cpy")).unwrap();
    let data = std::fs::read(format!("{dir}/input.ebc")).unwrap();
    let res = DirResolver(dir.into());
    let r = reconcile_encoded(
        "account-cp500",
        &cb,
        &data,
        55,
        "0.5.0",
        &res,
        Encoding::Cp500,
    )
    .unwrap();
    let r2 = reconcile_encoded(
        "account-cp500",
        &cb,
        &data,
        55,
        "0.5.0",
        &res,
        Encoding::Cp500,
    )
    .unwrap();
    assert_eq!(r.jsonl, r2.jsonl, "cp500 jsonl not byte-stable");
    assert_eq!(
        r.jsonl,
        std::fs::read_to_string(format!("{dir}/expected.jsonl")).unwrap(),
        "cp500 golden drift"
    );
    assert_eq!(
        r.unsupported_count, 0,
        "admitted cp500 fixture has no unsupported fields"
    );
    assert!(
        r.audit_json.contains("\"record_default\":\"cp500\""),
        "audit records the encoding"
    );
    assert!(r.audit_json.contains("\"binary_fields_passthrough\":true"));
    let first = r.jsonl.lines().next().unwrap();
    // EBCDIC text decoded + 88 evaluated on the decoded value:
    assert!(
        first.contains("\"ACCOUNT-ID\":\"100000\""),
        "EBCDIC text decoded"
    );
    assert!(first.contains("\"ACTIVE\":true") && first.contains("\"CUST-GOLD\":true"));
    // KOBOLD.DATA.5: cp500 NUMERIC DISPLAY (zoned) decoded via GNURUST.17 + audit names the court.
    assert!(
        first.contains("\"REGION-CODE\":")
            && first.contains("\"LIMIT-AMT\":")
            && first.contains("\"RISK-PERCENT\":"),
        "cp500 numeric DISPLAY decoded: {first}"
    );
    assert!(
        r.audit_json.contains("\"zoned_sign\":\"GNURUST.17\""),
        "audit names the GNURUST.17 numeric court"
    );
}

#[test]
fn ebcdic_never_touches_binary_or_packed() {
    // The crucial invariant: BINARY/PACKED fields are RAW storage — their decoded value is IDENTICAL
    // under ASCII vs cp500 (passthrough). cp500 DISPLAY numerics (REGION-CODE/LIMIT-AMT/RISK-PERCENT)
    // are encoding-sensitive and legitimately DIFFER — that's GNURUST.17, not a passthrough violation.
    let dir = "recon/account-cp500";
    let cb = std::fs::read_to_string(format!("{dir}/ACCTCP5.cpy")).unwrap();
    let data = std::fs::read(format!("{dir}/input.ebc")).unwrap();
    let res = DirResolver(dir.into());
    let ascii =
        kobold_data_shim::decode_record_encoded(&cb, &data[..55], &res, Encoding::Ascii).unwrap();
    let cp500 =
        kobold_data_shim::decode_record_encoded(&cb, &data[..55], &res, Encoding::Cp500).unwrap();
    let passthrough = [
        "BALANCE",
        "BRANCH-NO",
        "RISK-SCORE",
        "INTERNAL-ID",
        "ACCT-SEQ-C6",
    ]; // COMP-3/COMP/COMP-X/COMP-5
    let mut passthrough_same = 0;
    for (a, e) in ascii.fields.iter().zip(cp500.fields.iter()) {
        if a.category == "numeric" {
            // No field's raw bytes are ever mutated by decoding.
            assert_eq!(a.raw_hex, e.raw_hex, "{}: raw bytes differ", a.name);
            if passthrough.contains(&a.name.as_str()) {
                assert_eq!(
                    a.value, e.value,
                    "{}: binary/packed value changed under EBCDIC!",
                    a.name
                );
                passthrough_same += 1;
            }
        }
    }
    assert!(
        passthrough_same >= 5,
        "expected >=5 binary/packed fields proven untouched, got {passthrough_same}"
    );
}

#[test]
fn numeric_display_under_cp500_decodes_via_gnurust17() {
    // KOBOLD.DATA.5: cp500 numeric DISPLAY now decodes (GNURUST.17), no longer fails closed.
    // F1 F2 F3 F4 = unsigned 1234; F1 F2 D3 = signed -123 (0xD zone).
    let res = kobold_data_shim::NoCopy;
    let cb = "       01 R.\n           05 AMT PIC 9(4).\n";
    let r = kobold_data_shim::decode_record_encoded(cb, b"\xF1\xF2\xF3\xF4", &res, Encoding::Cp500)
        .unwrap();
    let amt = r.fields.iter().find(|f| f.name == "AMT").unwrap();
    assert_eq!((amt.category, amt.value.as_str()), ("numeric", "1234"));
    let cb2 = "       01 R.\n           05 N PIC S9(3).\n";
    let neg = kobold_data_shim::decode_record_encoded(cb2, b"\xF1\xF2\xD3", &res, Encoding::Cp500)
        .unwrap();
    assert_eq!(
        neg.fields.iter().find(|f| f.name == "N").unwrap().value,
        "-123"
    );
}

#[test]
fn cp500_numeric_under_ascii_unchanged() {
    // The ASCII path is untouched: PIC 9(4) under ASCII still decodes the ASCII way (not EBCDIC).
    let cb = "       01 R.\n           05 AMT PIC 9(4).\n";
    let res = kobold_data_shim::NoCopy;
    let rec = kobold_data_shim::decode_record_encoded(cb, b"1234", &res, Encoding::Ascii).unwrap();
    let amt = rec.fields.iter().find(|f| f.name == "AMT").unwrap();
    assert_eq!((amt.category, amt.value.as_str()), ("numeric", "1234"));
}

#[test]
fn comp6_composes_in_corpus() {
    // KOBOLD.DATA.6: an unsigned COMP-6 field decodes to a decimal string; audit names GNURUST.18.
    let dir = "recon/account";
    let cb = std::fs::read_to_string(format!("{dir}/ACCTREC.cpy")).unwrap();
    let data = std::fs::read(format!("{dir}/input.dat")).unwrap();
    let r = reconcile(
        "account-status-v1",
        &cb,
        &data,
        55,
        "0.7.0",
        &DirResolver(dir.into()),
    )
    .unwrap();
    assert_eq!(r.unsupported_count, 0);
    let first = r.jsonl.lines().next().unwrap();
    assert!(
        first.contains("\"ACCOUNT-SEQUENCE\":\"10000000\""),
        "COMP-6 decoded: {first}"
    );
    assert!(
        r.audit_json.contains("\"comp6\":{\"claim\":\"GNURUST.18\""),
        "audit names GNURUST.18"
    );
    assert!(r
        .audit_json
        .contains("\"domain\":\"comp6-unsigned-packed\""));
}

#[test]
fn signed_comp6_fails_closed() {
    // GNURUST.18 learned GnuCOBOL converts `S9(n) COMP-6` to COMP-3 -> the shim refuses to treat a
    // signed COMP-6 as COMP-6; it is surfaced as unsupported, never silently decoded.
    let cb = "       01 R.\n           05 N PIC S9(4) COMP-6.\n";
    let res = kobold_data_shim::NoCopy;
    let rec =
        kobold_data_shim::decode_record_encoded(cb, b"\x12\x34", &res, Encoding::Ascii).unwrap();
    let n = rec.fields.iter().find(|f| f.name == "N").unwrap();
    assert_eq!(
        n.category, "unsupported",
        "signed COMP-6 must fail closed, not decode"
    );
    // Unsigned COMP-6 in the same shape DOES decode.
    let cb2 = "       01 R.\n           05 M PIC 9(4) COMP-6.\n";
    let dec =
        kobold_data_shim::decode_record_encoded(cb2, b"\x12\x34", &res, Encoding::Ascii).unwrap();
    assert_eq!(
        dec.fields.iter().find(|f| f.name == "M").unwrap().value,
        "1234"
    );
}

#[test]
fn edited_picture_composes_end_to_end() {
    // KOBOLD.DATA.4: an edited DISPLAY field decodes; JSON keeps the presentation string, the audit
    // carries the oracle-proven numeric interpretation. 0 unsupported.
    let dir = "recon/account";
    let cb = std::fs::read_to_string(format!("{dir}/ACCTREC.cpy")).unwrap();
    let data = std::fs::read(format!("{dir}/input.dat")).unwrap();
    let r = reconcile(
        "account-status-v1",
        &cb,
        &data,
        55,
        "0.6.2",
        &DirResolver(dir.into()),
    )
    .unwrap();
    assert_eq!(
        r.unsupported_count, 0,
        "edited fields must decode, not fail closed"
    );
    let first = r.jsonl.lines().next().unwrap();
    assert!(
        first.contains("\"PRINT-BAL\":\"13,448.49\""),
        "edited presentation in fields: {first}"
    );
    assert!(
        first.contains("\"edited\":{\"PRINT-BAL\":{\"raw_text\":\"13,448.49\",\"numeric_value\":\"13448.49\",\"claim\":\"GNURUST.16\""),
        "edited audit block: {first}"
    );
    let rec = kobold_data_shim::decode_record_encoded(
        &cb,
        &data[..51],
        &DirResolver(dir.into()),
        Encoding::Ascii,
    )
    .unwrap();
    let pb = rec.fields.iter().find(|f| f.name == "PRINT-BAL").unwrap();
    assert_eq!(pb.category, "edited");
    assert_eq!(pb.value, "13,448.49");
    assert_eq!(pb.edited_numeric.as_deref(), Some("13448.49"));
}

#[test]
fn edited_negatives_fail_closed() {
    let res = kobold_data_shim::NoCopy;
    // edited under cp500 (the decode table is ASCII) → unsupported, not mis-decoded.
    let cb = "       01 R.\n           05 E PIC ZZ9.99.\n";
    let rec =
        kobold_data_shim::decode_record_encoded(cb, b" 12.34", &res, Encoding::Cp500).unwrap();
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "E").unwrap().category,
        "unsupported"
    );
    // an unsupported edited symbol → never the edited domain.
    let cb3 = "       01 R.\n           05 E PIC ZZ%9.\n";
    if let Ok(r) = kobold_data_shim::decode_record_encoded(cb3, b" %5", &res, Encoding::Ascii) {
        assert_ne!(
            r.fields.iter().find(|f| f.name == "E").map(|f| f.category),
            Some("edited")
        );
    }
}

#[test]
fn conditions_come_from_eval_88_only() {
    // Spot-check: the account corpus emits ACTIVE/CLOSED/DELINQUENT/CUST-GOLD, all from eval_88.
    let cb = std::fs::read_to_string("recon/account/ACCTREC.cpy").unwrap();
    let data = std::fs::read("recon/account/input.dat").unwrap();
    let r = reconcile(
        "account-status-v1",
        &cb,
        &data,
        51,
        "0.4.1",
        &DirResolver("recon/account".into()),
    )
    .unwrap();
    let first = r.jsonl.lines().next().unwrap();
    assert!(
        first.contains("\"ACTIVE\":true"),
        "record 0 should be ACTIVE"
    );
    assert!(first.contains("\"CLOSED\":false"));
    assert!(first.contains("\"CUST-GOLD\":true")); // proves COPY ... REPLACING expanded the 88
}

#[test]
fn condition_set_round_trips() {
    // The mutation fixture: SET TO TRUE -> bytes -> eval_88 true (condition -> bytes -> predicate).
    let cases: Vec<(&str, bool, Vec<CondValue>)> = vec![
        (
            "X(3)",
            false,
            vec![CondValue::Lit(CondLit::Alpha("A".into()))],
        ),
        (
            "9",
            false,
            vec![CondValue::Range(
                CondLit::Num("1".into()),
                CondLit::Num("3".into()),
            )],
        ),
        (
            "S9(3)",
            true,
            vec![CondValue::Range(
                CondLit::Num("1".into()),
                CondLit::Num("5".into()),
            )],
        ),
    ];
    for (pic, comp3, values) in cases {
        let usage = if comp3 { Usage::Comp3 } else { Usage::Display };
        let pf = build_field(pic, usage, false, false).unwrap();
        let cond = Condition {
            name: "C".into(),
            values,
        };
        let bytes = set_88_true(&pf.attr, pf.size, &cond).unwrap();
        assert_eq!(
            eval_88(&pf.attr, &bytes, &cond),
            Ok(true),
            "round-trip failed for {pic}"
        );
    }
}
