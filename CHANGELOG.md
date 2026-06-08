# Changelog

## 0.7.1

- **KOBOLD.PILOT-PACKET.1 — hash-bound pilot evidence packet.** `pilot_packet` bundles a pilot run's existing
  court artifacts (EXTRACT.PROFILE.1, redaction policy, BANK.RECONCILE.1, DIFF.1, TOOLING.EXPORT.1, SCALE
  receipt, DSSE verification) each pinned by sha256, plus the copybook sha, an operator review checklist, and
  a review-notes HASH (notes never embedded). `derived_view:true`, `creates_new_truth:false`; a changed
  artifact changes the packet; missing required artifacts are flagged. Pilot evidence only — not
  certification/compliance/production-approval/customer-acceptance. +9 NEG.PILOT.*. tests/pilot.rs (3).

## 0.7.0

- **NIST-STYLE-FIXTURE-FORMAT.1 — named replayable fixture format.** `kobold-fixture-v1` declares input
  bytes + copybook/profile + expected verdict/findings/non-claims + input hashes; `replay_fixture` runs the
  named court for real and compares actual-vs-expected (a wrong expectation fails). Negative fixtures are
  first-class; a risk-bearing fixture without non-claims is rejected; `nist_conformance` is hard-false. +6
  NEG.FIXTURE.*. tests/fixture.rs (4).

- **KOBOLD.BANK.RECONCILE.1 — source-casefile sha binding (the TRUST.5 follow-through).** The reconciliation
  report now carries a `source_evidence` block pinning each source court casefile by sha256 (BANK.1/BANK.2 +
  POSTING.1 + DB2HOST.1 + any extra EXTRACT.PROFILE.1/PRIVACY.REDACTION.1/DIFF.1), with `derived_view:true,
  creates_new_truth:false`. A changed source casefile changes the report hash; +2 NEG.BANK_RECONCILE.SOURCE_
  CASEFILE_REQUIRED/SOURCE_HASH_MISMATCH. **BREAKING (hence the minor):** `BankReconcileInputs` gains a
  required `extra_sources: &[(&str, &str)]` field. tests/bank_reconcile.rs (+1: source binding + freshness).

## 0.6.6

- **KOBOLD.TOOLING.EXPORT.1 — generated evidence export for downstream tools.** `tooling_export` maps each
  decoded field to copybook provenance (path/line), PIC/USAGE, offset/length, decoded value OR redaction
  status (no cleartext for a redacted field), `raw_sha256`, the sealed court ids that produced it, the
  witness `dialect_profile_id`, and per-field non-claims. `introduces_new_evidence:false`; refuses to be an
  LSP/IDE/parser/source-of-truth. +6 NEG.TOOLING.*. tests/tooling.rs (3).

## 0.6.5

- **KOBOLD.CURRENCY.PROFILE.1 — declared currency/amount-profile evidence.** `currency_validate` checks a
  field declared `role=amount` against an explicit `declared_scale` (observed implied scale vs declared) with
  an optional currency-code field preserved as EVIDENCE (never legal tender). A non-amount role is not
  admitted as money; the sign is not polarity (BANK.2 owns DR/CR). Money/FX/rounding/legal-tender/business
  meaning stay `claimed:false`. +8 NEG.CURRENCY.*. tests/currency.rs (3). Closes the value-profile trio
  (SENTINEL -> DATE -> CURRENCY).

## 0.6.4

- **KOBOLD.DIFF.1 — structural diff against a declared expected artifact.** `diff_artifacts(actual,
  expected, target)` compares KOBOLD output to a DECLARED target over selected dimensions (field values,
  audit hashes, finding-id set, control totals): exact match passes; field/missing/extra/finding-set/
  control-total/hash drift each emit a named SARIF finding (`KOBOLD-DIFF-*`). The target declares an
  `oracle_status` (default `not_oracle`); a match proves equality to the target, NOT oracle authority,
  business truth, ledger acceptance, settlement finality, or customer approval (a non-oracle match emits
  `KOBOLD-DIFF-TARGET-NOT-ORACLE`). Deterministic. +6 NEG.DIFF.*. tests/diff.rs (6).
- **KOBOLD.PERF.2 — deterministic multithreaded pipeline + per-stage profiling.** `reconcile_profile`
  returns a `StageProfile` (parse / per-record / aggregate ns) without changing the emitted bytes; it
  identifies the per-record stage as the bottleneck (the part PERF.1's Rayon already parallelizes
  byte-identically), with aggregation kept serial/ordered. Full custody workload (reconcile + POSTING.1 +
  PRIVACY.REDACTION.1) proven byte-identical scalar vs Rayon. +6 NEG.PERF.*. tests/perf2.rs.
- **KOBOLD.LAYOUT.REDEFINES.2 — overlapping REDEFINES view manifest.** `redefines_manifest` records each
  `REDEFINES` storage region (offset/length/`raw_sha256`) with every overlapping view decoded independently
  over the SAME bytes; `active_view` is `claimed:false` by default and admitted only by a declared
  discriminator (unknown -> false). Layout-valid byte views, never inferred business meaning. +5
  NEG.REDEFINES.*. tests/redefines.rs (4).
- **KOBOLD.SENTINEL.PROFILE.1 — declared sentinel-marker evidence.** `sentinel_scan` records which DECLARED
  markers (by `raw_hex` or `decoded_value`) match a named field, as evidence only; undeclared sentinel-
  looking values are never inferred (`undeclared_inference:false`). nullness/date/missingness/business
  status/account state stay `claimed:false` (nullness needs DB2HOST.1; dates need DATE.PROFILE). +7
  NEG.SENTINEL.*. tests/sentinel.rs (3).
- **KOBOLD.DATE.PROFILE.1 — declared date-format evidence.** `date_validate` checks a named field against an
  explicit format (`YYYYMMDD`/`YYDDD`) on its RAW digit string (leading zeros preserved); sentinels are
  delegated to SENTINEL.PROFILE.1; the strongest claim is `format_valid_only`. PIC shape alone gets no date
  claim; business/settlement/maturity/Y2K/currentness/arithmetic meaning stay `claimed:false`. +8
  NEG.DATE.*. tests/date_profile.rs (3).

## [0.6.3]
- **KOBOLD.DATA.6 — COMP-6 composed.** One unsigned COMP-6 field per family (ACCOUNT-SEQUENCE `9(8)`,
  PAYROLL-BATCH-NO `9(6)`, POLICY-CODE `9(10)`; +ACCT-SEQ-C6 in the cp500 corpus) decodes via
  gnucobol-rs `GNURUST.18`. Audit gains a `comp6` block (`claim:GNURUST.18`, `domain:comp6-unsigned-packed`).
  **Signed COMP-6 fails closed** (GnuCOBOL converts `S9(n) COMP-6` to COMP-3 — never silently decoded).
  The cp500 passthrough test proves EBCDIC decode never touches COMP-6 bytes. Operator dirty-data check
  validates COMP-6 (all-digit nibbles, no sign nibble). 0 unsupported, byte-stable, CLI==lib. Requires
  gnucobol-rs ^0.7. Additive composition, no shim API change -> patch.
- **KOBOLD.FILE.1 — fixed-record container ingest discipline.** New `file` module + `ingest` CLI
  subcommand: split a raw stream into records of an explicit `--record-len`, with stable true offsets,
  a byte-stable `kobold-file-ingest-v1` file audit, and **named policies** (`--trailing-newline
  reject|allow-final-lf|strip-final-lf`, `--partial-record reject|evidence`). **Strict by default**: a
  partial trailing record or unexpected final newline is rejected, never silently absorbed. Stable
  documented exit codes (0 success / 1 evidence-warnings / 2 invalid-shape / 3 unsupported-surface /
  4 internal / 5 io-or-config). This is KOBOLD ingest reliability, NOT GnuCOBOL file I/O parity.
- **KOBOLD.BANK.1 — banking header/detail/trailer court (declared-vs-observed control totals).** New
  `banking` module: route fixed records by a **declared** discriminator (H/D/T) to per-variant copybooks,
  then reconcile the trailer's **declared** control totals (count/debit/credit) against KOBOLD-**observed**
  totals. Debit/credit polarity comes only from the **declared** DR/CR field, never a numeric sign. A
  balanced file reconciles (exit 0); a tampered trailer fails with a `KOBOLD-BANK-CONTROL-MISMATCH`
  finding (exit 1); an unknown record type fails closed. Emits a `kobold-banking-forensic-casefile-v1`
  with truth LAYERS — byte/record truth proven, **posting/ledger/business truth explicitly unclaimed**.
  Doctrine: a balanced file is not a correct file; a trailer match is not ledger acceptance. tests/
  banking.rs (3) + recon/banking H/D/T corpus.
- **KOBOLD.BANK.2 / ACCOUNTING.PROFILE.1 — declared accounting profile.** Generalises BANK.1's hard-coded
  DR/CR into a reusable `kobold-accounting-profile-v1`: declared numeric **roles** (amount/rate/identifier/
  code/sequence/count) so **only `amount` fields are summed** (a rate or account-id is never money), and a
  declared **polarity profile** (`amount_field` + `source_field` + debit/credit value tables). Posting side
  is taken ONLY from the declared source — **a negative amount with declared `D` is still a debit** (sign
  is not polarity), CR/DB presentation and field-name heuristics are never used, and an unknown polarity
  value fails closed. The casefile embeds the `accounting_profile` (with `numeric_sign_policy:not_polarity`).
  tests/banking.rs now 5 (incl. negative-D-still-debit, rate-not-summed, unknown-fails-closed).
- **KOBOLD.DB2HOST.1 — declared Db2 host-variable null/truncation indicator manifest.** A decoded field
  is marked `semantic_null` / `truncation_evidence` ONLY via a declared `S9(4) COMP-5` indicator pairing
  (negative=null, zero=present, positive=truncation). Decoded bytes are always preserved; a missing or
  wrong-usage indicator fails closed; a field with no declared indicator gets no null-state claim. Emits a
  `db2_host` audit block + a casefile keeping `byte_truth`/`record_truth` separate from `database_truth`
  (`claimed:false`). NOT SQL execution / precompiler / SQLCA / DBRM-package / database truth. tests/
  db2host.rs (5).
- **KOBOLD.RECON.2 — declared transformed-record reconciliation.** A named **sealed** transform
  (`SET 88 TRUE` = GNURUST.12, `ADD`/`SUBTRACT` = GNURUST.7) takes input bytes → output bytes; both
  decode; an audit delta is produced; replay is byte-stable. Casefile splits truth layers — `read_truth`
  + `transform_truth` (scope: declared sealed transform only) claimed; **`write_back_truth` /
  `posting_truth` / `ledger_truth` / `business_truth` claimed:false**. Undeclared targets fail closed;
  nothing outside the declared field is touched. NOT Procedure Division / production write-back / file
  rewrite parity / ledger acceptance / business truth. tests/recon2.rs (4).
- **KOBOLD.CORPUS.2 — adversarial / banking-shaped corpus.** Hostile fixtures across 5 buckets (file/
  container, storage, banking, database, transform) each produce an expected fail-closed finding; none
  silently decodes as clean. Proves: short/partial-record handling, invalid packed nibble (dirty
  evidence), signed COMP-6 (unsupported), trailer mismatch, unknown record type/polarity, DB2 null/
  truncation/wrong-usage indicators, undeclared transform targets, and identifier record-truth (value is
  the numeric, raw_hex preserves the original digits). tests/corpus2.rs + recon/corpus2-manifest.json.
- **KOBOLD.POSTING.1 — declared posting-unit custody manifest.** Binds the banking spine (BANK.1/BANK.2/
  DB2HOST.1) into one custody record: batch identity, business date, extract metadata, a **sha256 hash
  chain over record ORDER** (reordering changes the chain), sequence min/max/duplicates/(gaps only when
  `sequence_contiguous` is declared), and duplicate transaction ids. `posting_truth`/`ledger_truth`/
  `business_truth` claimed:false. +5 NEG.POSTING.*. tests/posting.rs (6). NOT ledger acceptance /
  settlement finality / account balance / business truth.
- **KOBOLD.EXTRACT.PROFILE.1 (CUSTODY.1) — declared extraction provenance + copybook freshness.**
  `extract_manifest` records the declared provenance (file organization, extract method, record-length
  source, copybook source, pre-KOBOLD code-set conversion, source-system cutoff, operator assumptions)
  bound to the data + copybook sha256. **Refuses extraction truth**; **copybook freshness is `claimed:false`**
  (hash + provenance evidence only; risk: a stale copybook decodes plausibly wrong). +6 NEG.EXTRACT.* /
  NEG.COPYBOOK.STALE / NEG.CODESET.*. tests/extract.rs (2).
- **KOBOLD.PRIVACY.REDACTION.1 — declared evidence-preserving redaction.** `redact_record` withholds
  (`redact_value_keep_hash` / `…_and_raw_keep_hashes`) or tokenizes (`tokenize_deterministic`, scope-stable,
  never reversible) declared field values while preserving `value_sha256`/`raw_sha256`, offset/size,
  copybook provenance, and court identity. Unlisted fields **fail closed** under `deny_unlisted`;
  `public_output_claim:false`. +7 NEG.PRIVACY.*/NEG.REDACTION.*. tests/privacy.rs (3). NOT anonymization /
  regulatory compliance / reversibility / safe public release.
- **KOBOLD.PERF.1 — gated record-level Rayon (off by default).** New `reconcile_encoded_parallel` behind
  the `rayon` feature decodes records in parallel while emitting **byte-identical** evidence to scalar
  `reconcile_encoded` (same JSONL/audit/unsupported/`decode_output_sha256` + downstream posting chain).
  Internally `reconcile` now parses the program ONCE and decodes per-record from the shared layout (also
  faster scalar; output unchanged — all sealed goldens still pass). tests/perf1.rs proves scalar==rayon.
  No semantic change; no production/AWS/SIMD/parallel-throughput claim.
- **KOBOLD.BANK.RECONCILE.1 — opinionated generated banking reconciliation VIEW.** `bank_reconcile_report`
  assembles an operator report (json + md + SARIF) **only from existing court structs** (BANK.1/2 summary,
  POSTING.1 custody, DB2HOST.1, PRIVACY counts) — declared-vs-observed count/debit/credit + matched/mismatch
  verdict, custody seq min/max/gaps/dups + last_chain_hash, DB2 null/truncation, dirty/unsupported counts,
  redaction counts, refused truth layers; aggregates the EXISTING findings into one SARIF. Introduces no new
  evidence; a VIEW, not a new truth source. `BankingResult` gains a structured `summary`; `PostingManifest`
  gains `seq_min/seq_max/file_hash` (both now `#[non_exhaustive]`). +6 NEG.BANK_RECONCILE.*. tests/bank_reconcile.rs (2).

## [0.6.2]
- **KOBOLD.DATA.5 — cp500 numeric DISPLAY composed.** EBCDIC zoned-decimal numeric DISPLAY fields now
  decode through `gnucobol-rs` `GNURUST.17` (was fail-closed). The `account-cp500` family gained
  REGION-CODE `9(3)`, LIMIT-AMT `S9(7)V99`, RISK-PERCENT `9(3)V99` (record-len 51); the audit `encoding`
  block gains `numeric_display: {zoned_sign: GNURUST.17, code_page: cp500}`. Binary/packed stay raw
  passthrough (proven); cp500 zoned numerics legitimately differ ASCII-vs-cp500 (encoding-sensitive).
  0 unsupported, byte-stable, CLI == library. Bumped to gnucobol-rs ^0.6.3. Additive court composition,
  no API change → patch.

## [0.6.1]
- Fix: the operator test fixtures used the pre-DATA.4 account record-len (42); updated to 51. Test-
  only change (0.6.0's library/corpus were correct; only its bundled operator tests failed).

## [0.6.0]
- **KOBOLD.DATA.4 — edited-picture decode composed into the corpus.** The shim now decodes **edited
  DISPLAY fields** via `gnucobol-rs` `GNURUST.16` (slot-based). JSON keeps the **presentation string**
  (e.g. `"PRINT-BAL":"13,448.49"`); the oracle-proven **numeric interpretation** goes to a per-record
  audit `edited` block (`raw_text`/`numeric_value`/`claim`/`domain`) — never a silent replacement.
  Added one edited field per family (account `ZZ,ZZ9.99`, payroll `ZZ9.99CR`, insurance `ZZZ,ZZ9.99`);
  the corpus generator's editor is cobc-faithful (verified against `MOVE numeric → edited → DISPLAY`).
  0 unsupported, byte-stable, CLI == library. Negatives: edited under cp500 and unsupported edited
  symbols fail closed.
- **Breaking (semver-minor):** `DecodedField` gains `edited_numeric: Option<String>` and is now
  `#[non_exhaustive]` (so future fields stay additive). Bumped to gnucobol-rs ^0.6.2.

## [0.5.0]
- **KOBOLD.OPERATOR.1 — operator trust layer.** New `operator` module + CLI subcommands that make every
  decoded field accountable:
  - `explain <cpy> <data> <FIELD> --record N` → `explain_field`: provenance (copybook:line), offset/
    size/usage/pic, raw bytes, decoded value, **validity**, **sealed courts used**, dependent LEVEL-88s,
    record hash, non-claims, and the stale-copybook risk statement.
  - `totals <cpy> <data> --record-len N` → `control_totals`: record count, per-field numeric sums,
    condition true-counts, invalid/unsupported counts.
  - `decode <cpy> <data> --dirty-mode evidence|strict` → `DirtyMode`: evidence preserves invalid bytes
    and lists them; strict errors. **Never coerces.**
  - JSON key collisions are **refused** (duplicate field names); exact COBOL names preserved.
  - The reconcile audit now carries `stale_copybook_risk` (semantic decode hashes unchanged; goldens
    regenerated). Bumped to gnucobol-rs ^0.6. Additive API → semver-minor.

## [0.4.0]
- **KOBOLD.DATA.3 — cp500 EBCDIC in the reconciliation packet.** New `Encoding` enum (`Ascii`/`Cp500`,
  never auto-detected) + `reconcile_encoded` / `decode_record_encoded`. Under `Cp500`, alphanumeric
  DISPLAY fields (and the parent bytes feeding `eval_88`) are decoded through the sealed `GNURUST.15`
  cp500 table; **binary and packed fields pass through as raw storage and are never text-converted.**
  New `account-cp500` fixture family (120 records, 0 unsupported, byte-stable); the audit carries an
  `encoding` block (`record_default`, `source`, `auto_detected:false`, `*_passthrough:true`). The CLI
  gains `--encoding ascii|cp500`. Numeric DISPLAY under cp500 (EBCDIC zoned sign) fails closed.
  Proven: `ebcdic_never_touches_binary_or_packed` (same bytes, ASCII vs cp500 → identical numeric
  values). Bumped to gnucobol-rs ^0.5. Additive API → semver-minor.

## [0.3.2]
- Fix: the corpus golden test is now version-agnostic (compares decode/layout/input hashes, not the
  embedded tool-version metadata that changes per release), so a shim version bump no longer drifts
  the goldens; regenerated the committed audit.json. (0.3.1's bundled goldens carried the prior
  version string in a test fixture — cosmetic, no functional effect.)

## [0.3.1]
- **Ergonomics.** Optional `serde` feature (off by default) derives `Serialize` on `DecodedField`/
  `DecodedCondition`/`DecodedRecord` for Rust services consuming legacy feeds — values stay strings,
  never floats, so it cannot change decode semantics. New `decode_all(copybook, data, record_len,
  resolver)` higher-level iterator returns one `DecodedRecord` per fixed-length record.

## [0.3.0]
- **KOBOLD.DATA.2 — binary fields in the reconciliation corpus.** The shim now decodes `USAGE COMP`/
  `BINARY`/`COMP-5`/`COMP-X` via `gnucobol-rs` `GNURUST.14` (`Decimal::from_binary`); the three corpus
  families gained binary fields (BRANCH-NO/RISK-SCORE/INTERNAL-ID, EMPLOYEE-NO/HOURS-BUCKET, POLICY-
  SEQUENCE/CLAIM-COUNT), still 120 records each, 0 unsupported, byte-stable, CLI==lib. The audit now
  carries a `binary_byteorder` note (big-endian COMP/COMP-X, native COMP-5). Bumped to gnucobol-rs ^0.4.

## [0.2.0]
- **LEVEL-88 conditions.** `decode_record` evaluates condition names via `gnucobol-rs`' `eval_88`;
  decoded records now carry `conditions: { name: bool }`.
- **KOBOLD.RECON.1 — end-to-end reconciliation.** New `recon` module + `kobold-recon` CLI: decode a
  fixed-record file into byte-stable JSONL + a `kobold-recon-receipt-v1` audit (SHA-256s, counts,
  versions) + explicit `unsupported.json`. A committed 3-family / 360-record corpus with COPY/REPLACING,
  COMP-3, and alpha + numeric-range LEVEL-88s lives in `recon/`, with a sealing receipt. Proven
  byte-stable across runs and CLI == library; conditions come from `eval_88` only.
- **`SET condition TO TRUE` fixture** (`recon/condition-set/`): condition → bytes → `eval_88` true.
- Self-contained SHA-256 (`sha256` module, Apache-2.0; no GPL provenance).
- Bumped to `gnucobol-rs = "0.3.2"` (adds `eval_88`/`set_88_true`, ODO physical-max, P-scaling).

## [0.1.1]
- Re-export `CopyResolver` (public API for `decode_with_resolver`).

## [0.1.0]
- Initial release: copybook + record → decoded fields via the oracle-proven gnucobol-rs courts.
