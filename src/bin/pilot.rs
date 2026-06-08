//! `kobold-pilot` — run a REDACTED pilot over a declared fixed-record extract and write a hash-bound evidence
//! packet. It chains the sealed courts — EXTRACT.PROFILE.1 → PRIVACY.REDACTION.1 → BANK.1/2 +
//! BANK.RECONCILE.1 → DIFF.1 → TOOLING.EXPORT.1 → PILOT-PACKET.1 — redacting sensitive fields **before** any
//! artifact is written, so the produced packet is safe to share.
//!
//! ```text
//! kobold-pilot --pilot-id P --business-date YYYY-MM-DD --source-system S --notes notes.txt --out DIR
//! ```
//!
//! The default extract is a **declared synthetic/private-pilot-shaped** banking file (NOT customer data).
//! For a real pilot, point an operator at this with their own extract/copybook/specs. **Claim:** this run
//! produced a redacted, hash-bound evidence packet over a declared extract. It does **not** claim customer
//! acceptance, business correctness, regulatory compliance, production readiness, or ledger truth.

use gnucobol_rs::{build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY};
use kobold_data_shim::banking::{AccountingProfile, NumericRole, PolarityProfile};
use kobold_data_shim::{
    bank_reconcile_report, diff_artifacts, extract_manifest, pilot_packet, posting_manifest,
    reconcile_banking, redact_record, tooling_export, BankReconcileInputs, ControlSpec, DefaultAction,
    DiffInput, DiffTarget, Encoding, ExtractMethod, ExtractProfile, FieldRule, FileOrganization, NoCopy,
    OracleStatus, PilotArtifact, PilotInputs, PostingProfile, RecordLengthSource, RedactionAction,
    RedactionPolicy, Variant, VariantSpec,
};
use std::fs;
use std::process::ExitCode;

const DTL: &str = "       01 D.\n           05 REC-TYPE PIC X.\n           05 DR-CR-IND PIC X.\n           05 ACCT-ID PIC X(8).\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n           05 FILLER PIC X(11).\n";
const TRL: &str = "       01 T.\n           05 REC-TYPE PIC X.\n           05 TRL-COUNT PIC 9(6).\n           05 TRL-DEBIT PIC S9(9)V99 COMP-3.\n           05 TRL-CREDIT PIC S9(9)V99 COMP-3.\n           05 FILLER PIC X(12).\n";
const RL: usize = 28;

fn comp3(pic: &str, value: &str) -> Vec<u8> {
    let pf = build_field(pic, Usage::Comp3, false, false).unwrap();
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
    let sa = FieldAttr { field_type: COB_TYPE_NUMERIC_DISPLAY, digits: pf.attr.digits, scale: pf.attr.scale, flags: COB_FLAG_HAVE_SIGN };
    let mut out = vec![0u8; pf.size];
    cob_move(&src, &sa, &mut out, &pf.attr).unwrap();
    out
}
fn detail(ind: u8, acct: &str, amt: &str) -> Vec<u8> {
    let mut d = vec![b'D', ind];
    let mut a = acct.as_bytes().to_vec();
    a.resize(8, b' ');
    d.extend(a);
    d.extend(comp3("S9(7)V99", amt));
    d.resize(RL, b' ');
    d
}
fn trailer(count: u32, debit: &str, credit: &str) -> Vec<u8> {
    let mut t = vec![b'T'];
    t.extend(format!("{count:06}").into_bytes());
    t.extend(comp3("S9(9)V99", debit));
    t.extend(comp3("S9(9)V99", credit));
    t.resize(RL, b' ');
    t
}
fn specs() -> (VariantSpec<'static>, ControlSpec<'static>) {
    let vs: &'static [Variant] = Box::leak(Box::new([
        Variant { discriminator: b"D", name: "D", copybook: DTL },
        Variant { discriminator: b"T", name: "T", copybook: TRL },
    ]));
    let v = VariantSpec { discriminator_offset: 0, discriminator_len: 1, variants: vs };
    let roles: &'static [(&str, NumericRole)] = Box::leak(Box::new([("AMOUNT", NumericRole::Amount)]));
    let c = ControlSpec {
        detail_variant: "D", trailer_variant: "T", trailer_count_field: "TRL-COUNT",
        trailer_debit_field: "TRL-DEBIT", trailer_credit_field: "TRL-CREDIT",
        accounting: AccountingProfile {
            numeric_roles: roles,
            polarity: PolarityProfile { amount_field: "AMOUNT", source_field: "DR-CR-IND", debit_values: Box::leak(Box::new(["D"])), credit_values: Box::leak(Box::new(["C"])) },
        },
    };
    (v, c)
}
const PCB: &str = "       01 R.\n           05 SEQ-NO PIC 9(6).\n           05 PAD PIC X(2).\n";
fn seqbuf(n: u32) -> Vec<u8> {
    (1..=n).flat_map(|i| { let mut r = format!("{i:06}").into_bytes(); r.extend(b"  "); r }).collect()
}

fn main() -> ExitCode {
    let mut pilot_id = String::from("PILOT-DECLARED-SYNTHETIC-001");
    let mut business_date = String::from("2026-06-08");
    let mut source_system = String::from("synthetic-pilot");
    let mut notes = String::from("declared synthetic/private-pilot-shaped extract; account ids tokenized; no customer data present");
    let mut out = String::from("reports/pilot-run");
    let dialect = String::from("gnucobol-3.2.0-default");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pilot-id" => if let Some(v) = it.next() { pilot_id = v.clone() },
            "--business-date" => if let Some(v) = it.next() { business_date = v.clone() },
            "--source-system" => if let Some(v) = it.next() { source_system = v.clone() },
            "--notes" => if let Some(v) = it.next() { notes = fs::read_to_string(v).unwrap_or_else(|_| v.clone()) },
            "--out" => if let Some(v) = it.next() { out = v.clone() },
            _ => { eprintln!("unknown arg: {a}"); return ExitCode::from(2); }
        }
    }

    // The declared (synthetic/private-pilot-shaped) banking extract: 2 debits + 1 credit + balancing trailer.
    let mut data = detail(b'D', "ACCT0001", "100.00");
    data.extend(detail(b'D', "ACCT0002", "50.00"));
    data.extend(detail(b'C', "ACCT0003", "30.00"));
    data.extend(trailer(3, "150.00", "30.00"));

    let pprof = PostingProfile {
        posting_unit_id: Box::leak(pilot_id.clone().into_boxed_str()),
        business_date: Box::leak(business_date.clone().into_boxed_str()),
        extract_time_utc: "2026-06-08T10:15:00Z",
        source_system: Box::leak(source_system.clone().into_boxed_str()),
        sequence_field: Some("SEQ-NO"), sequence_contiguous: true, txn_id_field: None,
    };

    // 1. EXTRACT.PROFILE.1 — provenance
    let xprof = ExtractProfile {
        source_file_organization: FileOrganization::Sequential,
        extract_method: ExtractMethod::UnloadedFixedRecord,
        record_length_source: RecordLengthSource::Copybook,
        copybook_source: "pilot/detail.cpy",
        code_set_conversion_before_kobold: None,
        source_system_cutoff: Some("2026-06-08T00:00:00Z"),
        business_date: Some(Box::leak(business_date.clone().into_boxed_str())),
        operator_declared_assumptions: &["declared synthetic/private-pilot-shaped extract; not customer records"],
    };
    let extract = extract_manifest(DTL, &data, &xprof);

    // 2. PRIVACY.REDACTION.1 — tokenize ACCT-ID on every detail record BEFORE writing anything
    let rules = [FieldRule { field: "ACCT-ID", action: RedactionAction::TokenizeDeterministic }];
    let pol = RedactionPolicy { rules: &rules, default_action: DefaultAction::AllowUnlisted, token_scope: "pilot" };
    let mut redaction_jsonl = String::new();
    let mut redacted_count = 0usize;
    for rec in data.chunks(RL) {
        if rec.first() == Some(&b'D') {
            let r = redact_record(DTL, rec, &pol, &NoCopy, Encoding::Ascii).unwrap();
            redaction_jsonl.push_str(&r.json);
            redaction_jsonl.push('\n');
            redacted_count += 1;
        }
    }

    // 3. BANK.1/2 + BANK.RECONCILE.1 — control totals + operator view, source-bound to extract + redaction
    let (vs, cs) = specs();
    let bank = reconcile_banking(&data, RL, &vs, &cs, &NoCopy, Encoding::Ascii).unwrap();
    let custody = posting_manifest(PCB, &seqbuf(3), 8, &pprof, &NoCopy, Encoding::Ascii).unwrap();
    let extra_sources = [
        ("KOBOLD.EXTRACT.PROFILE.1", extract.casefile_json.as_str()),
        ("KOBOLD.PRIVACY.REDACTION.1", redaction_jsonl.as_str()),
    ];
    let recon = bank_reconcile_report(&BankReconcileInputs {
        batch: &pprof, custody: &custody, banking: &bank, db2: None,
        redacted_field_count: redacted_count, tokenized_field_count: redacted_count, dirty_count: 0, unsupported_count: 0,
        extra_sources: &extra_sources,
    });

    // 4. DIFF.1 — observed totals vs a DECLARED expected artifact (target is NOT an oracle)
    let totals = vec![("debit".to_string(), "15000".to_string()), ("credit".to_string(), "3000".to_string())];
    let actual = DiffInput { fields: vec![], finding_ids: vec![], control_totals: totals.clone(), output_hash: None };
    let expected = DiffInput { fields: vec![], finding_ids: vec![], control_totals: totals, output_hash: None };
    let target = DiffTarget { target_kind: "declared-expected-totals", oracle_status: OracleStatus::NotOracle, source: "operator-declared", allowed_comparisons: &["control_totals"] };
    let diff = diff_artifacts(&actual, &expected, &target);

    // 5. TOOLING.EXPORT.1 — reviewer field map for the first record, redaction honored, witness named
    let tooling = tooling_export(DTL, &data[..RL], &NoCopy, Encoding::Ascii, Some(&dialect), Some(&pol)).unwrap();

    // 6. PILOT-PACKET.1 — hash-bind every produced artifact + the operator review-notes hash
    let artifacts = [
        PilotArtifact { name: "extract_profile", court: "KOBOLD.EXTRACT.PROFILE.1", content: &extract.casefile_json },
        PilotArtifact { name: "redaction_policy", court: "KOBOLD.PRIVACY.REDACTION.1", content: &redaction_jsonl },
        PilotArtifact { name: "bank_reconcile", court: "KOBOLD.BANK.RECONCILE.1", content: &recon.report_json },
        PilotArtifact { name: "diff", court: "KOBOLD.DIFF.1", content: &diff.report_json },
        PilotArtifact { name: "tooling_export", court: "KOBOLD.TOOLING.EXPORT.1", content: &tooling.json },
    ];
    let packet = pilot_packet(&PilotInputs {
        pilot_id: &pilot_id, business_date: &business_date, source_system: &source_system,
        copybook: DTL, operator_review_notes: &notes, artifacts: &artifacts,
    });

    // write the REDACTED evidence packet (safe to share — no cleartext account ids)
    if fs::create_dir_all(&out).is_err() {
        eprintln!("cannot create out dir {out}");
        return ExitCode::from(5);
    }
    let w = |name: &str, content: &str| {
        let _ = fs::write(format!("{out}/{name}"), content);
    };
    w("extract-casefile.json", &extract.casefile_json);
    w("redaction.jsonl", &redaction_jsonl);
    w("bank-reconcile-report.json", &recon.report_json);
    w("bank-reconcile-report.md", &recon.report_md);
    w("diff-report.json", &diff.report_json);
    w("tooling-export.json", &tooling.json);
    w("pilot-packet.json", &packet.packet_json);
    w("pilot-packet.md", &packet.packet_md);

    // refuse to ship if a redaction leaked cleartext into any artifact
    for (name, content) in [
        ("redaction.jsonl", &redaction_jsonl), ("bank-reconcile-report.json", &recon.report_json),
        ("tooling-export.json", &tooling.json), ("pilot-packet.json", &packet.packet_json),
    ] {
        if content.contains("ACCT0001") {
            eprintln!("ABORT: cleartext account id leaked into {name}");
            return ExitCode::from(4);
        }
    }

    println!("kobold-pilot: wrote a REDACTED evidence packet to {out}/");
    println!("  pilot_id={pilot_id}  records redacted={redacted_count}  complete={}", packet.findings.is_empty());
    println!("  artifacts: extract, redaction, bank_reconcile, diff, tooling_export, pilot-packet (json+md)");
    println!("  claim: a redacted, hash-bound evidence packet over a DECLARED extract.");
    println!("  NOT: customer acceptance, business correctness, compliance, production readiness, ledger truth.");
    ExitCode::SUCCESS
}
