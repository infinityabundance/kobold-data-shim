//! KOBOLD.DIFF.1 acceptance: structural comparison against a DECLARED expected artifact. Exact match
//! passes; field/missing/extra/finding-set/control-total/hash drifts each produce a named finding; the
//! target is never called an oracle unless oracle_status permits; truth layers above equality are refused.

use kobold_data_shim::{diff_artifacts, DiffInput, DiffTarget, OracleStatus};

const CMP: &[&str] = &[
    "field_values",
    "audit_hashes",
    "finding_ids",
    "control_totals",
];
fn target(status: OracleStatus) -> DiffTarget<'static> {
    DiffTarget {
        target_kind: "declared_expected_artifact",
        oracle_status: status,
        source: "test-golden",
        allowed_comparisons: CMP,
    }
}
fn input(fields: &[(&str, &str)]) -> DiffInput {
    DiffInput {
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        finding_ids: vec![],
        control_totals: vec![],
        output_hash: Some("h0".into()),
    }
}
fn has(r: &kobold_data_shim::DiffReport, rule: &str) -> bool {
    r.findings.iter().any(|(x, _)| x == rule)
}

#[test]
fn identical_passes_but_marks_target_not_oracle() {
    let a = input(&[("AMOUNT", "100.00"), ("STATUS", "A")]);
    let e = input(&[("AMOUNT", "100.00"), ("STATUS", "A")]);
    let r = diff_artifacts(&a, &e, &target(OracleStatus::NotOracle));
    assert!(r.matched, "exact structural match");
    assert!(r
        .report_json
        .contains("\"verdict\":\"equal_to_declared_target\""));
    // a non-oracle match must NOT be smuggled into truth
    assert!(has(&r, "KOBOLD-DIFF-TARGET-NOT-ORACLE"));
    assert!(r.report_json.contains("\"oracle_authority_claimed\":false"));
    assert!(
        r.report_json.contains("\"business_truth\":false")
            && r.report_json.contains("\"ledger_truth\":false")
    );
    assert!(r.report_json.contains("NEG.DIFF.MATCH_NOT_BUSINESS_TRUTH"));
}

#[test]
fn field_mismatch_missing_extra() {
    let a = input(&[("AMOUNT", "150.00"), ("EXTRA", "x")]); // AMOUNT changed, EXTRA added, STATUS missing
    let e = input(&[("AMOUNT", "100.00"), ("STATUS", "A")]);
    let r = diff_artifacts(&a, &e, &target(OracleStatus::NotOracle));
    assert!(!r.matched);
    assert!(has(&r, "KOBOLD-DIFF-FIELD-MISMATCH"));
    assert!(has(&r, "KOBOLD-DIFF-MISSING-FIELD"));
    assert!(has(&r, "KOBOLD-DIFF-EXTRA-FIELD"));
    assert!(r.sarif_json.contains("KOBOLD-DIFF-FIELD-MISMATCH"));
}

#[test]
fn finding_set_and_control_total_and_hash_drift() {
    let a = DiffInput {
        finding_ids: vec!["KOBOLD-BANK-CONTROL-MISMATCH".into()],
        control_totals: vec![("debit".into(), "360.50".into())],
        output_hash: Some("HASH-A".into()),
        ..Default::default()
    };
    let e = DiffInput {
        finding_ids: vec![],
        control_totals: vec![("debit".into(), "999.99".into())],
        output_hash: Some("HASH-E".into()),
        ..Default::default()
    };
    let r = diff_artifacts(&a, &e, &target(OracleStatus::DeclaredBaseline));
    assert!(has(&r, "KOBOLD-DIFF-FINDING-SET-MISMATCH"));
    assert!(has(&r, "KOBOLD-DIFF-CONTROL-TOTAL-MISMATCH"));
    assert!(has(&r, "KOBOLD-DIFF-HASH-MISMATCH"));
    assert!(!r.matched);
}

#[test]
fn admitted_oracle_match_claims_oracle_authority_no_refusal_note() {
    let a = input(&[("AMOUNT", "100.00")]);
    let e = input(&[("AMOUNT", "100.00")]);
    let r = diff_artifacts(&a, &e, &target(OracleStatus::AdmittedOracle));
    assert!(r.matched);
    assert!(!has(&r, "KOBOLD-DIFF-TARGET-NOT-ORACLE")); // permitted -> no refusal note
    assert!(r.report_json.contains("\"oracle_authority_claimed\":true"));
}

#[test]
fn deterministic_report() {
    let a = input(&[("B", "2"), ("A", "1")]);
    let e = input(&[("A", "1"), ("B", "9")]);
    let r1 = diff_artifacts(&a, &e, &target(OracleStatus::NotOracle));
    let r2 = diff_artifacts(&a, &e, &target(OracleStatus::NotOracle));
    assert_eq!(r1.report_json, r2.report_json);
    assert_eq!(r1.sarif_json, r2.sarif_json);
}

#[test]
fn allowed_comparisons_gate_which_dimensions_run() {
    // only field_values allowed -> a hash drift is NOT reported
    let t = DiffTarget {
        target_kind: "x",
        oracle_status: OracleStatus::NotOracle,
        source: "previous-run",
        allowed_comparisons: &["field_values"],
    };
    let mut a = input(&[("X", "1")]);
    a.output_hash = Some("HA".into());
    let mut e = input(&[("X", "1")]);
    e.output_hash = Some("HB".into());
    let r = diff_artifacts(&a, &e, &t);
    assert!(
        !has(&r, "KOBOLD-DIFF-HASH-MISMATCH"),
        "hash not in allowed_comparisons -> not checked"
    );
    assert!(r.matched);
}
