//! Generate the `condition-set` mutation fixture for KOBOLD.RECON.1: for several (parent,
//! condition) pairs, run `set_88_true` to construct the parent bytes, confirm `eval_88` is true,
//! and write the input spec, the produced bytes, and an audit. Proves: condition name → parent
//! bytes → decoded condition true.

use gnucobol_rs::{build_field, eval_88, set_88_true, CondLit, CondValue, Condition, Usage};
use kobold_data_shim::sha256::sha256_hex;

fn jstr(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn main() {
    // (label, parent pic, comp3?, condition name, values)
    let cases: Vec<(&str, &str, bool, &str, Vec<CondValue>)> = vec![
        (
            "account-active",
            "X(3)",
            false,
            "ACTIVE",
            vec![CondValue::Lit(CondLit::Alpha("A".into()))],
        ),
        (
            "account-tier",
            "X",
            false,
            "GOLD",
            vec![CondValue::Lit(CondLit::Alpha("G".into()))],
        ),
        (
            "insurance-low-risk",
            "9",
            false,
            "LOW-RISK",
            vec![CondValue::Range(
                CondLit::Num("1".into()),
                CondLit::Num("3".into()),
            )],
        ),
        (
            "payroll-salaried",
            "X",
            false,
            "SALARIED",
            vec![
                CondValue::Lit(CondLit::Alpha("S".into())),
                CondValue::Lit(CondLit::Alpha("M".into())),
            ],
        ),
        (
            "amount-comp3",
            "S9(3)",
            true,
            "SMALL",
            vec![CondValue::Range(
                CondLit::Num("1".into()),
                CondLit::Num("5".into()),
            )],
        ),
    ];

    let mut input = String::from("{\"schema\":\"kobold-condition-set-v1\",\"cases\":[");
    let mut audit = String::from("{\"schema\":\"kobold-condition-set-audit-v1\",\"cases\":[");
    let mut out_bytes = Vec::new();

    for (i, (label, pic, comp3, name, values)) in cases.iter().enumerate() {
        let usage = if *comp3 { Usage::Comp3 } else { Usage::Display };
        let pf = build_field(pic, usage, false, false).expect("pic");
        let cond = Condition {
            name: name.to_string(),
            values: values.clone(),
        };
        let bytes = set_88_true(&pf.attr, pf.size, &cond).expect("set_88_true");
        let ok = eval_88(&pf.attr, &bytes, &cond) == Ok(true);
        assert!(ok, "round-trip self-check failed for {label}");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        out_bytes.extend_from_slice(&bytes);

        if i > 0 {
            input.push(',');
            audit.push(',');
        }
        input.push_str(&format!(
            "{{\"label\":{},\"parent_pic\":{},\"usage\":{},\"condition\":{},\"set\":\"TO TRUE\"}}",
            jstr(label),
            jstr(pic),
            jstr(if *comp3 { "COMP-3" } else { "DISPLAY" }),
            jstr(name)
        ));
        audit.push_str(&format!(
            "{{\"label\":{},\"output_hex\":{},\"output_sha256\":{},\"eval_88_true\":{}}}",
            jstr(label),
            jstr(&hex),
            jstr(&sha256_hex(&bytes)),
            ok
        ));
    }
    input.push_str("]}\n");
    audit.push_str("]}\n");

    std::fs::create_dir_all("recon/condition-set").unwrap();
    std::fs::write("recon/condition-set/condition-set-input.json", input).unwrap();
    std::fs::write("recon/condition-set/condition-set-output.dat", &out_bytes).unwrap();
    std::fs::write("recon/condition-set/condition-set-audit.json", audit).unwrap();
    eprintln!(
        "wrote condition-set fixture ({} cases, all round-trip true)",
        cases.len()
    );
}
