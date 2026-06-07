# Enterprise readiness — SBOM, CVE scanning, support policy

Regulated adopters (finance, government) treat every dependency as supply-chain risk. This document
states where `kobold-data-shim` stands.

## SBOM (Software Bill of Materials)

- **Runtime dependency graph is tiny and auditable:** this crate depends only on `gnucobol-rs`,
  which itself has **zero runtime dependencies**. The full runtime closure is two crates.
- A CycloneDX SBOM is generated on release (`cargo cyclonedx --format json`) and committed to
  `reports/sbom.json`. Regenerate with `make sbom` (or the CI job). A hand-maintained minimal SBOM
  is committed so the closure is visible even without the tool.
- The SBOM records the pinned `gnucobol-rs` version and, transitively, its admitted GnuCOBOL 3.2
  oracle identity (source sha256) — so the *semantic* provenance is auditable, not just the code.

## CVE / vulnerability scanning

- CI runs `cargo audit` (RustSec advisory DB) on every push; a clean run is a merge gate.
- Zero runtime deps means the attack surface is the two crates plus `std`. Optional performance
  features (e.g. a future `parallel`/`simd`) would add build-time deps; those are documented
  separately and never enabled by default.

## Versioning & support policy

- **SemVer.** Pre-1.0, minor bumps may change the API; patch bumps do not. The decode *semantics*
  track `gnucobol-rs`' sealed courts — a court correctness fix (rare) is called out in the changelog
  as a behavioural change, distinct from an API change.
- **Host assumptions:** little-endian ASCII (the `gnucobol-rs` sealed claim). EBCDIC must be
  translated upstream until an EBCDIC court lands.
- **MSRV:** 1.74.
- A long-term-support / SLA tier is a commercial-track question; the permissive clean-room kernel
  (future) is the natural basis for SLA-backed offerings.

## Trust posture

The differentiator is **oracle-driven, receipt-backed transparency**: the decode kernel is proven
byte-identical to GnuCOBOL under differential sweeps, with Kani proofs and fuzzing, and every
non-claim is machine-readable. That evidence is the supply-chain story.
