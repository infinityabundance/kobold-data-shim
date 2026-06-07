# Changelog

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
