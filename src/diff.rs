//! KOBOLD.DIFF.1 — structural comparison against a *declared* expected artifact.
//!
//! **Doctrine.** KOBOLD.DIFF.1 admits only structural comparison against declared expected artifacts: a
//! match proves equality to the declared target under the selected comparison rules, **not** business
//! truth, ledger acceptance, settlement finality, customer approval, or oracle authority. The comparison
//! target is **never** called an oracle unless its declared `oracle_status` permits it — a diff lets a
//! bank compare KOBOLD output to a declared artifact without smuggling in *"therefore the old system was
//! correct."*

/// The declared authority of the comparison target. Default (and safest) is `NotOracle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OracleStatus {
    NotOracle,
    DeclaredBaseline,
    CustomerSuppliedReference,
    SystemOfRecordExportUnvalidated,
    AdmittedOracle,
}
impl OracleStatus {
    fn as_str(self) -> &'static str {
        match self {
            OracleStatus::NotOracle => "not_oracle",
            OracleStatus::DeclaredBaseline => "declared_baseline",
            OracleStatus::CustomerSuppliedReference => "customer_supplied_reference",
            OracleStatus::SystemOfRecordExportUnvalidated => "system_of_record_export_unvalidated",
            OracleStatus::AdmittedOracle => "admitted_oracle",
        }
    }
}

/// A declared comparison target. `allowed_comparisons` gates which dimensions are compared.
pub struct DiffTarget<'a> {
    pub target_kind: &'a str,
    pub oracle_status: OracleStatus,
    /// e.g. `"test-golden" | "customer-supplied" | "system-export" | "previous-run"`.
    pub source: &'a str,
    /// any of `"field_values" | "audit_hashes" | "finding_ids" | "control_totals"`.
    pub allowed_comparisons: &'a [&'a str],
}

/// One side of a comparison, extracted from a KOBOLD output (actual) or a declared artifact (expected).
#[derive(Default)]
pub struct DiffInput {
    pub fields: Vec<(String, String)>,
    pub finding_ids: Vec<String>,
    pub control_totals: Vec<(String, String)>,
    pub output_hash: Option<String>,
}

/// The deterministic diff result.
pub struct DiffReport {
    pub report_json: String,
    pub sarif_json: String,
    pub casefile_json: String,
    pub findings: Vec<(String, String)>,
    pub matched: bool,
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

/// Compare a KOBOLD output (`actual`) against a declared expected artifact (`expected`) under `target`.
/// Deterministic: findings are sorted; only the `allowed_comparisons` dimensions are checked.
pub fn diff_artifacts(actual: &DiffInput, expected: &DiffInput, target: &DiffTarget) -> DiffReport {
    let allowed = |k: &str| target.allowed_comparisons.contains(&k);
    let mut findings: Vec<(String, String)> = Vec::new();

    if allowed("field_values") {
        let amap: std::collections::HashMap<&str, &str> = actual
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let emap: std::collections::HashMap<&str, &str> = expected
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        for (k, ev) in &expected.fields {
            match amap.get(k.as_str()) {
                None => findings.push((
                    "KOBOLD-DIFF-MISSING-FIELD".into(),
                    format!("field {k:?} present in expected, absent in actual"),
                )),
                Some(av) if av != ev => findings.push((
                    "KOBOLD-DIFF-FIELD-MISMATCH".into(),
                    format!("field {k:?}: actual {av:?} != expected {ev:?}"),
                )),
                _ => {}
            }
        }
        for (k, _) in &actual.fields {
            if !emap.contains_key(k.as_str()) {
                findings.push((
                    "KOBOLD-DIFF-EXTRA-FIELD".into(),
                    format!("field {k:?} present in actual, not in expected"),
                ));
            }
        }
    }
    if allowed("finding_ids") {
        let aset: std::collections::BTreeSet<&str> =
            actual.finding_ids.iter().map(String::as_str).collect();
        let eset: std::collections::BTreeSet<&str> =
            expected.finding_ids.iter().map(String::as_str).collect();
        if aset != eset {
            findings.push((
                "KOBOLD-DIFF-FINDING-SET-MISMATCH".into(),
                format!("finding-id set differs: actual {aset:?} != expected {eset:?}"),
            ));
        }
    }
    if allowed("control_totals") {
        let emap: std::collections::HashMap<&str, &str> = expected
            .control_totals
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let amap: std::collections::HashMap<&str, &str> = actual
            .control_totals
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        for (k, ev) in &expected.control_totals {
            if amap.get(k.as_str()) != Some(&ev.as_str()) {
                findings.push((
                    "KOBOLD-DIFF-CONTROL-TOTAL-MISMATCH".into(),
                    format!(
                        "control total {k:?}: actual {:?} != expected {ev:?}",
                        amap.get(k.as_str())
                    ),
                ));
            }
        }
        for (k, _) in &actual.control_totals {
            if !emap.contains_key(k.as_str()) {
                findings.push((
                    "KOBOLD-DIFF-CONTROL-TOTAL-MISMATCH".into(),
                    format!("control total {k:?} present in actual, not in expected"),
                ));
            }
        }
    }
    if allowed("audit_hashes") && actual.output_hash != expected.output_hash {
        findings.push((
            "KOBOLD-DIFF-HASH-MISMATCH".into(),
            format!(
                "audit output hash differs: actual {:?} != expected {:?}",
                actual.output_hash, expected.output_hash
            ),
        ));
    }

    let matched = findings.is_empty();
    // A non-oracle target that MATCHES must NOT be read as truth — emit a loud refusal note so a match is
    // never smuggled into 'the old system was correct'.
    if matched && target.oracle_status != OracleStatus::AdmittedOracle {
        findings.push(("KOBOLD-DIFF-TARGET-NOT-ORACLE".into(),
            format!("matched the declared target (oracle_status={}); this proves equality to the target, NOT correctness/oracle authority", target.oracle_status.as_str())));
    }

    findings.sort();
    let oracle_authority = target.oracle_status == OracleStatus::AdmittedOracle;
    let verdict = if matched {
        "equal_to_declared_target"
    } else {
        "differs_from_declared_target"
    };

    let sarif_results: Vec<String> = findings
        .iter()
        .map(|(r, m)| {
            let level = if r == "KOBOLD-DIFF-TARGET-NOT-ORACLE" {
                "note"
            } else {
                "error"
            };
            format!(
                "{{\"ruleId\":{},\"level\":{},\"message\":{{\"text\":{}}}}}",
                jstr(r),
                jstr(level),
                jstr(m)
            )
        })
        .collect();
    let sarif_json = format!(
        concat!(
            "{{\"version\":\"2.1.0\",\"$schema\":\"https://json.schemastore.org/sarif-2.1.0.json\",",
            "\"runs\":[{{\"tool\":{{\"driver\":{{\"name\":\"kobold-diff\",\"rules\":[]}}}},\"results\":[{}]}}]}}\n"
        ),
        sarif_results.join(",")
    );

    let allowed_json = target
        .allowed_comparisons
        .iter()
        .map(|c| jstr(c))
        .collect::<Vec<_>>()
        .join(",");
    let find_json = findings
        .iter()
        .map(|(r, m)| format!("{{\"ruleId\":{},\"message\":{}}}", jstr(r), jstr(m)))
        .collect::<Vec<_>>()
        .join(",");
    let report_json = format!(
        concat!(
            "{{\"schema\":\"kobold-diff-report-v1\",\"court\":\"KOBOLD.DIFF.1\",\"verdict\":{},\"matched\":{},",
            "\"target\":{{\"schema\":\"kobold-diff-target-v1\",\"target_kind\":{},\"oracle_status\":{},\"source\":{},",
            "\"allowed_comparisons\":[{}]}},\"oracle_authority_claimed\":{},",
            "\"truth_layers\":{{\"equal_to_declared_target\":{},\"business_truth\":false,\"ledger_truth\":false,",
            "\"settlement_truth\":false,\"customer_approval\":false}},",
            "\"findings\":[{}],",
            "\"negative_capabilities\":[\"NEG.DIFF.MATCH_NOT_BUSINESS_TRUTH\",\"NEG.DIFF.EXPECTED_OUTPUT_NOT_ORACLE\",",
            "\"NEG.DIFF.SYSTEM_OF_RECORD_NOT_VALIDATED\",\"NEG.DIFF.NO_LEDGER_ACCEPTANCE\",",
            "\"NEG.DIFF.NO_SETTLEMENT_FINALITY\",\"NEG.DIFF.NO_CUSTOMER_APPROVAL\"]}}\n"
        ),
        jstr(verdict), matched,
        jstr(target.target_kind), jstr(target.oracle_status.as_str()), jstr(target.source),
        allowed_json, oracle_authority,
        matched, find_json,
    );
    let casefile_json = report_json.replace(
        "\"schema\":\"kobold-diff-report-v1\"",
        "\"schema\":\"kobold-diff-forensic-casefile-v1\"",
    );

    DiffReport {
        report_json,
        sarif_json,
        casefile_json,
        findings,
        matched,
    }
}
