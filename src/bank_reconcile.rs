//! KOBOLD.BANK.RECONCILE.1 — opinionated generated operator view over existing evidence.
//!
//! **Doctrine.** BANK.RECONCILE.1 is an opinionated generated audit *view* over existing KOBOLD banking
//! custody, accounting-profile, DB2-indicator, privacy, and casefile evidence; it helps operators *read*
//! the evidence but does **not** create posting truth, ledger acceptance, settlement finality,
//! account-balance truth, or business approval. It introduces **no new evidence** — every number is taken
//! from a court result struct (`BANK.1/2`, `DB2HOST.1`, `POSTING.1`, `PRIVACY.REDACTION.1`), never
//! recomputed in a way that could drift from the casefile.

use crate::banking::BankingResult;
use crate::db2host::Db2HostResult;
use crate::posting::{PostingManifest, PostingProfile};

/// The existing-evidence inputs the view summarizes. Each is a court output, not a fresh computation.
pub struct BankReconcileInputs<'a> {
    pub batch: &'a PostingProfile<'a>,
    pub custody: &'a PostingManifest,
    pub banking: &'a BankingResult,
    pub db2: Option<&'a Db2HostResult>,
    pub redacted_field_count: usize,
    pub tokenized_field_count: usize,
    pub dirty_count: usize,
    pub unsupported_count: usize,
}

/// The generated operator report (json + markdown + SARIF), all from the inputs above.
pub struct BankReconcileReport {
    pub report_json: String,
    pub report_md: String,
    pub sarif_json: String,
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
fn cents(c: i64) -> String {
    format!(
        "{}{}.{:02}",
        if c < 0 { "-" } else { "" },
        c.abs() / 100,
        c.abs() % 100
    )
}
fn opt_cents(c: Option<i64>) -> String {
    c.map(|v| jstr(&cents(v))).unwrap_or_else(|| "null".into())
}
fn opt_u(v: Option<u64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "null".into())
}
fn arr_u(v: &[u64]) -> String {
    v.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
fn arr_s(v: &[String]) -> String {
    v.iter().map(|s| jstr(s)).collect::<Vec<_>>().join(",")
}

/// Build the operator reconciliation view from existing court evidence.
pub fn bank_reconcile_report(i: &BankReconcileInputs) -> BankReconcileReport {
    let s = &i.banking.summary;
    let verdict = if i.banking.balanced {
        "matched"
    } else {
        "mismatch"
    };
    let (null_n, trunc_n, dirty_ind) = match i.db2 {
        Some(d) => (
            d.states.iter().filter(|s| s.semantic_null).count(),
            d.states.iter().filter(|s| s.truncation_evidence).count(),
            d.findings.len(),
        ),
        None => (0, 0, 0),
    };

    // Aggregate the EXISTING findings (banking + db2 + custody) into one SARIF view — no new findings.
    let mut sarif_results: Vec<String> = Vec::new();
    let mut push = |rule: &str, msg: &str, level: &str| {
        sarif_results.push(format!(
            "{{\"ruleId\":{},\"level\":{},\"message\":{{\"text\":{}}}}}",
            jstr(rule),
            jstr(level),
            jstr(msg)
        ));
    };
    for (r, m) in &i.banking.findings {
        push(r, m, "error");
    }
    if let Some(d) = i.db2 {
        for (r, m) in &d.findings {
            push(r, m, "warning");
        }
    }
    for (r, m) in &i.custody.findings {
        push(r, m, "warning");
    }
    let sarif_json = format!(
        concat!(
            "{{\"version\":\"2.1.0\",\"$schema\":\"https://json.schemastore.org/sarif-2.1.0.json\",",
            "\"runs\":[{{\"tool\":{{\"driver\":{{\"name\":\"kobold-bank-reconcile\",\"informationUri\":\"https://github.com/infinityabundance/kobold-data-shim\",\"rules\":[]}}}},",
            "\"results\":[{}]}}]}}\n"
        ),
        sarif_results.join(",")
    );

    let report_json = format!(
        concat!(
            "{{\"schema\":\"kobold-bank-reconcile-report-v1\",\"court\":\"KOBOLD.BANK.RECONCILE.1\",",
            "\"view_only\":true,\"introduces_new_evidence\":false,",
            "\"batch\":{{\"posting_unit_id\":{},\"business_date\":{},\"extract_time_utc\":{},\"source_system\":{},\"file_hash\":{},\"last_chain_hash\":{}}},",
            "\"controls\":{{\"declared_count\":{},\"observed_count\":{},\"declared_debit\":{},\"observed_debit\":{},\"declared_credit\":{},\"observed_credit\":{},\"verdict\":{}}},",
            "\"custody\":{{\"sequence_min\":{},\"sequence_max\":{},\"sequence_gaps\":[{}],\"duplicate_sequences\":[{}],\"duplicate_transaction_ids\":[{}]}},",
            "\"db2\":{{\"semantic_null_count\":{},\"truncation_evidence_count\":{},\"dirty_indicator_count\":{}}},",
            "\"findings\":{{\"dirty_count\":{},\"unsupported_count\":{},\"unknown_record_type_count\":{},\"unknown_polarity_count\":{}}},",
            "\"privacy\":{{\"redacted_field_count\":{},\"tokenized_field_count\":{},\"public_output_claim\":false}},",
            "\"truth_layers\":{{\"record_truth\":true,\"posting_truth\":false,\"ledger_truth\":false,\"settlement_truth\":false,\"business_truth\":false}},",
            "\"negative_capabilities\":[\"NEG.BANK_RECONCILE.NOT_LEDGER_ACCEPTANCE\",\"NEG.BANK_RECONCILE.NOT_SETTLEMENT_FINALITY\",",
            "\"NEG.BANK_RECONCILE.NOT_ACCOUNT_BALANCE_TRUTH\",\"NEG.BANK_RECONCILE.NOT_BUSINESS_APPROVAL\",",
            "\"NEG.BANK_RECONCILE.VIEW_NOT_NEW_EVIDENCE\",\"NEG.BANK_RECONCILE.MATCH_NOT_CORRECTNESS\"]}}\n"
        ),
        jstr(i.batch.posting_unit_id), jstr(i.batch.business_date), jstr(i.batch.extract_time_utc),
        jstr(i.batch.source_system), jstr(&i.custody.file_hash), jstr(&i.custody.last_chain_hash),
        opt_u(s.declared_count), s.observed_count, opt_cents(s.declared_debit_cents), jstr(&cents(s.observed_debit_cents)),
        opt_cents(s.declared_credit_cents), jstr(&cents(s.observed_credit_cents)), jstr(verdict),
        opt_u(i.custody.seq_min), opt_u(i.custody.seq_max), arr_u(&i.custody.seq_gaps), arr_u(&i.custody.seq_duplicates), arr_s(&i.custody.txn_duplicates),
        null_n, trunc_n, dirty_ind,
        i.dirty_count, i.unsupported_count, s.unknown_record_type_count, s.unknown_polarity_count,
        i.redacted_field_count, i.tokenized_field_count,
    );

    let report_md = format!(
        concat!(
            "# Banking reconciliation view — {}\n\n",
            "> Generated operator **view** over existing KOBOLD evidence (BANK.1/2 · DB2HOST.1 · POSTING.1 · PRIVACY.REDACTION.1). ",
            "It summarizes; it does **not** create posting, ledger, settlement, account-balance, or business truth.\n\n",
            "- business date: {} · source: {} · extract: {}\n",
            "- file_hash: `{}`\n- last_chain_hash: `{}`\n\n",
            "## Controls — **{}**\n\n",
            "| | declared | observed |\n|---|---|---|\n| count | {} | {} |\n| debit | {} | {} |\n| credit | {} | {} |\n\n",
            "## Custody\n- sequence: {}…{}  · gaps: {}  · dup sequences: {}  · dup txn ids: {}\n\n",
            "## DB2 indicators\n- semantic null: {} · truncation: {} · dirty/missing: {}\n\n",
            "## Findings\n- dirty: {} · unsupported: {} · unknown record type: {} · unknown polarity: {}\n\n",
            "## Privacy\n- redacted: {} · tokenized: {} · public_output_claim: false\n\n",
            "## Truth layers\n- record_truth: ✓  ·  posting/ledger/settlement/business truth: **refused** (claimed:false)\n"
        ),
        i.batch.posting_unit_id, i.batch.business_date, i.batch.source_system, i.batch.extract_time_utc,
        i.custody.file_hash, i.custody.last_chain_hash,
        verdict,
        opt_u(s.declared_count), s.observed_count,
        opt_cents(s.declared_debit_cents).replace('"', ""), cents(s.observed_debit_cents),
        opt_cents(s.declared_credit_cents).replace('"', ""), cents(s.observed_credit_cents),
        opt_u(i.custody.seq_min), opt_u(i.custody.seq_max),
        if i.custody.seq_gaps.is_empty() { "none".into() } else { arr_u(&i.custody.seq_gaps) },
        if i.custody.seq_duplicates.is_empty() { "none".into() } else { arr_u(&i.custody.seq_duplicates) },
        if i.custody.txn_duplicates.is_empty() { "none".into() } else { i.custody.txn_duplicates.join(",") },
        null_n, trunc_n, dirty_ind,
        i.dirty_count, i.unsupported_count, s.unknown_record_type_count, s.unknown_polarity_count,
        i.redacted_field_count, i.tokenized_field_count,
    );

    BankReconcileReport {
        report_json,
        report_md,
        sarif_json,
    }
}
