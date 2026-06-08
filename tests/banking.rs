//! KOBOLD.BANK.2 / ACCOUNTING.PROFILE.1 acceptance: numeric-role + debit/credit polarity come ONLY from
//! a declared profile over named fields and value tables — never from PIC, numeric sign, CR/DB
//! presentation, or field-name heuristics. Identifiers/rates/codes are numeric but never summed as money.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::banking::{AccountingProfile, NumericRole, PolarityProfile};
use kobold_data_shim::{reconcile_banking, ControlSpec, Encoding, NoCopy, Variant, VariantSpec};

const HDR: &str = "       01 H.\n           05 REC-TYPE PIC X.\n           05 BATCH-ID PIC X(10).\n           05 BUS-DATE PIC 9(8).\n           05 FILLER PIC X(11).\n";
const DTL: &str = "       01 D.\n           05 REC-TYPE PIC X.\n           05 ACCT-NO PIC 9(10).\n           05 DR-CR-IND PIC X.\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n           05 RATE PIC S9(1)V9(4) COMP-3.\n           05 FILLER PIC X(10).\n";
const TRL: &str = "       01 T.\n           05 REC-TYPE PIC X.\n           05 TRL-COUNT PIC 9(6).\n           05 TRL-DEBIT PIC S9(9)V99 COMP-3.\n           05 TRL-CREDIT PIC S9(9)V99 COMP-3.\n           05 FILLER PIC X(11).\n";

fn variants() -> VariantSpec<'static> {
    let vs: &'static [Variant<'static>] = Box::leak(Box::new([
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
    VariantSpec {
        discriminator_offset: 0,
        discriminator_len: 1,
        variants: vs,
    }
}

fn control() -> ControlSpec<'static> {
    let roles: &'static [(&str, NumericRole)] = Box::leak(Box::new([
        ("AMOUNT", NumericRole::Amount),
        ("RATE", NumericRole::Rate),
        ("ACCT-NO", NumericRole::Identifier),
    ]));
    ControlSpec {
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
    }
}

// --- byte builders (COMP-3 via the sealed gnucobol-rs court) ---
fn comp3(pic: &str, value: &str) -> Vec<u8> {
    let pf = build_field(pic, Usage::Comp3, false, false).unwrap();
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    let (have, scale) = (fp.len() as i16, pf.attr.scale);
    if scale > have {
        d.resize(d.len() + (scale - have) as usize, 0);
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
    let sattr = FieldAttr {
        field_type: COB_TYPE_NUMERIC_DISPLAY,
        digits: pf.attr.digits,
        scale: pf.attr.scale,
        flags: COB_FLAG_HAVE_SIGN,
    };
    let mut out = vec![0u8; pf.size];
    cob_move(&src, &sattr, &mut out, &pf.attr).unwrap();
    out
}
fn pad(mut r: Vec<u8>) -> Vec<u8> {
    r.resize(30, b' ');
    r
}
fn detail(ind: &str, amount: &str) -> Vec<u8> {
    let mut d = vec![b'D'];
    d.extend(b"0000000001");
    d.push(ind.as_bytes()[0]);
    d.extend(comp3("S9(7)V99", amount));
    d.extend(comp3("S9(1)V9(4)", "9.9999")); // RATE: a large numeric that must NEVER be summed as money
    pad(d)
}
fn trailer(count: u32, debit: &str, credit: &str) -> Vec<u8> {
    let mut t = vec![b'T'];
    t.extend(format!("{count:06}").into_bytes());
    t.extend(comp3("S9(9)V99", debit));
    t.extend(comp3("S9(9)V99", credit));
    pad(t)
}

// --- corpus tests (BANK.1 spine, now under the declared profile) ---
#[test]
fn balanced_corpus_reconciles_rate_not_summed() {
    let data = std::fs::read("recon/banking/input.dat").unwrap();
    let r =
        reconcile_banking(&data, 30, &variants(), &control(), &NoCopy, Encoding::Ascii).unwrap();
    // If the RATE field (1.05 each) were summed as money, observed would not equal the declared totals.
    assert!(r.balanced, "balanced; findings={:?}", r.findings);
    assert!(r
        .casefile_json
        .contains("\"observed\":{\"count\":6,\"debit\":\"360.50\",\"credit\":\"380.80\"}"));
    assert!(
        r.casefile_json.contains("\"RATE\":\"rate\"")
            && r.casefile_json
                .contains("\"numeric_sign_policy\":\"not_polarity\"")
    );
}

#[test]
fn tampered_trailer_fails_with_finding() {
    let data = std::fs::read("recon/banking/input-tampered.dat").unwrap();
    let r =
        reconcile_banking(&data, 30, &variants(), &control(), &NoCopy, Encoding::Ascii).unwrap();
    assert!(!r.balanced);
    assert!(r
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-BANK-CONTROL-MISMATCH"));
}

// --- ACCOUNTING.PROFILE.1 acceptance ---
#[test]
fn negative_amount_with_debit_indicator_is_still_debit() {
    // sign != polarity: a NEGATIVE amount with a declared "D" stays a debit (value preserved with sign).
    let mut data = detail("D", "-50.00");
    data.extend(trailer(1, "-50.00", "0.00"));
    let r =
        reconcile_banking(&data, 30, &variants(), &control(), &NoCopy, Encoding::Ascii).unwrap();
    assert!(
        r.balanced,
        "declared debit -50.00 == observed; findings={:?}",
        r.findings
    );
    assert!(r
        .casefile_json
        .contains("\"observed\":{\"count\":1,\"debit\":\"-50.00\",\"credit\":\"0.00\"}"));
}

#[test]
fn unknown_polarity_value_fails_closed() {
    let mut data = detail("X", "100.00"); // 'X' not in declared debit/credit tables
    data.extend(trailer(1, "100.00", "0.00"));
    let r =
        reconcile_banking(&data, 30, &variants(), &control(), &NoCopy, Encoding::Ascii).unwrap();
    assert!(
        r.findings
            .iter()
            .any(|(rule, _)| rule == "KOBOLD-BANK-UNKNOWN-POLARITY"),
        "unknown polarity must fail closed; got {:?}",
        r.findings
    );
    assert!(!r.balanced);
}

#[test]
fn no_profile_match_means_no_total() {
    // A credit detail whose amount is summed only to credit (declared C) -- a debit total computed from
    // sign would be wrong; the declared profile is the only authority.
    let mut data = detail("C", "100.00");
    data.extend(trailer(1, "0.00", "100.00"));
    let r =
        reconcile_banking(&data, 30, &variants(), &control(), &NoCopy, Encoding::Ascii).unwrap();
    assert!(r.balanced);
    assert!(r
        .casefile_json
        .contains("\"observed\":{\"count\":1,\"debit\":\"0.00\",\"credit\":\"100.00\"}"));
}
