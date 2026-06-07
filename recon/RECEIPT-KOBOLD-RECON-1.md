# RECEIPT-KOBOLD-RECON-1 — sealed: end-to-end fixed-record reconciliation

**Campaign KOBOLD.RECON.1.** Goal: prove `kobold-data-shim` can decode realistic fixed-record COBOL
data into stable JSON + audit receipts using **only the sealed `gnucobol-rs` courts**, including
LEVEL-88 condition predicates, without guessing unsupported fields.

## Doctrine (the one sentence)

> KOBOLD.RECON.1 admits only end-to-end fixed-record reconciliation over sealed `gnucobol-rs` courts:
> copybook expansion, layout, decoded field bytes, LEVEL-88 predicates, and audit hashes are proven
> together, while unsupported COBOL surfaces are emitted as explicit non-decoded evidence rather than
> guessed.

## Claim (exact)

`kobold_data_shim::recon::reconcile` decodes a fixed-record buffer against a copybook into:
- **`expected.jsonl`** — one JSON object per record: `fields` (decoded values), `conditions`
  (LEVEL-88 truths from `eval_88` only), and per-record `audit` (raw offset/len + `record_sha256`);
- **`audit.json`** — a `kobold-recon-receipt-v1` with input/copybook/expanded-copybook/layout/output
  SHA-256s, record/field/condition/unsupported counts, and the pinned `gnucobol-rs` +
  `kobold-data-shim` versions;
- **`unsupported.json`** — the explicit list of any field/condition outside the sealed courts.

Every value comes from a sealed court: `COPY`/`REPLACING` expansion (`GNURUST.5`/`6`), `PIC`+layout
(`GNURUST.3`/`4`, incl. ODO physical-max `GNURUST.10`), packed/zoned/display decode (`GNURUST.2`),
and LEVEL-88 predicates (`GNURUST.11`). The inverse `SET ... TO TRUE` (`GNURUST.12`) is exercised by
the `condition-set` mutation fixture: condition name → parent bytes → decoded condition true.

## Corpus

| Fixture | Records | Fields | Conditions | Notable |
|---------|--------:|-------:|-----------:|---------|
| `account-status-v1` | 120 | 8 | 4 | `COPY ACCTNAME REPLACING ==:P:== BY ==CUST==`; alpha 88s (`ACTIVE`/`CLOSED`/`DELINQUENT`/`CUST-GOLD`); COMP-3 `BALANCE` |
| `payroll-v1` | 120 | 7 | 2 | alpha 88 (`SALARIED`/`HOURLY`); two COMP-3 (`GROSS-PAY`/`DEDUCTIONS`) |
| `insurance-policy-v1` | 120 | 6 | 2 | **numeric-range** 88s (`LOW-RISK VALUE 1 THRU 3`, `HIGH-RISK 7 THRU 9`); COMP-3 `PREMIUM` |

**360 records total**, 0 unsupported. **KOBOLD.DATA.2** added COMP/COMP-5/COMP-X **binary fields** to
all three families (decoded via `GNURUST.14`; the audit records the endian assumption), proving the
binary court composes end-to-end. **KOBOLD.DATA.3** added an `account-cp500` EBCDIC family
(120 records): cp500 text decoded via `GNURUST.15`, LEVEL-88 on decoded values, and COMP/COMP-3/
COMP-5/COMP-X passed through as raw storage (proven untouched by the EBCDIC layer). Each family's `receipt.md` carries its audit hashes.

## Evidence (acceptance gate)

| Check | Result |
|-------|--------|
| 3 fixture families, ≥ 300 records | **360** (3 × 120) |
| JSON output byte-stable across two runs | ✓ (`tests/recon.rs::corpus_is_byte_stable_and_golden`) |
| Audit receipts byte-stable across two runs | ✓ |
| CLI path == library path | ✓ (committed goldens were written by `kobold-recon`; the test compares them to `reconcile()`) |
| Unsupported fields explicitly listed, no silent fallback | ✓ (`unsupported.json`; `unsupported_count = 0`) |
| Conditions emitted from `eval_88` only | ✓ (`conditions_come_from_eval_88_only`) |
| Condition-set fixture self-checks through `eval_88` | ✓ (`condition_set_round_trips`; `recon/condition-set/`) |
| `cargo test` / `clippy -D warnings` | clean |

## Non-claims (fail closed)

Unsupported PIC/usage, edited pictures, P-scaled or VALUE+P combinations, ODO **logical**
interpretation, condition expressions, `SET ... TO FALSE`, unknown encodings, record-length
mismatch, and copybook-expansion ambiguity are emitted as explicit non-decoded evidence
(`unsupported.json` / typed errors), never guessed. EBCDIC, `COMP`/`COMP-5`/`COMP-X`, and
line-sequential containers are out of scope for this first reconciliation court (raw fixed-record
ASCII only).
