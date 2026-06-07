<!-- snapshot of STATUS.md at kobold-data-shim 0.6.3 release (2026-06-07T23:48:28Z) -->

# STATUS — live current-state authority

> **This page wins.** README is orientation; receipts are evidence; historical close docs are seal
> snapshots. **When any of them disagree with this page, this page is correct.** It answers one
> question: *what may a user rely on today?*

_gnucobol-rs 0.7.0 · 18 sealed GNURUST courts · oracle: GnuCOBOL 3.2.0 (admitted, built in lab)._
_(The git repo is the authority; crates.io may trail by a version under publish rate limits.)_

## What may be relied on today

Byte-exact, oracle-proven **read fidelity** for fixed-record COBOL data, within the admitted subset:

- **Storage + MOVE bytes** — COMP-3 / zoned / display (`GNURUST.2`), packed ADD/SUB/MUL (`7/13`).
- **Field model + layout** — PIC (+P) (`3/9`), record offsets / fixed OCCURS / REDEFINES(≤target) /
  ODO **physical-max** (`4/10`), VALUE initial image (`8`).
- **COPY / REPLACING** text-word expansion (`5/6`).
- **LEVEL-88** truth + SET TO TRUE (`11/12`).
- **Binary** COMP/COMP-5/COMP-X storage+MOVE (`14`, default 1-2-4-8 table only).
- **cp500 EBCDIC** DISPLAY text decode (`15`) and **zoned-decimal numeric** decode (`17`).
- **COMP-6** unsigned packed-decimal storage + MOVE (`18`).
- **Edited-picture** decode 16a+16b (`16`, decode-only).
- **KOBOLD** composes these into a byte-stable reconciliation packet with an operator trust layer
  (explain / totals / dirty-mode), binary/packed **passthrough** under EBCDIC, **cp500 numeric DISPLAY**
  (zoned sign) decode, and edited fields' presentation-vs-numeric split.

Every one of those has a **generated receipt** (`reports/receipts/<CAMPAIGN>/receipt.json`) produced
from a live replay, and appears green in `lab/verify-sealed-courts.sh`.

## What may NOT be relied on

See [`docs/not-yet-ready.md`](docs/not-yet-ready.md). Headline: this is **not** a compiler, not
`libcob`, not Procedure Division execution, not universal COBOL truth, not business-truth validation,
not automatic migration, not dirty-data repair, not a proven AWS deployment.

## Readiness level

**KRL-3** (operator explain/totals/dirty-mode over a composed synthetic corpus). Not yet KRL-4 (pilot
on sanitized customer-like data). See the ladder in [`docs/not-yet-ready.md`](docs/not-yet-ready.md).

## Fastest reproduction (~3–5 min)

```sh
cargo test                              # self-contained court tests (no oracle needed)
bash lab/verify-sealed-courts.sh        # all sweeps + shim suite + doc-gate (needs built oracle)
python3 lab/receipt/run.py check        # receipts == live replay, no hand-edits
```

## Open debts (current)

- cp037 + other code pages; numeric DISPLAY under non-cp500 — deferred.
- File/container ingest discipline — deferred (`KOBOLD.FILE.1`).
- Transformed-record (write) reconciliation — deferred (`KOBOLD.RECON.2`).
- Live AWS deployment receipt — `kobold-lambda-layer` is compile-verified only.

The machine-readable risk ledger is [`docs/future-risk-register.md`](docs/future-risk-register.md);
the per-court non-claims are [`docs/negative-capabilities.md`](docs/negative-capabilities.md).
