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
    check_family("account-status-v1", "recon/account", "ACCTREC.cpy", 42);
    check_family("payroll-v1", "recon/payroll", "PAYREC.cpy", 25);
    check_family("insurance-policy-v1", "recon/insurance", "INSREC.cpy", 26);
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
        34,
        "0.5.0",
        &res,
        Encoding::Cp500,
    )
    .unwrap();
    let r2 = reconcile_encoded(
        "account-cp500",
        &cb,
        &data,
        34,
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
}

#[test]
fn ebcdic_never_touches_binary_or_packed() {
    // The most important negative: decoding the SAME bytes as ASCII vs cp500 changes ONLY the
    // alphanumeric (text) fields; every COMP/COMP-3/COMP-5/COMP-X value is identical (raw passthrough).
    let dir = "recon/account-cp500";
    let cb = std::fs::read_to_string(format!("{dir}/ACCTCP5.cpy")).unwrap();
    let data = std::fs::read(format!("{dir}/input.ebc")).unwrap();
    let res = DirResolver(dir.into());
    let ascii =
        kobold_data_shim::decode_record_encoded(&cb, &data[..34], &res, Encoding::Ascii).unwrap();
    let cp500 =
        kobold_data_shim::decode_record_encoded(&cb, &data[..34], &res, Encoding::Cp500).unwrap();
    let mut numeric_same = 0;
    let mut alpha_diff = 0;
    for (a, e) in ascii.fields.iter().zip(cp500.fields.iter()) {
        match a.category {
            "numeric" => {
                assert_eq!(
                    a.value, e.value,
                    "{}: COMP/packed value changed under EBCDIC!",
                    a.name
                );
                assert_eq!(
                    a.raw_hex, e.raw_hex,
                    "{}: raw bytes differ (should be identical)",
                    a.name
                );
                numeric_same += 1;
            }
            "alphanumeric" if a.value != e.value => alpha_diff += 1,
            _ => {}
        }
    }
    assert!(
        numeric_same >= 4,
        "expected >=4 packed/binary fields proven untouched, got {numeric_same}"
    );
    assert!(
        alpha_diff >= 1,
        "encoding should change at least one text field (else test is vacuous)"
    );
}

#[test]
fn numeric_display_under_cp500_fails_closed() {
    // EBCDIC zoned numeric (sign mode) is deferred (GNURUST.15 admits only text) -> fail closed.
    let cb = "       01 R.\n           05 AMT PIC 9(4).\n";
    let res = kobold_data_shim::NoCopy;
    let rec =
        kobold_data_shim::decode_record_encoded(cb, b"\xF1\xF2\xF3\xF4", &res, Encoding::Cp500)
            .unwrap();
    let amt = rec.fields.iter().find(|f| f.name == "AMT").unwrap();
    assert_eq!(
        amt.category, "unsupported",
        "numeric DISPLAY under cp500 must fail closed, not mis-decode"
    );
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
        42,
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
