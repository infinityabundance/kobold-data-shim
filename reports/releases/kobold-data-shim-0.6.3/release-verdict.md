# Release verdict — kobold-data-shim 0.6.3

_Generated 2026-06-07T23:48:28Z from the machine files in this packet. A release is an evidence packet, not merely a
version number._

| evidence | value |
|----------|-------|
| crate / version | `kobold-data-shim` 0.6.3 (kobold) |
| git commit | `7c38e10003f50a46afc7dc575a01b33d11013ccb` |
| publish status | pending_crates_io_rate_limit_window |
| this-crate license | Apache-2.0 |
| dependencies | 2 (SBOM: `sbom.spdx.json`) |
| sealed courts in this crate | 4 (`claim-ladder-snapshot.json`) |
| TRUST.2 receipts | 14 (`receipt-manifest.json`) |
| cargo-audit | **pass** (`cargo-audit.txt`) |
| cargo-geiger | **pass** (`cargo-geiger.txt`) — every shipped crate is `#![forbid(unsafe_code)]` |

## What this release admits
The sealed courts in `claim-ladder-snapshot.json`, each proven against the admitted GnuCOBOL 3.2 oracle
with a reproducible TRUST.2 receipt.

## What this release does NOT admit
The non-claims in `negative-capabilities-snapshot.json`. **No production-readiness claim** beyond the
KRL level in `status-snapshot.md`. Unavailable tools above are marked honestly (`not_installed` /
`not_run` / `network_unavailable`), never faked green.

> Doctrine: ENTERPRISE.1 treats a release as an evidence packet — reproducible receipts, dependency/
> license inventory, feature flags, audit status, claim boundaries, negative capabilities, and a verdict
> that refuses to overstate production readiness.
