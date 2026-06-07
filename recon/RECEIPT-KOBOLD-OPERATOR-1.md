# RECEIPT-KOBOLD-OPERATOR-1 — operator trust layer

**Campaign KOBOLD.OPERATOR.1.** Make every decoded field **accountable**, so a migration engineer (or
auditor) can point at a value and ask *"why do you say this field means that?"* and get evidence.

## Doctrine

> KOBOLD.OPERATOR.1 makes every decoded field accountable: each value must be explainable from source
> provenance, raw bytes, sealed courts, and receipt hashes, while dirty or unsupported data remains
> preserved evidence rather than coerced output.

## What it adds

| Capability | Surface |
|------------|---------|
| **explain-field** | `kobold-recon explain <cpy> <data> <FIELD> --record N` → JSON: provenance (`copybook:line`), offset/size/level, usage, pic, raw bytes, decoded value, **validity**, **sealed courts used**, **dependent LEVEL-88s**, record hash, **non-claims**, **stale-copybook risk** |
| **control totals** | `kobold-recon totals <cpy> <data> --record-len N` → record count, per-field numeric sums, condition true-counts, **invalid_field_count**, unsupported_field_count |
| **dirty-data mode** | `kobold-recon decode <cpy> <data> --dirty-mode evidence\|strict` — `evidence` preserves invalid bytes and lists them in `invalid_fields`; `strict` is a hard error. **Never coerces.** |
| **JSON key-collision policy** | duplicate elementary field names are **refused** (not silently clobbered); exact COBOL names preserved, flattening is opt-in |
| **stale-copybook risk** | every operator output **and** the reconcile audit carry the explicit risk statement |
| **exact name preservation** | COBOL field names are JSON keys verbatim — no silent normalization |

## Acceptance (tests)

| Check | Result |
|-------|--------|
| `explain` carries provenance, courts, value, validity, non-claims, risk | `explain_field_is_accountable` |
| `explain` includes dependent conditions + LEVEL-88 court | `explain_includes_dependent_conditions` |
| `totals` reports record count, field sums, condition counts, dirty/unsupported | `control_totals_accounting` |
| duplicate JSON keys refused | `json_key_collision_is_refused` |
| dirty COMP-3: `evidence` preserves + lists, `strict` errors, never coerces | `dirty_data_evidence_vs_strict` |
| all KOBOLD.DATA.1–3 fixtures replay unchanged | `corpus_is_byte_stable_and_golden`, `cp500_family_composes_end_to_end` (audit gained a `stale_copybook_risk` field; semantic decode hashes unchanged) |

Dirty-data validity is a **detection** signal (COMP-3 sign nibble, zoned digits), never a correction —
the migration-safe stance: surface the dirt, preserve the bytes, let the operator decide.
