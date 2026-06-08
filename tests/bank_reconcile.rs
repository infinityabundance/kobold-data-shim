//! KOBOLD.BANK.RECONCILE.1 acceptance: the report is a faithful VIEW over existing court evidence
//! (BANK.1/2 + POSTING.1 + DB2HOST.1 + PRIVACY) — every number comes from a court struct, matched/mismatch
//! renders from `balanced`, custody/db2/privacy appear, refused truth layers are visible, and SARIF carries
//! the EXISTING findings. It introduces no new evidence and claims no posting/ledger/settlement/business truth.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::banking::{AccountingProfile, NumericRole, PolarityProfile};
use kobold_data_shim::{
    bank_reconcile_report, posting_manifest, reconcile_banking, BankReconcileInputs, ControlSpec,
    Encoding, NoCopy, PostingProfile, Variant, VariantSpec,
};

fn comp3(pic: &str, value: &str) -> Vec<u8> {
    let pf = build_field(pic, Usage::Comp3, false, false).unwrap();
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    if pf.attr.scale > fp.len() as i16 {
        d.resize(d.len() + (pf.attr.scale - fp.len() as i16) as usize, 0);
    }
    while d.len() < pf.attr.digits as usize {
        d.insert(0, 0);
    }
    let extra = d.len().saturating_sub(pf.attr.digits as usize);
    d.drain(0..extra);
    let mut src: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    if neg {
        if let Some(l) = src.last_mut() {
            *l |= 0x40;
        }
    }
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

const DTL: &str = "       01 D.\n           05 REC-TYPE PIC X.\n           05 DR-CR-IND PIC X.\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n           05 FILLER PIC X(18).\n";
const TRL: &str = "       01 T.\n           05 REC-TYPE PIC X.\n           05 TRL-COUNT PIC 9(6).\n           05 TRL-DEBIT PIC S9(9)V99 COMP-3.\n           05 TRL-CREDIT PIC S9(9)V99 COMP-3.\n           05 FILLER PIC X(12).\n";

fn specs() -> (VariantSpec<'static>, ControlSpec<'static>) {
    let vs: &'static [Variant<'static>] = Box::leak(Box::new([
        Variant {
            discriminator: b"D",
            name: "D",
            copybook: DTL,
        },
        Variant {
            discriminator: b"T",
            name: "T",
            copybook: TRL,
        },
    ]));
    let v = VariantSpec {
        discriminator_offset: 0,
        discriminator_len: 1,
        variants: vs,
    };
    let roles: &'static [(&str, NumericRole)] =
        Box::leak(Box::new([("AMOUNT", NumericRole::Amount)]));
    let c = ControlSpec {
        detail_variant: "D",
        trailer_variant: "T",
        trailer_count_field: "TRL-COUNT",
        trailer_debit_field: "TRL-DEBIT",
        trailer_credit_field: "TRL-CREDIT",
        accounting: AccountingProfile {
            numeric_roles: roles,
            polarity: PolarityProfile {
                amount_field: "AMOUNT",
                source_field: "DR-CR-IND",
                debit_values: Box::leak(Box::new(["D"])),
                credit_values: Box::leak(Box::new(["C"])),
            },
        },
    };
    (v, c)
}
fn detail(ind: u8, amt: &str) -> Vec<u8> {
    let mut d = vec![b'D', ind];
    d.extend(comp3("S9(7)V99", amt));
    d.resize(28, b' ');
    d
}
fn trailer(count: u32, debit: &str, credit: &str) -> Vec<u8> {
    let mut t = vec![b'T'];
    t.extend(format!("{count:06}").into_bytes());
    t.extend(comp3("S9(9)V99", debit));
    t.extend(comp3("S9(9)V99", credit));
    t.resize(28, b' ');
    t
}

// POSTING.1 over a clean uniform sequence buffer (the custody court's output for the same batch run).
const PCB: &str = "       01 R.\n           05 SEQ-NO PIC 9(6).\n           05 PAD PIC X(2).\n";
fn seqbuf(n: u32) -> Vec<u8> {
    (1..=n)
        .flat_map(|i| {
            let mut r = format!("{i:06}").into_bytes();
            r.extend(b"  ");
            r
        })
        .collect()
}
fn pprof() -> PostingProfile<'static> {
    PostingProfile {
        posting_unit_id: "BATCH-20260608-001",
        business_date: "2026-06-08",
        extract_time_utc: "2026-06-08T10:15:00Z",
        source_system: "synthetic",
        sequence_field: Some("SEQ-NO"),
        sequence_contiguous: true,
        txn_id_field: None,
    }
}

#[test]
fn matched_view_is_faithful_and_refuses_truth() {
    let (v, c) = specs();
    let mut data = detail(b'D', "100.00");
    data.extend(trailer(1, "100.00", "0.00"));
    let bank = reconcile_banking(&data, 28, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(bank.balanced);
    let custody = posting_manifest(PCB, &seqbuf(3), 8, &pprof(), &NoCopy, Encoding::Ascii).unwrap();
    let inputs = BankReconcileInputs {
        batch: &pprof(),
        custody: &custody,
        banking: &bank,
        db2: None,
        redacted_field_count: 2,
        tokenized_field_count: 1,
        dirty_count: 0,
        unsupported_count: 0,
        extra_sources: &[],
    };
    let r = bank_reconcile_report(&inputs);
    // faithful to the court structs (no recomputation/drift)
    assert!(r.report_json.contains("\"verdict\":\"matched\""));
    assert!(r.report_json.contains(&format!(
        "\"observed_count\":{}",
        bank.summary.observed_count
    )));
    assert!(r.report_json.contains("\"observed_debit\":\"100.00\""));
    assert!(
        r.report_json.contains("\"last_chain_hash\":")
            && r.report_json.contains(&custody.last_chain_hash)
    );
    assert!(
        r.report_json.contains("\"sequence_min\":1")
            && r.report_json.contains("\"sequence_max\":3")
    );
    assert!(
        r.report_json.contains("\"redacted_field_count\":2")
            && r.report_json.contains("\"tokenized_field_count\":1")
    );
    // view contract + refused truth layers
    assert!(
        r.report_json.contains("\"view_only\":true")
            && r.report_json.contains("\"introduces_new_evidence\":false")
    );
    assert!(
        r.report_json.contains("\"posting_truth\":false")
            && r.report_json.contains("\"business_truth\":false")
    );
    assert!(r.report_json.contains("\"public_output_claim\":false"));
    assert!(r
        .report_json
        .contains("NEG.BANK_RECONCILE.MATCH_NOT_CORRECTNESS"));
    // markdown + sarif render
    assert!(r.report_md.contains("Controls — **matched**"));
    assert!(r.sarif_json.contains("\"version\":\"2.1.0\""));
}

#[test]
fn mismatch_renders_finding_in_view_and_sarif() {
    let (v, c) = specs();
    let mut data = detail(b'D', "100.00");
    data.extend(trailer(1, "999.99", "0.00")); // declared != observed
    let bank = reconcile_banking(&data, 28, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(!bank.balanced);
    let custody = posting_manifest(PCB, &seqbuf(2), 8, &pprof(), &NoCopy, Encoding::Ascii).unwrap();
    let inputs = BankReconcileInputs {
        batch: &pprof(),
        custody: &custody,
        banking: &bank,
        db2: None,
        redacted_field_count: 0,
        tokenized_field_count: 0,
        dirty_count: 0,
        unsupported_count: 0,
        extra_sources: &[],
    };
    let r = bank_reconcile_report(&inputs);
    assert!(r.report_json.contains("\"verdict\":\"mismatch\""));
    // the EXISTING banking finding surfaces in the aggregated SARIF (not a new finding)
    assert!(r.sarif_json.contains("KOBOLD-BANK-CONTROL-MISMATCH"));
}

#[test]
fn source_evidence_binds_courts_and_changes_with_source() {
    let (v, c) = specs();
    let mut data = detail(b'D', "100.00");
    data.extend(trailer(1, "100.00", "0.00"));
    let bank = reconcile_banking(&data, 28, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    let custody = posting_manifest(PCB, &seqbuf(3), 8, &pprof(), &NoCopy, Encoding::Ascii).unwrap();
    let extra: [(&str, &str); 1] = [("KOBOLD.EXTRACT.PROFILE.1", "{\"schema\":\"x\"}")];
    let inputs = BankReconcileInputs {
        batch: &pprof(),
        custody: &custody,
        banking: &bank,
        db2: None,
        redacted_field_count: 0,
        tokenized_field_count: 0,
        dirty_count: 0,
        unsupported_count: 0,
        extra_sources: &extra,
    };
    let r = bank_reconcile_report(&inputs);
    // provably derived from named, hash-pinned source casefiles
    assert!(
        r.report_json.contains("\"derived_view\":true")
            && r.report_json.contains("\"creates_new_truth\":false")
    );
    assert!(r.report_json.contains(
        "\"court\":\"KOBOLD.BANK.1\",\"path\":\"reports/casefiles/KOBOLD.BANK.1/casefile.json\""
    ));
    assert!(
        r.report_json.contains("\"court\":\"KOBOLD.POSTING.1\"")
            && r.report_json
                .contains("\"court\":\"KOBOLD.EXTRACT.PROFILE.1\"")
    );
    assert!(r
        .report_json
        .contains("NEG.BANK_RECONCILE.SOURCE_HASH_MISMATCH"));
    // a CHANGED source (different banking input) changes the report hash -> freshness is real
    let mut data2 = detail(b'D', "999.99");
    data2.extend(trailer(1, "999.99", "0.00"));
    let bank2 = reconcile_banking(&data2, 28, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    let inputs2 = BankReconcileInputs {
        banking: &bank2,
        ..inputs
    };
    let r2 = bank_reconcile_report(&inputs2);
    assert_ne!(
        r.report_json, r2.report_json,
        "changed source casefile must change the report"
    );
}
