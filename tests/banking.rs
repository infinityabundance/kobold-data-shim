//! KOBOLD.BANK.1 acceptance: a header/detail/trailer banking file reconciles DECLARED trailer control
//! totals against KOBOLD-OBSERVED totals. A balanced file passes; a tampered trailer fails with a SARIF
//! finding. Polarity is taken only from the declared DR/CR field, never a numeric sign. No posting/
//! ledger/business truth is ever claimed.

use kobold_data_shim::{
    reconcile_banking, ControlSpec, Encoding, ExitCode, NoCopy, Variant, VariantSpec,
};

const HDR: &str = "       01 H.\n           05 REC-TYPE PIC X.\n           05 BATCH-ID PIC X(10).\n           05 BUS-DATE PIC 9(8).\n           05 FILLER PIC X(11).\n";
const DTL: &str = "       01 D.\n           05 REC-TYPE PIC X.\n           05 ACCT-NO PIC 9(10).\n           05 DR-CR-IND PIC X.\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n           05 FILLER PIC X(13).\n";
const TRL: &str = "       01 T.\n           05 REC-TYPE PIC X.\n           05 TRL-COUNT PIC 9(6).\n           05 TRL-DEBIT PIC S9(9)V99 COMP-3.\n           05 TRL-CREDIT PIC S9(9)V99 COMP-3.\n           05 FILLER PIC X(11).\n";

fn specs() -> (VariantSpec<'static>, ControlSpec<'static>) {
    let variants: &'static [Variant<'static>] = Box::leak(Box::new([
        Variant {
            discriminator: b"H",
            name: "H",
            copybook: HDR,
        },
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
        variants,
    };
    let c = ControlSpec {
        detail_variant: "D",
        trailer_variant: "T",
        amount_field: "AMOUNT",
        drcr_field: "DR-CR-IND",
        debit_indicator: "D",
        credit_indicator: "C",
        trailer_count_field: "TRL-COUNT",
        trailer_debit_field: "TRL-DEBIT",
        trailer_credit_field: "TRL-CREDIT",
    };
    (v, c)
}

#[test]
fn balanced_file_reconciles() {
    let data = std::fs::read("recon/banking/input.dat").unwrap();
    let (v, c) = specs();
    let r = reconcile_banking(&data, 30, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(
        r.balanced,
        "balanced file should reconcile; findings={:?}",
        r.findings
    );
    assert_eq!(r.verdict, ExitCode::Success);
    assert!(r.findings.is_empty());
    // observed: D 100.00+250.50+10.00 = 360.50 ; C 75.25+300.00+5.55 = 380.80 ; count 6
    assert!(r
        .casefile_json
        .contains("\"observed\":{\"count\":6,\"debit\":\"360.50\",\"credit\":\"380.80\"}"));
    assert!(r.casefile_json.contains("\"balanced\":true"));
    // posting/ledger/business truth are NOT claimed
    assert!(r
        .casefile_json
        .contains("\"posting_truth\":{\"claimed\":false"));
    assert!(r
        .casefile_json
        .contains("\"business_truth\":{\"claimed\":false}"));
    assert!(r
        .casefile_json
        .contains("NEG.BANKING.BALANCED_FILE_IS_NOT_CORRECT_FILE"));
    std::fs::write("recon/banking/casefile.json", &r.casefile_json).unwrap();
}

#[test]
fn tampered_trailer_fails_with_finding() {
    let data = std::fs::read("recon/banking/input-tampered.dat").unwrap();
    let (v, c) = specs();
    let r = reconcile_banking(&data, 30, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(!r.balanced, "tampered trailer must not reconcile");
    assert_eq!(r.verdict, ExitCode::DecodedWithEvidenceWarnings);
    assert!(
        r.findings
            .iter()
            .any(|(rule, _)| rule == "KOBOLD-BANK-CONTROL-MISMATCH"),
        "must emit a control-mismatch finding; got {:?}",
        r.findings
    );
    assert!(r.casefile_json.contains("\"balanced\":false"));
    // the declared debit (tampered, 360.51) != observed (360.50)
    assert!(
        r.casefile_json
            .contains("debit_total: declared 360.51 != observed 360.50")
            || r.findings
                .iter()
                .any(|(_, m)| m.contains("360.51") && m.contains("360.50"))
    );
}

#[test]
fn unknown_record_type_fails_closed() {
    // A record with an undeclared discriminator must fail closed, never be silently decoded.
    let mut data = std::fs::read("recon/banking/input.dat").unwrap();
    data[0] = b'Z'; // corrupt the header discriminator
    let (v, c) = specs();
    let r = reconcile_banking(&data, 30, &v, &c, &NoCopy, Encoding::Ascii).unwrap();
    assert!(r
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-BANK-UNKNOWN-RECORD-TYPE"));
}
