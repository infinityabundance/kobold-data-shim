# Banking reconciliation view — PILOT-DECLARED-SYNTHETIC-001

> Generated operator **view** over existing KOBOLD evidence (BANK.1/2 · DB2HOST.1 · POSTING.1 · PRIVACY.REDACTION.1). It summarizes; it does **not** create posting, ledger, settlement, account-balance, or business truth.

- business date: 2026-06-08 · source: synthetic-pilot · extract: 2026-06-08T10:15:00Z
- file_hash: `6c25c069bae96b5589065da62de11346af580514419d769475b630a2d3b246ad`
- last_chain_hash: `36b72d89086bf6ead0902850f11f6db1725de2097491f720f7ac48cddcde90ed`

## Controls — **matched**

| | declared | observed |
|---|---|---|
| count | 3 | 3 |
| debit | 150.00 | 150.00 |
| credit | 30.00 | 30.00 |

## Custody
- sequence: 1…3  · gaps: none  · dup sequences: none  · dup txn ids: none

## DB2 indicators
- semantic null: 0 · truncation: 0 · dirty/missing: 0

## Findings
- dirty: 0 · unsupported: 0 · unknown record type: 0 · unknown polarity: 0

## Privacy
- redacted: 3 · tokenized: 3 · public_output_claim: false

## Truth layers
- record_truth: ✓  ·  posting/ledger/settlement/business truth: **refused** (claimed:false)
