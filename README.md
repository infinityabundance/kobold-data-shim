# kobold-data-shim

<img src="assets/kobold_data_shim.png" width="200">


**A verifiable COBOL record-decoding shim for data-migration pipelines.** Give it a copybook and a
raw record dump; it tells you — byte-exactly — *what that COBOL record actually meant*, by composing
the oracle-proven [`gnucobol-rs`](https://github.com/infinityabundance/gnucobol-rs) compatibility
courts. Fields outside the sealed subset **fail closed** (reported `unsupported`), never silently
guessed — the reconciliation signal that real migrations need.

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

- it reuses the sealed [`gnucobol-rs`] courts (`GNURUST.2` MOVE bytes, `GNURUST.3` PIC, `GNURUST.4`
  layout, `GNURUST.5/6` COPY/REPLACING, `GNURUST.7` arithmetic), each proven against the GnuCOBOL
  oracle;
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
`CopyResolver` (the CLI resolves them from `--copydir`). `decode_record(...)` additionally evaluates
**LEVEL-88 condition names** (`gnucobol-rs`' `eval_88`), so a decoded record carries both fields and
condition truths.

## End-to-end reconciliation (`KOBOLD.RECON.1`)

`kobold_data_shim::recon::reconcile` (and the `kobold-recon` CLI) decode a fixed-record file into
**byte-stable** JSONL + a `kobold-recon-receipt-v1` audit + an explicit `unsupported.json`, using only
the sealed `gnucobol-rs` courts:

```json
{"record_index":0,
 "fields":{"ACCOUNT-ID":"100000","STATUS-CODE":"A","BALANCE":"5459318.55","CUST-NAME":"CUSTOMER 0000","CUST-TIER":"G"},
 "conditions":{"ACTIVE":true,"CLOSED":false,"DELINQUENT":false,"CUST-GOLD":true},
 "audit":{"raw_offset":0,"raw_len":33,"record_sha256":"…"}}
```

A committed corpus of 3 fixture families (account / payroll / insurance, 360 records, with `COPY ... REPLACING`, COMP-3, **COMP/COMP-5/COMP-X binary**, and alpha + numeric-range LEVEL-88s) is in [`recon/`](recon/), with a
sealing receipt at [`recon/RECEIPT-KOBOLD-RECON-1.md`](recon/RECEIPT-KOBOLD-RECON-1.md). The output is
proven byte-stable across runs and CLI == library; `unsupported.json` lists anything outside the
sealed courts — never a silent fallback. The inverse direction (`SET condition TO TRUE` →
bytes → `eval_88` true) is the [`recon/condition-set/`](recon/condition-set/) fixture.

## AWS reference architecture (S3 → verified records)

```text
Mainframe VSAM/flat export
        │  (AWS Transfer Family / Direct Connect; EBCDIC→ASCII converted early)
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

- **Sealed (byte-exact):** COMP-3 / zoned / display numerics, `PIC` widths, record offsets, fixed
  `OCCURS`, `REDEFINES (≤ target)`, `FILLER`, `COPY [REPLACING]`.
- **Fails closed (surfaced for reconciliation):** edited pictures, `P` scaling, `OCCURS DEPENDING
  ON`, binary/`COMP`/float, EBCDIC-host sign mode. Convert EBCDIC→ASCII *before* this shim until an
  EBCDIC court lands.
- **Host:** little-endian ASCII (matches the `gnucobol-rs` sealed claim).

See [`docs/enterprise-readiness.md`](docs/enterprise-readiness.md) for the SBOM / CVE / SLA stance.

## License

This crate's source is **Apache-2.0** (`LICENSE`). It links `gnucobol-rs`, which is
**LGPL-3.0-or-later** — see [`NOTICE`](NOTICE) for what that requires when you distribute a binary
(relink-ability + notice).
