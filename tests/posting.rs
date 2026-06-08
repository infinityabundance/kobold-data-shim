//! KOBOLD.POSTING.1 acceptance: declared posting-unit custody — batch identity, hash chain over record
//! ORDER, sequence min/max/duplicates/gaps, duplicate transaction ids. No ledger/settlement/balance/
//! business truth. Record-order mutation must change the chain hash.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::{posting_manifest, Encoding, NoCopy, PostingProfile};

const CB: &str = "       01 D.\n           05 SEQ-NO PIC 9(6).\n           05 TXN-ID PIC X(8).\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n";
const RL: usize = 19; // 6 + 8 + 5

fn comp3(value: &str) -> Vec<u8> {
    let pf = build_field("S9(7)V99", Usage::Comp3, false, false).unwrap();
    let (ip, fp) = value.split_once('.').unwrap_or((value, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    let have = fp.len() as i16;
    if pf.attr.scale > have {
        d.resize(d.len() + (pf.attr.scale - have) as usize, 0);
    }
    while d.len() < pf.attr.digits as usize {
        d.insert(0, 0);
    }
    let extra = d.len().saturating_sub(pf.attr.digits as usize);
    d.drain(0..extra);
    let src: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
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
fn rec(seq: u32, txn: &str, amt: &str) -> Vec<u8> {
    let mut r = format!("{seq:06}").into_bytes();
    let mut t = txn.as_bytes().to_vec();
    t.resize(8, b' ');
    r.extend(t);
    r.extend(comp3(amt));
    r
}
fn buf(records: &[Vec<u8>]) -> Vec<u8> {
    records.iter().flatten().copied().collect()
}

fn profile(contiguous: bool) -> PostingProfile<'static> {
    PostingProfile {
        posting_unit_id: "BATCH-20260608-001",
        business_date: "2026-06-08",
        extract_time_utc: "2026-06-08T10:15:00Z",
        source_system: "synthetic",
        sequence_field: Some("SEQ-NO"),
        sequence_contiguous: contiguous,
        txn_id_field: Some("TXN-ID"),
    }
}

#[test]
fn clean_posting_unit() {
    let data = buf(&(1..=5)
        .map(|n| rec(n, &format!("T{n:07}"), "10.00"))
        .collect::<Vec<_>>());
    let m = posting_manifest(CB, &data, RL, &profile(true), &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(m.record_count, 5);
    assert!(m.seq_duplicates.is_empty() && m.seq_gaps.is_empty() && m.txn_duplicates.is_empty());
    assert!(m.findings.is_empty());
    assert!(m.manifest_json.contains("\"min\":1") && m.manifest_json.contains("\"max\":5"));
    assert!(m
        .manifest_json
        .contains("\"posting_unit_id\":\"BATCH-20260608-001\""));
    assert!(!m.last_chain_hash.is_empty());
    // truth layers refused
    assert!(m
        .casefile_json
        .contains("\"ledger_truth\":{\"claimed\":false}"));
}

#[test]
fn duplicate_sequence_detected() {
    let data = buf(&[
        rec(1, "A", "1.00"),
        rec(2, "B", "1.00"),
        rec(2, "C", "1.00"),
        rec(4, "D", "1.00"),
    ]);
    let m = posting_manifest(CB, &data, RL, &profile(false), &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(m.seq_duplicates, vec![2]);
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-POSTING-DUPLICATE-SEQUENCE"));
}

#[test]
fn sequence_gap_detected_only_when_contiguous_declared() {
    let data = buf(&[
        rec(1, "A", "1.00"),
        rec(2, "B", "1.00"),
        rec(4, "C", "1.00"),
        rec(5, "D", "1.00"),
    ]);
    // contiguous declared -> gap at 3
    let m = posting_manifest(CB, &data, RL, &profile(true), &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(m.seq_gaps, vec![3]);
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-POSTING-SEQUENCE-GAP"));
    // NOT declared contiguous -> no gap claim
    let m2 = posting_manifest(CB, &data, RL, &profile(false), &NoCopy, Encoding::Ascii).unwrap();
    assert!(m2.seq_gaps.is_empty());
}

#[test]
fn duplicate_transaction_id_detected() {
    let data = buf(&[
        rec(1, "DUP", "1.00"),
        rec(2, "DUP", "2.00"),
        rec(3, "OK", "3.00"),
    ]);
    let m = posting_manifest(CB, &data, RL, &profile(false), &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(m.txn_duplicates, vec!["DUP".to_string()]);
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-POSTING-DUPLICATE-TXN-ID"));
}

#[test]
fn record_order_mutation_changes_chain_hash() {
    let recs: Vec<Vec<u8>> = (1..=5).map(|n| rec(n, &format!("T{n}"), "1.00")).collect();
    let forward = posting_manifest(
        CB,
        &buf(&recs),
        RL,
        &profile(false),
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    let mut rev = recs.clone();
    rev.reverse();
    let reversed = posting_manifest(
        CB,
        &buf(&rev),
        RL,
        &profile(false),
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    assert_ne!(
        forward.last_chain_hash, reversed.last_chain_hash,
        "reordering records must change the chain"
    );
}

#[test]
fn posting_unit_id_required() {
    let p = PostingProfile {
        posting_unit_id: "",
        business_date: "x",
        extract_time_utc: "x",
        source_system: "x",
        sequence_field: None,
        sequence_contiguous: false,
        txn_id_field: None,
    };
    assert!(posting_manifest(CB, &rec(1, "A", "1.00"), RL, &p, &NoCopy, Encoding::Ascii).is_err());
}
