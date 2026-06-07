//! KOBOLD.FILE.1 acceptance: fixed-record container ingest with explicit policies, stable offsets,
//! a byte-stable file audit, and documented exit codes. Strict by default; nothing silently repaired.

use kobold_data_shim::file::{ingest, IngestPolicy, PartialRecord, TrailingNewline};
use kobold_data_shim::ExitCode;

fn strict(rl: usize) -> IngestPolicy {
    IngestPolicy::strict(rl)
}

#[test]
fn exact_multiple_splits_with_true_offsets() {
    let data = vec![b'x'; 55 * 4];
    let ing = ingest(&data, &strict(55)).unwrap();
    assert_eq!(ing.spans.len(), 4);
    assert_eq!(ing.verdict, ExitCode::Success);
    for (i, s) in ing.spans.iter().enumerate() {
        assert_eq!(s.offset, i * 55, "offset == index * record_len");
        assert_eq!(s.len, 55);
        assert!(!s.partial);
    }
    // record() slices the true bytes.
    assert_eq!(ing.record(&data, 2).unwrap().len(), 55);
}

#[test]
fn empty_file_is_zero_records_success() {
    let ing = ingest(b"", &strict(55)).unwrap();
    assert_eq!(ing.spans.len(), 0);
    assert_eq!(ing.verdict, ExitCode::Success);
    assert!(ing.file_audit_json().contains("\"record_count\":0"));
    assert!(ing
        .file_audit_json()
        .contains("\"offsets\":{\"first\":-1,\"last\":-1}"));
}

#[test]
fn record_len_zero_is_config_error() {
    let e = ingest(b"abc", &strict(0)).unwrap_err();
    assert_eq!(e.exit, ExitCode::IoOrConfigError);
}

#[test]
fn short_trailing_record_fails_strict() {
    let data = vec![b'x'; 55 * 3 + 10]; // 10 leftover bytes
    let e = ingest(&data, &strict(55)).unwrap_err();
    assert_eq!(e.exit, ExitCode::InvalidInputShape);
    assert!(format!("{e}").contains("partial trailing record"));
}

#[test]
fn short_trailing_record_preserved_in_evidence() {
    let data = vec![b'x'; 55 * 3 + 10];
    let policy = IngestPolicy {
        record_len: 55,
        trailing_newline: TrailingNewline::Reject,
        partial_record: PartialRecord::Evidence,
    };
    let ing = ingest(&data, &policy).unwrap();
    assert_eq!(ing.spans.len(), 4); // 3 full + 1 partial
    let last = ing.spans.last().unwrap();
    assert!(last.partial && last.len == 10 && last.offset == 165);
    assert_eq!(ing.verdict, ExitCode::DecodedWithEvidenceWarnings);
    assert!(ing
        .file_audit_json()
        .contains("\"partial_record_present\":true"));
    assert!(ing.file_audit_json().contains("\"verdict_exit_code\":1"));
}

#[test]
fn trailing_newline_policy_is_explicit() {
    let mut data = vec![b'x'; 55 * 2];
    data.push(b'\n');
    // Reject: a final LF is an invalid shape.
    let e = ingest(&data, &strict(55)).unwrap_err();
    assert_eq!(e.exit, ExitCode::InvalidInputShape);
    assert!(format!("{e}").contains("trailing newline"));
    // AllowFinalLf: tolerated, stripped for splitting, success, NOT recorded as stripped.
    let allow = IngestPolicy {
        record_len: 55,
        trailing_newline: TrailingNewline::AllowFinalLf,
        partial_record: PartialRecord::Reject,
    };
    let ing = ingest(&data, &allow).unwrap();
    assert_eq!((ing.spans.len(), ing.verdict), (2, ExitCode::Success));
    assert!(ing
        .file_audit_json()
        .contains("\"stripped_final_lf\":false"));
    // StripFinalLf: stripped AND recorded.
    let strip = IngestPolicy {
        record_len: 55,
        trailing_newline: TrailingNewline::StripFinalLf,
        partial_record: PartialRecord::Reject,
    };
    let ing = ingest(&data, &strip).unwrap();
    assert!(ing.stripped_final_lf);
    assert!(ing.file_audit_json().contains("\"stripped_final_lf\":true"));
}

#[test]
fn final_lf_not_silently_stripped() {
    // A final LF on an otherwise-exact file must be a *decision*, never silently dropped: under the
    // strict default it is an error, so the operator must choose a policy.
    let mut data = vec![b'x'; 55];
    data.push(b'\n');
    assert!(ingest(&data, &strict(55)).is_err());
}

#[test]
fn audit_is_byte_stable_and_manifest_shaped() {
    let data = vec![b'z'; 55 * 360];
    let a = ingest(&data, &strict(55)).unwrap().file_audit_json();
    let b = ingest(&data, &strict(55)).unwrap().file_audit_json();
    assert_eq!(a, b, "file audit must be byte-stable");
    assert!(a.contains("\"record_len\":55") && a.contains("\"record_count\":360"));
    assert!(a.contains("\"offsets\":{\"first\":0,\"last\":19745}")); // 359 * 55
    assert!(a.contains("\"input_len\":19800"));
}

#[test]
fn exit_codes_are_stable() {
    // Frozen numeric values — operators script against these.
    assert_eq!(ExitCode::Success.code(), 0);
    assert_eq!(ExitCode::DecodedWithEvidenceWarnings.code(), 1);
    assert_eq!(ExitCode::InvalidInputShape.code(), 2);
    assert_eq!(ExitCode::UnsupportedCobolSurface.code(), 3);
    assert_eq!(ExitCode::InternalInvariantFailure.code(), 4);
    assert_eq!(ExitCode::IoOrConfigError.code(), 5);
}

#[test]
fn ingest_matches_corpus_reconcile_path() {
    // The exact-length corpus file ingests to the same record count + offsets the reconcile path uses.
    let data = std::fs::read("recon/account/input.dat").unwrap();
    let ing = ingest(&data, &strict(55)).unwrap();
    assert_eq!(ing.spans.len(), 120);
    assert_eq!(ing.verdict, ExitCode::Success);
    // offsets are exactly index * record_len (what reconcile's data.chunks(55) walks).
    assert!(ing
        .spans
        .iter()
        .all(|s| s.offset == s.index * 55 && s.len == 55));
    assert_eq!(data.len() % 55, 0); // exact multiple -> no partial
}
