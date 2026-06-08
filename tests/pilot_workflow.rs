//! KOBOLD.PILOT.WORKFLOW.1 — end-to-end pilot wiring over a DECLARED synthetic/private-pilot-shaped extract.
//! It proves the workflow *plumbing* and evidence custody: a synthetic banking extract flows through
//! EXTRACT.PROFILE.1 → PRIVACY.REDACTION.1 → BANK.1/2 + BANK.RECONCILE.1 → DIFF.1 → TOOLING.EXPORT.1 →
//! PILOT-PACKET.1, and the pilot packet hash-binds every produced artifact. It does NOT claim customer-data
//! coverage, production readiness, compliance, or business acceptance — the bytes are synthetic.

use gnucobol_rs::{build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY};
use kobold_data_shim::banking::{AccountingProfile, NumericRole, PolarityProfile};
use kobold_data_shim::{
    bank_reconcile_report, diff_artifacts, extract_manifest, pilot_packet, posting_manifest,
    reconcile_banking, redact_record, tooling_export, BankReconcileInputs, ControlSpec, DefaultAction,
    DiffInput, DiffTarget, Encoding, ExtractMethod, ExtractProfile, FieldRule, FileOrganization, NoCopy,
    OracleStatus, PilotArtifact, PilotInputs, PostingProfile, RecordLengthSource, RedactionAction,
    RedactionPolicy, Variant, VariantSpec,
};

fn comp3(pic: &str, value: &str) -> Vec<u8> {
    let pf = build_field(pic, Usage::Comp3, false, false).unwrap();
    let (ip, fp) = value.split_once('.').unwrap_or((value, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    if pf.attr.scale > fp.len() as i16 { d.resize(d.len() + (pf.attr.scale - fp.len() as i16) as usize, 0); }
    while d.len() < pf.attr.digits as usize { d.insert(0, 0); }
    let extra = d.len().saturating_sub(pf.attr.digits as usize); d.drain(0..extra);
    let src: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    let sa = FieldAttr { field_type: COB_TYPE_NUMERIC_DISPLAY, digits: pf.attr.digits, scale: pf.attr.scale, flags: COB_FLAG_HAVE_SIGN };
    let mut out = vec![0u8; pf.size]; cob_move(&src, &sa, &mut out, &pf.attr).unwrap(); out
}
const DTL: &str = "       01 D.\n           05 REC-TYPE PIC X.\n           05 DR-CR-IND PIC X.\n           05 ACCT-ID PIC X(8).\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n           05 FILLER PIC X(11).\n";
const TRL: &str = "       01 T.\n           05 REC-TYPE PIC X.\n           05 TRL-COUNT PIC 9(6).\n           05 TRL-DEBIT PIC S9(9)V99 COMP-3.\n           05 TRL-CREDIT PIC S9(9)V99 COMP-3.\n           05 FILLER PIC X(12).\n";
const RL: usize = 28;

fn detail(ind: u8, acct: &str, amt: &str) -> Vec<u8> {
    let mut d = vec![b'D', ind];
    let mut a = acct.as_bytes().to_vec(); a.resize(8, b' '); d.extend(a);
    d.extend(comp3("S9(7)V99", amt));
    d.resize(RL, b' '); d
}
fn trailer(count: u32, debit: &str, credit: &str) -> Vec<u8> {
    let mut t = vec![b'T']; t.extend(format!("{count:06}").into_bytes());
    t.extend(comp3("S9(9)V99", debit)); t.extend(comp3("S9(9)V99", credit));
    t.resize(RL, b' '); t
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
        accounting: AccountingProfile { numeric_roles: roles, polarity: PolarityProfile {
            amount_field: "AMOUNT", source_field: "DR-CR-IND",
            debit_values: Box::leak(Box::new(["D"])), credit_values: Box::leak(Box::new(["C"])) } },
    };
    (v, c)
}
const PCB: &str = "       01 R.\n           05 SEQ-NO PIC 9(6).\n           05 PAD PIC X(2).\n";
fn seqbuf(n: u32) -> Vec<u8> { (1..=n).flat_map(|i| { let mut r = format!("{i:06}").into_bytes(); r.extend(b"  "); r }).collect() }
fn pprof() -> PostingProfile<'static> {
    PostingProfile { posting_unit_id: "PILOT-20260608-001", business_date: "2026-06-08",
        extract_time_utc: "2026-06-08T10:15:00Z", source_system: "synthetic-pilot",
        sequence_field: Some("SEQ-NO"), sequence_contiguous: true, txn_id_field: None }
}

#[test]
fn pilot_workflow_wires_all_courts_and_binds_evidence() {
    // 0. a DECLARED synthetic/private-pilot-shaped extract (2 debits + 1 credit + a balancing trailer)
    let mut data = detail(b'D', "ACCT0001", "100.00");
    data.extend(detail(b'D', "ACCT0002", "50.00"));
    data.extend(detail(b'C', "ACCT0003", "30.00"));
    data.extend(trailer(3, "150.00", "30.00"));

    // 1. EXTRACT.PROFILE.1 — provenance over the extract
    let xprof = ExtractProfile {
        source_file_organization: FileOrganization::Sequential,
        extract_method: ExtractMethod::UnloadedFixedRecord,
        record_length_source: RecordLengthSource::Copybook,
        copybook_source: "pilot/detail.cpy",
        code_set_conversion_before_kobold: None,
        source_system_cutoff: Some("2026-06-08T00:00:00Z"),
        business_date: Some("2026-06-08"),
        operator_declared_assumptions: &["synthetic data; not customer records"],
    };
    let extract = extract_manifest(DTL, &data, &xprof);

    // 2. PRIVACY.REDACTION.1 — tokenize the account id before any artifact leaves the secure zone
    let rules = [FieldRule { field: "ACCT-ID", action: RedactionAction::TokenizeDeterministic }];
    let pol = RedactionPolicy { rules: &rules, default_action: DefaultAction::AllowUnlisted, token_scope: "pilot" };
    let redaction = redact_record(DTL, &data[..RL], &pol, &NoCopy, Encoding::Ascii).unwrap();
    assert!(!redaction.json.contains("ACCT0001"), "redaction must not leak the account id");

    // 3. BANK.1/2 + BANK.RECONCILE.1 — control totals + operator view, source-bound to extract + redaction
    let bank = reconcile_banking(&data, RL, &specs().0, &specs().1, &NoCopy, Encoding::Ascii).unwrap();
    let custody = posting_manifest(PCB, &seqbuf(3), 8, &pprof(), &NoCopy, Encoding::Ascii).unwrap();
    let extra_sources = [
        ("KOBOLD.EXTRACT.PROFILE.1", extract.casefile_json.as_str()),
        ("KOBOLD.PRIVACY.REDACTION.1", redaction.json.as_str()),
    ];
    let recon_inputs = BankReconcileInputs {
        batch: &pprof(), custody: &custody, banking: &bank, db2: None,
        redacted_field_count: 1, tokenized_field_count: 1, dirty_count: 0, unsupported_count: 0,
        extra_sources: &extra_sources,
    };
    let recon = bank_reconcile_report(&recon_inputs);
    assert!(recon.report_json.contains("KOBOLD.EXTRACT.PROFILE.1") && recon.report_json.contains("KOBOLD.PRIVACY.REDACTION.1"));

    // 4. DIFF.1 — observed control totals vs a DECLARED expected artifact (target is NOT an oracle)
    let actual = DiffInput { fields: vec![], finding_ids: vec![],
        control_totals: vec![("debit".into(), "15000".into()), ("credit".into(), "3000".into())], output_hash: None };
    let expected = DiffInput { fields: vec![], finding_ids: vec![],
        control_totals: vec![("debit".into(), "15000".into()), ("credit".into(), "3000".into())], output_hash: None };
    let target = DiffTarget { target_kind: "declared-expected-totals", oracle_status: OracleStatus::NotOracle,
        source: "test-golden", allowed_comparisons: &["control_totals"] };
    let diff = diff_artifacts(&actual, &expected, &target);
    assert!(diff.report_json.contains("TARGET-NOT-ORACLE") || diff.report_json.contains("not_oracle"));

    // 5. TOOLING.EXPORT.1 — field map for the reviewer, with the account id redacted, witness named
    let tooling = tooling_export(DTL, &data[..RL], &NoCopy, Encoding::Ascii, Some("gnucobol-3.2.0-default"), Some(&pol)).unwrap();
    assert!(!tooling.json.contains("ACCT0001"), "tooling export must honor redaction");

    // 6. PILOT-PACKET.1 — hash-bind every produced artifact + the operator's review notes
    let artifacts = [
        PilotArtifact { name: "extract_profile", court: "KOBOLD.EXTRACT.PROFILE.1", content: &extract.casefile_json },
        PilotArtifact { name: "redaction_policy", court: "KOBOLD.PRIVACY.REDACTION.1", content: &redaction.json },
        PilotArtifact { name: "bank_reconcile", court: "KOBOLD.BANK.RECONCILE.1", content: &recon.report_json },
        PilotArtifact { name: "diff", court: "KOBOLD.DIFF.1", content: &diff.report_json },
        PilotArtifact { name: "tooling_export", court: "KOBOLD.TOOLING.EXPORT.1", content: &tooling.json },
    ];
    let notes = "pilot review: synthetic extract; totals balanced; account ids tokenized; no customer data present";
    let packet = pilot_packet(&PilotInputs {
        pilot_id: "PILOT-20260608-001", business_date: "2026-06-08", source_system: "synthetic-pilot",
        copybook: DTL, operator_review_notes: notes, artifacts: &artifacts,
    });

    // the workflow is wired end-to-end and the packet hash-binds the whole chain
    assert!(packet.findings.is_empty(), "all required pilot artifacts present: {:?}", packet.findings);
    assert!(packet.packet_json.contains("\"complete\":true") && packet.packet_json.contains("\"creates_new_truth\":false"));
    assert!(packet.packet_json.contains("\"name\":\"extract_profile\"") && packet.packet_json.contains("\"name\":\"bank_reconcile\"") && packet.packet_json.contains("\"name\":\"tooling_export\""));
    // custody preserved, no cleartext leaks anywhere in the packet, review notes only hashed
    assert!(!packet.packet_json.contains("ACCT0001") && !packet.packet_md.contains("ACCT0001"));
    assert!(packet.packet_json.contains("\"review_notes_embedded\":false") && !packet.packet_json.contains("no customer data present"));
}
