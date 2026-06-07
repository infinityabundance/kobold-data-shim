# Changelog

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
