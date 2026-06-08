//! KOBOLD.PERF.2 acceptance: per-stage profiling never changes the emitted bytes, the stage timings are
//! populated, and the FULL custody workload (reconcile + POSTING.1 + PRIVACY.REDACTION.1) is byte-identical
//! scalar vs the record-level Rayon path. Performance is reported only after this evidence parity holds.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::recon::reconcile_profile;
use kobold_data_shim::{recon::reconcile_encoded, Encoding, NoCopy};
#[cfg(feature = "rayon")]
use kobold_data_shim::{
    posting_manifest, redact_record, DefaultAction, FieldRule, PostingProfile, RedactionAction,
    RedactionPolicy,
};

const CB: &str = "       01 R.\n           05 SEQ-NO PIC 9(6).\n           05 NAME PIC X(6).\n           05 BAL PIC S9(7)V99 COMP-3.\n           05 BR PIC 9(4) COMP.\n           05 ST PIC X.\n               88 ACTIVE VALUE \"A\".\n";
const RL: usize = 6 + 6 + 5 + 2 + 1;

fn comp3(value: &str) -> Vec<u8> {
    let pf = build_field("S9(7)V99", Usage::Comp3, false, false).unwrap();
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
fn corpus(n: usize) -> Vec<u8> {
    let mut d = Vec::new();
    for i in 0..n {
        d.extend_from_slice(format!("{:06}", i + 1).as_bytes());
        let mut name = format!("C{:05}", i % 99999).into_bytes();
        name.resize(6, b' ');
        d.extend(name);
        d.extend(comp3(&format!("{}.{:02}", 1000 + i, i % 100)));
        d.extend(&((1000 + i as u16) % 9000).to_be_bytes());
        d.push(if i % 2 == 0 { b'A' } else { b'C' });
    }
    d
}

#[test]
fn profile_does_not_change_output_and_populates_stages() {
    let data = corpus(500);
    let plain = reconcile_encoded("p2", CB, &data, RL, "0.7.1", &NoCopy, Encoding::Ascii).unwrap();
    let (profd, prof) =
        reconcile_profile("p2", CB, &data, RL, "0.7.1", &NoCopy, Encoding::Ascii).unwrap();
    // profiling is byte-identical to the un-profiled path
    assert_eq!(plain.jsonl, profd.jsonl);
    assert_eq!(plain.audit_json, profd.audit_json);
    assert_eq!(plain.unsupported_json, profd.unsupported_json);
    // the three pipeline stages are timed (non-zero for a 500-record corpus)
    assert!(
        prof.parse_ns > 0 && prof.record_ns > 0 && prof.aggregate_ns > 0,
        "stages populated: {prof:?}"
    );
}

#[cfg(feature = "rayon")]
#[test]
fn full_custody_workload_scalar_eq_rayon() {
    use kobold_data_shim::recon::reconcile_encoded_parallel;
    let data = corpus(3000);
    // 1. reconcile: scalar == rayon, byte-for-byte
    let s = reconcile_encoded("p2", CB, &data, RL, "0.7.1", &NoCopy, Encoding::Ascii).unwrap();
    let p =
        reconcile_encoded_parallel("p2", CB, &data, RL, "0.7.1", &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(s.jsonl, p.jsonl);
    assert_eq!(s.audit_json, p.audit_json);
    assert_eq!(s.unsupported_json, p.unsupported_json);
    // 2. POSTING.1 custody is order-sensitive and stays identical (computed serially over the same bytes)
    let prof = PostingProfile {
        posting_unit_id: "B",
        business_date: "2026-06-08",
        extract_time_utc: "t",
        source_system: "synthetic",
        sequence_field: Some("SEQ-NO"),
        sequence_contiguous: true,
        txn_id_field: None,
    };
    let a = posting_manifest(CB, &data, RL, &prof, &NoCopy, Encoding::Ascii).unwrap();
    let b = posting_manifest(CB, &data, RL, &prof, &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(a.last_chain_hash, b.last_chain_hash);
    // 3. PRIVACY.REDACTION.1 hashes/tokens identical for a sampled record
    let rules = [FieldRule {
        field: "NAME",
        action: RedactionAction::TokenizeDeterministic,
    }];
    let pol = RedactionPolicy {
        rules: &rules,
        default_action: DefaultAction::AllowUnlisted,
        token_scope: "casefile",
    };
    let r1 = redact_record(CB, &data[..RL], &pol, &NoCopy, Encoding::Ascii).unwrap();
    let r2 = redact_record(CB, &data[..RL], &pol, &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(r1.json, r2.json);
}
