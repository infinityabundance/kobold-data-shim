# Pilot evidence packet — PILOT-DECLARED-SYNTHETIC-001

> [!IMPORTANT]
> A **pilot evidence packet** — a generated derived view over named, hash-pinned court artifacts. **Not** certification, compliance, production approval, or customer acceptance. It creates **no new truth**.

- business date: 2026-06-08 · source: synthetic-pilot
- artifacts: **5**  ·  complete: **true**  ·  review-notes hash: `6f1d122e867ac276…`

## Source evidence (hash-pinned)

| artifact | court | sha256 |
|---|---|---|
| `extract_profile` | `KOBOLD.EXTRACT.PROFILE.1` | `2eacebcd3929234e…` |
| `redaction_policy` | `KOBOLD.PRIVACY.REDACTION.1` | `787e9d7f3b203e2f…` |
| `bank_reconcile` | `KOBOLD.BANK.RECONCILE.1` | `b80e8c1f04c0bb82…` |
| `diff` | `KOBOLD.DIFF.1` | `0b79e58a6a55c9d9…` |
| `tooling_export` | `KOBOLD.TOOLING.EXPORT.1` | `dd92003b89087fef…` |

## Operator review checklist

- [ ] declared copybook confirmed current for this extract (EXTRACT.PROFILE.1 copybook freshness is a permanent uncertainty)
- [ ] redaction policy reviewed and applied before any extract left the secure zone (PRIVACY.REDACTION.1)
- [ ] BANK.1 declared-vs-observed control totals matched, or the mismatch is acknowledged in the notes
- [ ] BANK.2 polarity came only from declared value tables; no sign-as-polarity inference
- [ ] DIFF.1 target oracle-status recorded; a match was read as equality-to-declared, not correctness
- [ ] no real customer data appears in any artifact shared outside the secure zone
- [ ] truth boundaries acknowledged: this packet is pilot evidence, not ledger/settlement/business truth

