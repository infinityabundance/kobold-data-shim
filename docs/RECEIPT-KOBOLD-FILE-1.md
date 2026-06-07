# RECEIPT — KOBOLD.FILE.1: fixed-record container ingest

**Claim.** `kobold_data_shim::file::ingest` splits a raw byte stream into records of an explicit
caller-declared length, with stable **true** offsets, named trailing-newline / partial-record policies,
a byte-stable file audit manifest, and **stable documented exit codes**. Strict by default.

**Doctrine.** KOBOLD.FILE.1 admits only explicit fixed-record container ingest: bytes are split by a
caller-declared record length with stable offsets, policies, manifests, and exit codes, while GnuCOBOL
file I/O, indexed files, line-sequential runtime behavior, auto-resynchronization, and silent repair
remain outside the claim.

## Replayable evidence (`cargo test --test file`, 10 tests)

| Acceptance | Test |
|------------|------|
| exact multiple → records with `offset == index*record_len` | `exact_multiple_splits_with_true_offsets` |
| empty file → 0 records, success, offsets `{first:-1,last:-1}` | `empty_file_is_zero_records_success` |
| `record_len = 0` → config error (exit 5) | `record_len_zero_is_config_error` |
| short trailing record → strict reject (exit 2) | `short_trailing_record_fails_strict` |
| short trailing record → evidence preserves it (exit 1) | `short_trailing_record_preserved_in_evidence` |
| final LF policy-controlled, never silent | `trailing_newline_policy_is_explicit`, `final_lf_not_silently_stripped` |
| file audit byte-stable + manifest-shaped | `audit_is_byte_stable_and_manifest_shaped` |
| exit codes frozen 0..5 | `exit_codes_are_stable` |
| ingest offsets == reconcile corpus path | `ingest_matches_corpus_reconcile_path` |

The file audit (`kobold-file-ingest-v1`) is the **replayable receipt**: identical input + policy → byte-
identical manifest (proven by `audit_is_byte_stable_and_manifest_shaped`).

## Exit codes (stable)

`0` success · `1` decoded-with-evidence-warnings · `2` invalid-input-shape · `3` unsupported-cobol-surface
· `4` internal-invariant-failure · `5` io-or-config-error.

## Non-claims

GnuCOBOL file organization, indexed/relative I/O, line-sequential runtime parity, auto-resynchronization
on a length mismatch, encoding auto-detection, and any silent repair of dirty bytes.
