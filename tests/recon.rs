//! KOBOLD.RECON.1 acceptance: the committed corpus reconciles to byte-stable JSONL + audit, replay
//! is identical, the CLI and library paths agree, and the condition-set fixture round-trips through
//! `eval_88`. The committed `recon/*/expected.jsonl` etc. are the golden outputs (drift = test fail).

use gnucobol_rs::{build_field, eval_88, set_88_true, CondLit, CondValue, Condition, Usage};
use kobold_data_shim::recon::reconcile;
use kobold_data_shim::CopyResolver;

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
