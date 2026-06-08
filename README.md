# kobold-data-shim

<img src="assets/kobold_data_shim.png" width="200">

[![crates.io](https://img.shields.io/crates/v/kobold-data-shim.svg)](https://crates.io/crates/kobold-data-shim) [![docs.rs](https://img.shields.io/docsrs/kobold-data-shim)](https://docs.rs/kobold-data-shim) ![license](https://img.shields.io/badge/license-Apache--2.0-blue) ![kernel](https://img.shields.io/badge/kernel-gnucobol--rs_(oracle--proven)-orange) ![courts](https://img.shields.io/badge/KOBOLD_courts-12-brightgreen) ![fail](https://img.shields.io/badge/unsupported-fails_closed-success)

**A verifiable COBOL record-decoding shim for data-migration pipelines.** Give it a copybook and a
raw record dump; it tells you — byte-exactly — *what that COBOL record actually meant*, by composing
the oracle-proven [`gnucobol-rs`](https://github.com/infinityabundance/gnucobol-rs) compatibility
courts. Fields outside the sealed subset **fail closed** (reported `unsupported`), never silently
guessed — the reconciliation signal that real migrations need.

> [!IMPORTANT]
> KOBOLD does not turn COBOL data into truth. It turns legacy bytes into **accountable evidence** under
> declared courts, profiles, custody, and refusals.

## The courts at a glance

| 🔒 court | proves (✅) | refuses (❌) |
|---|---|---|
| `RECON.1` | end-to-end fixed-record decode → byte-stable JSONL + audit | business truth |
| `RECON.2` | declared transform before/after bytes + audit delta | write-back · posting · ledger truth |
| `FILE.1` | fixed-record container ingest (offsets, exit codes) | GnuCOBOL file-I/O parity |
| `BANK.1` / `BANK.2` | declared-vs-observed totals · numeric role · DR/CR polarity | posting · ledger truth (sign ≠ polarity) |
| `DB2HOST.1` | null / truncation indicators (bytes preserved) | database truth without the indicator |
| `POSTING.1` | record-order custody + sha256 hash chain | ledger acceptance · settlement finality |
| `EXTRACT.PROFILE.1` | declared extraction provenance | extraction truth · copybook freshness |
| `BANK.RECONCILE.1` | generated operator reconciliation **view** | new evidence (it is a view) · match ≠ correctness |
| `PRIVACY.REDACTION.1` | declared redaction, hashes/provenance kept | anonymization · compliance · reversibility |
| `CORPUS.2` | hostile fixtures fail closed (none silently clean) | production representativeness |
| `PERF.1` | gated Rayon, byte-identical to scalar | production / parallel throughput |
| `OPERATOR.1` | explain · totals · dirty-mode (accountable fields) | silent coercion of dirty data |

```text
$ kobold-record-dump --copybook CUST.cpy --record dump.bin
FIELD                      OFF  SIZE CATEGORY      VALUE                    RAW
  CUST-ID                      0     3 numeric       42                       303432
  CUST-NAME                    3     4 alphanumeric  ANNA                     414e4e41
  CUST-BAL                     7     3 numeric       -12.34                   01234d
CUST                         0    10 group         (group)
```

Every numeric value is decoded by a court proven **byte-identical to GnuCOBOL 3.2's `libcob`** under
a differential sweep. The raw bytes travel alongside every field as the audit trail.

## Why it exists

Mainframe modernization on AWS routinely ingests VSAM/flat-file exports into S3 + Aurora/Redshift.
The single most error-prone step is *interpreting the legacy bytes*: packed decimal (COMP-3) signs,
implied decimal scale, `REDEFINES`, `OCCURS`, copybook `COPY`/`REPLACING`. A silent off-by-one in a
sign nibble or a scale is a financial discrepancy. This shim makes that step **verifiable**:

- it reuses the sealed [`gnucobol-rs`] courts — `GNURUST.2` MOVE bytes, `GNURUST.3/9` PIC (+P),
  `GNURUST.4/10` layout (+ODO), `GNURUST.5/6` COPY/REPLACING, `GNURUST.7/13` arithmetic, `GNURUST.8`
  VALUE, `GNURUST.11/12` LEVEL-88, `GNURUST.14` binary COMP/COMP-5/COMP-X, `GNURUST.15` cp500 EBCDIC
  text decode, `GNURUST.16` edited-picture decode, `GNURUST.17` cp500 zoned numeric, `GNURUST.18`
  COMP-6 unsigned packed — each proven against the GnuCOBOL oracle;
- it **fails closed** on anything outside those courts, so unsupported fields are surfaced for
  reconciliation rather than mis-decoded.

## Library API

```rust
use kobold_data_shim::decode;

let copybook = "01 CUST.\n05 CUST-ID PIC 9(3).\n05 CUST-BAL PIC S9(3)V99 COMP-3.";
let record = [b'0', b'4', b'2', 0x01, 0x23, 0x4d];
let fields = decode(copybook, &record).unwrap();
assert_eq!(fields.iter().find(|f| f.name == "CUST-BAL").unwrap().value, "-12.34");
```

`decode_with_resolver(copybook, record, resolver)` expands `COPY` statements via a caller-supplied
`CopyResolver` (the CLI resolves them from `--copydir`). `decode_all(copybook, data, record_len, resolver)` is a higher-level iterator yielding one decoded
record per fixed-length record. An optional `serde` feature (off by default) derives `Serialize` on the
decoded types (values stay strings — never floats). `decode_record(...)` additionally evaluates
**LEVEL-88 condition names** (`gnucobol-rs`' `eval_88`), so a decoded record carries both fields and
condition truths.

## Operator trust layer (KOBOLD.OPERATOR.1)

Every decoded field is **accountable** — point at a value and ask *why*:

```sh
kobold-recon explain account.cpy input.dat ACCOUNT-RECORD.BALANCE --record 0 --copydir .
kobold-recon totals  account.cpy input.dat --record-len 42 --copydir .
kobold-recon decode  account.cpy input.dat --record-len 42 --dirty-mode strict
```

`explain` returns source provenance (`copybook:line`), offset/size, usage/pic, raw bytes, decoded
value, the **sealed courts** that produced it, dependent LEVEL-88s, the record hash, the explicit
non-claims, and a stale-copybook risk note. `totals` gives control totals (record count, per-field
numeric sums, condition counts, invalid/unsupported). `decode --dirty-mode` preserves dirty bytes as
evidence (or rejects in `strict`) — **never coerces**. Duplicate JSON keys are refused, not clobbered.

## End-to-end reconciliation (`KOBOLD.RECON.1`)

`kobold_data_shim::recon::reconcile` (and the `kobold-recon` CLI) decode a fixed-record file into
**byte-stable** JSONL + a `kobold-recon-receipt-v1` audit + an explicit `unsupported.json`, using only
the sealed `gnucobol-rs` courts:

```json
{"record_index":0,
 "fields":{"ACCOUNT-ID":"100000","STATUS-CODE":"A","BALANCE":"5459318.55","CUST-NAME":"CUSTOMER 0000","CUST-TIER":"G","BRANCH-NO":"5895","RISK-SCORE":"796453","INTERNAL-ID":"900453110"},
 "conditions":{"ACTIVE":true,"CLOSED":false,"DELINQUENT":false,"CUST-GOLD":true},
 "audit":{"raw_offset":0,"raw_len":42,"record_sha256":"…"}}
```

(`BRANCH-NO` is `COMP`, `RISK-SCORE` is `COMP-X`, `INTERNAL-ID` is `COMP-5` — binary fields decoded
from raw storage, not text-converted.)

A committed corpus of 3 fixture families (account / payroll / insurance, 360 records, with `COPY ... REPLACING`, COMP-3, **COMP/COMP-5/COMP-X binary**, **COMP-6 unsigned packed**, **cp500 EBCDIC text + zoned numeric DISPLAY** (explicit `--encoding`; `REGION-CODE`/`LIMIT-AMT`/`RISK-PERCENT`; binary/packed passthrough), and alpha + numeric-range LEVEL-88s) is in [`recon/`](recon/), with a
sealing receipt at [`recon/RECEIPT-KOBOLD-RECON-1.md`](recon/RECEIPT-KOBOLD-RECON-1.md). The output is
proven byte-stable across runs and CLI == library; `unsupported.json` lists anything outside the
sealed courts — never a silent fallback. The inverse direction (`SET condition TO TRUE` →
bytes → `eval_88` true) is the [`recon/condition-set/`](recon/condition-set/) fixture.

## Transformed-record reconciliation (`KOBOLD.RECON.2`)

A **declared** transform — a *named sealed court* — takes input bytes to output bytes, and both states
decode and audit:

```
input record ─► declared transform (SET 88 TRUE / ADD / SUBTRACT) ─► output record ─► before/after decode + audit delta
```

`reconcile_transform` proves only **read truth** and **transform truth** (`scope: declared sealed
transform only`); **write-back, posting, ledger, and business truth stay `claimed: false`**. Undeclared
targets fail closed, and nothing outside the declared field is touched. *Read truth ≠ transform truth ≠
write-back truth ≠ business truth* — this is reconciliation evidence, **not** production write-back, file
rewrite parity, or Procedure Division execution.

## Fixed-record container ingest (`KOBOLD.FILE.1`)

Before decoding fields, split a raw byte stream into records with **defensible offsets and failure
modes** — ingest reliability, not GnuCOBOL file I/O parity:

```
kobold-recon ingest data.bin --record-len 55 \
    [--trailing-newline reject|allow-final-lf|strip-final-lf] \
    [--partial-record reject|evidence]
```

It prints a byte-stable `kobold-file-ingest-v1` audit (`input_sha256`, `input_len`, `record_len`,
`record_count`, policies, `offsets.{first,last}`, verdict) and exits with a **stable code**. The default
is **strict**: a partial trailing record or an unexpected final newline is rejected — never silently
absorbed, resynced, or repaired; the encoding is always explicit.

| exit | meaning |
|------|---------|
| 0 | success |
| 1 | decoded with evidence warnings (preserved partial record / dirty fields) |
| 2 | invalid input shape (record_len 0, partial under strict, unexpected trailing LF) |
| 3 | unsupported COBOL surface |
| 4 | internal invariant failure |
| 5 | I/O or configuration error |

> [!IMPORTANT]
> **Doctrine.** KOBOLD.FILE.1 admits only explicit fixed-record container ingest: bytes are split by a
> caller-declared record length with stable offsets, policies, manifests, and exit codes, while GnuCOBOL
> file I/O, indexed files, line-sequential runtime behavior, auto-resynchronization, and silent repair
> remain outside the claim.

## Banking control totals (`KOBOLD.BANK.1`)

Header/detail/trailer banking files, reconciled under a **declared** profile — *the court's job is to stop
banking data being over-interpreted*:

- route records by a declared discriminator (H/D/T) to per-variant copybooks; **unknown type fails closed**;
- reconcile the trailer's **declared** control totals (count / debit / credit) against KOBOLD-**observed**
  totals — a `KOBOLD-BANK-CONTROL-MISMATCH` finding (SARIF-shaped) on any discrepancy;
- debit/credit polarity comes only from the **declared** DR/CR field, **never** a numeric sign.

It composes a **declared accounting profile** (`KOBOLD.BANK.2` / `kobold-accounting-profile-v1`): each
field is given a numeric **role** (amount / rate / identifier / code / sequence / count) and **only
`amount` fields are summed** — a rate or an account-id is numeric but never money. Polarity comes only
from a declared source field + value tables; a **negative amount with a declared `D` is still a debit**
(sign is not polarity), and an unknown polarity value fails closed.

It emits a `kobold-banking-forensic-casefile-v1` with truth **layers**: byte truth and record truth are
proven; **posting, ledger, and business truth are explicitly `claimed: false`** and require declared
profiles. *A balanced file is not a correct file; a trailer match is not ledger acceptance.* The full
banking refusal set is in the registry (`NEG.BANKING.*`).

## Db2 host-variable null indicators (`KOBOLD.DB2HOST.1`)

A field can decode perfectly and still be **semantically NULL** at the database boundary. This court
applies a **declared** indicator manifest — a `PIC S9(4) COMP-5` indicator paired with a value field:
**negative → null, zero → present, positive → truncation evidence**. The **decoded bytes are always
preserved**; a missing or wrong-usage indicator **fails closed**; a field with no declared indicator gets
**no** null-state claim. The casefile keeps `byte_truth`/`record_truth` separate from `database_truth`
(`claimed: false`) — *the host value is not the database value without its indicator*.

## Posting-unit custody (`KOBOLD.POSTING.1`)

A **declared** posting-unit manifest binds the banking spine into one custody record — *which exact
records, in which order, were reconciled* — without claiming the unit was posted, accepted, or settled.
`posting_manifest` records the batch identity, business date, extract metadata, a **sha256 hash chain over
record order** (reordering changes the chain), the sequence min/max/duplicates (and **gaps only when the
profile declares the sequence contiguous**), and duplicate transaction ids. `posting_truth` /
`ledger_truth` / `business_truth` stay `claimed: false` — *a sequenced, de-duplicated batch is custody
evidence, not ledger acceptance or settlement finality.*

## Extraction provenance + copybook freshness (`KOBOLD.EXTRACT.PROFILE.1`)

Every real migration depends on *how the bytes were obtained*. `extract_manifest` records the **declared**
provenance — file organization, extract method, record-length source, copybook source, any code-set
conversion done **before** KOBOLD, source-system cutoff, operator assumptions — bound to the data +
copybook sha256. It **refuses extraction truth** and holds **copybook freshness as a permanent
uncertainty** (`copybook_freshness: {claimed:false, risk: "a stale copybook may decode bytes plausibly
wrong"}`). *KOBOLD proves decoded extracted bytes — not that the extraction or the copybook is production
truth.*

## Banking reconciliation view (`KOBOLD.BANK.RECONCILE.1`)

An **opinionated generated operator view** that lets an operator read the banking evidence in one report —
*Did this batch reconcile under the declared profile? What failed? What was refused? What should I not
conclude?* `bank_reconcile_report` assembles **only from existing court structs** (BANK.1/2 summary,
POSTING.1 custody, DB2HOST.1 indicators, PRIVACY redaction counts): declared-vs-observed count/debit/credit
+ matched/mismatch verdict, sequence min/max/gaps/duplicates + `last_chain_hash`, DB2 null/truncation
counts, dirty/unsupported counts, redaction counts, and the **refused truth layers** — emitting json + md +
an aggregated SARIF of the *existing* findings. It **introduces no new evidence** (`introduces_new_evidence:
false`) and a match proves equality to the **declared** totals, *not* posting, ledger, settlement,
account-balance, or business truth.

## Evidence-preserving redaction (`KOBOLD.PRIVACY.REDACTION.1`)

Before real files enter the pipeline, privacy is a **court**, not a footnote. `redact_record` applies a
**declared** field policy: a value is **withheld** (`redact_value_keep_hash` / `…_and_raw_keep_hashes`) or
**tokenized** (`tokenize_deterministic`, scope-stable, never reversible), while `value_sha256`,
`raw_sha256`, offset/size, copybook provenance, and court identity stay visible so the evidence remains
auditable. An **unlisted field fails closed** under a `deny_unlisted` policy, and `public_output_claim` is
always `false`. *It claims no anonymization, regulatory compliance, reversibility, or safe public release.*

## Adversarial corpus (`KOBOLD.CORPUS.2`)

Hostile and banking-shaped fixtures that prove the court **refuses plausible wrongness** — each must
produce a named fail-closed finding, and **none may silently decode as clean** (`tests/corpus2.rs`,
manifest in [`recon/corpus2-manifest.json`](recon/corpus2-manifest.json)): short/partial records, invalid
packed nibbles, signed COMP-6, trailer mismatch, unknown record type, unknown DR/CR polarity, DB2 null /
truncation / wrong-usage indicators, and undeclared transform targets. Synthetic only — *not* production
representativeness or business correctness.

## Gated parallelism (`KOBOLD.PERF.1`)

`reconcile_encoded_parallel` (behind the **off-by-default `rayon` feature**) decodes records with
record-level Rayon, emitting **byte-identical** evidence to scalar [`reconcile_encoded`] — same JSONL,
audit, unsupported ledger, `decode_output_sha256`, and downstream posting hash chain. The program is
parsed once and the parallel path captures only the `Sync` layout (no resolver), with an order-preserving
`collect`. *Performance is a derived property of preserved evidence, not a separate semantic authority* —
proven in `tests/perf1.rs` (scalar == rayon across n=1…999).

## AWS reference architecture (S3 → verified records)

```text
Mainframe VSAM/flat export
        │  (AWS Transfer Family / Direct Connect; cp500 EBCDIC text decoded in-shim via --encoding)
        ▼
   S3 landing zone ──(ObjectCreated)──► Lambda / AWS Batch / Glue
        │                                   │ kobold-data-shim:
        │                                   │   COPY-expand copybook → lay_out → decode (cob_move/Decimal)
        │                                   │   emit Parquet/JSON  +  per-record audit (raw_hex, court, unsupported)
        ▼                                   ▼
   raw retained                     S3 (Parquet) → Athena/Glue        Aurora (txn)
                                    + S3 (audit/parity receipts) → reconciliation jobs
```

The serverless packaging (Lambda layer / container sidecar) and a benchmark harness live in sibling
repos: [`kobold-lambda-layer`](https://github.com/infinityabundance/kobold-lambda-layer) and
[`kobold-bench`](https://github.com/infinityabundance/kobold-bench).

## Parity proofs as a migration artifact

Because the kernel is oracle-proven, a migration can ship **evidence**, not assertions: the
`gnucobol-rs` differential sweeps (`FAIL=0`), Kani proofs, fuzz runs, and the machine-readable
non-claims become part of the compliance package (SOX/audits). This shim adds the per-record audit
trail (raw bytes + the court each value came from + any `unsupported` field).

## Scope & honest edges

Claims are **layered**: a court is *sealed in `gnucobol-rs`* (proven byte-exact vs the oracle) and
*composed into the KOBOLD corpus* (exercised end-to-end here). Both are stated, never conflated.

- **Composed in the KOBOLD reconciliation corpus (byte-stable, CLI == library):**
  DISPLAY / COMP-3 / **COMP / COMP-5 / COMP-X**, `PIC` widths + record offsets, fixed `OCCURS` /
  `REDEFINES (≤ target)` / `OCCURS DEPENDING ON` physical-max / `FILLER`, `COPY [REPLACING]`,
  **cp500 alphanumeric DISPLAY text + numeric DISPLAY/zoned sign with explicit `--encoding`** (audit
  `numeric_display` block; binary/packed pass through untouched),
  LEVEL-88 condition predicates, **edited-picture decode** (`GNURUST.16` — presentation string in JSON,
  numeric in the audit `edited` block), and **cp500 numeric DISPLAY / EBCDIC zoned sign** (`GNURUST.17`).
- **Fails closed / future (surfaced for reconciliation, never guessed):** signed COMP-6, COMP-6 arithmetic, malformed COMP-6 bytes, floats, `SET ... TO FALSE` / FALSE clause, mixed/auto-detected
  encodings, and any **business truth beyond what the copybook + sealed courts prove**.
- **Host:** little-endian ASCII (matches the `gnucobol-rs` sealed claim).

Decoded fields are **accountable**: `kobold-recon explain` shows provenance, raw bytes, the sealed
courts used, and the stale-copybook risk; `kobold-recon totals` gives control totals; `decode
--dirty-mode` preserves dirty bytes as evidence (see the operator section above).

See [`docs/enterprise-readiness.md`](docs/enterprise-readiness.md) for the SBOM / CVE / SLA stance.

## License

This crate's source is **Apache-2.0** (`LICENSE`). It links `gnucobol-rs`, which is
**LGPL-3.0-or-later** — see [`NOTICE`](NOTICE) for what that requires when you distribute a binary
(relink-ability + notice).
