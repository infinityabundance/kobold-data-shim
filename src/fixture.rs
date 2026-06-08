//! NIST-STYLE-FIXTURE-FORMAT.1 — a small, named, replayable fixture format for KOBOLD/GNURUST courts.
//!
//! **Doctrine.** NIST-STYLE-FIXTURE-FORMAT.1 admits only *replayable, named fixture evidence* for declared
//! courts: a fixture may prove that one input/copybook/profile combination produces the **expected verdict,
//! findings, hashes, and non-claims**, while NIST conformance, COBOL language parity, certification, oracle
//! authority, customer representativeness, and business truth remain **non-claims**. "NIST-style" names the
//! *shape* (named replayable cases with expected outputs) — it is **not** a NIST conformance claim.
//!
//! Replay is honest: the caller runs the named court, wraps its real outcome in a [`FixtureOutcome`], and
//! [`replay_fixture`] compares **actual vs expected** — so a wrong expected verdict or finding genuinely
//! fails (`matched:false`). Negative (fail-closed) fixtures are first-class and must carry non-claims.

use crate::sha256::sha256_hex;
use std::collections::BTreeSet;

/// A replayable fixture verdict. Negative fixtures (`FailClosed`/`Mismatch`/`Refused`) are first-class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FixtureVerdict {
    /// The court accepted the input cleanly (no findings).
    Pass,
    /// The court refused/failed closed on a hostile or undeclared input (the negative case).
    FailClosed,
    /// The court reported a declared-vs-observed mismatch.
    Mismatch,
    /// The input is outside the sealed subset and was refused as unsupported.
    Refused,
}
fn verdict_str(v: FixtureVerdict) -> &'static str {
    match v {
        FixtureVerdict::Pass => "pass",
        FixtureVerdict::FailClosed => "fail_closed",
        FixtureVerdict::Mismatch => "mismatch",
        FixtureVerdict::Refused => "refused",
    }
}

/// A named, replayable fixture: declared inputs + the expected outcome.
pub struct Fixture<'a> {
    pub fixture_id: &'a str,
    pub court: &'a str,
    pub description: &'a str,
    pub copybook: &'a str,
    pub record: &'a [u8],
    /// Optional declared profile reference (e.g. a path or id), bound by hash if non-empty.
    pub profile: &'a str,
    pub expected_verdict: FixtureVerdict,
    pub expected_findings: &'a [&'a str],
    pub expected_non_claims: &'a [&'a str],
}

/// The ACTUAL outcome the caller captured by running the named court on the fixture inputs.
pub struct FixtureOutcome {
    pub verdict: FixtureVerdict,
    pub findings: Vec<String>,
}

/// The replay result: did the court's actual outcome match the fixture's expectation?
pub struct FixtureResult {
    pub matched: bool,
    pub fixture_json: String,
    pub mismatches: Vec<String>,
}

fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
fn arr(items: &[String]) -> String {
    items.iter().map(|s| jstr(s)).collect::<Vec<_>>().join(",")
}

/// Replay a fixture: compare a court's ACTUAL outcome against the fixture's declared expectation, and emit
/// the `kobold-fixture-v1` evidence. A risk-bearing (negative) fixture with no non-claims fails.
pub fn replay_fixture(fx: &Fixture, actual: &FixtureOutcome) -> FixtureResult {
    let mut mismatches: Vec<String> = Vec::new();
    if fx.expected_verdict != actual.verdict {
        mismatches.push(format!(
            "KOBOLD-NIST-FIXTURE-VERDICT-MISMATCH: expected {}, actual {}",
            verdict_str(fx.expected_verdict),
            verdict_str(actual.verdict)
        ));
    }
    let exp: BTreeSet<&str> = fx.expected_findings.iter().copied().collect();
    let act: BTreeSet<&str> = actual.findings.iter().map(|s| s.as_str()).collect();
    if exp != act {
        mismatches.push(format!(
            "KOBOLD-NIST-FIXTURE-FINDING-MISMATCH: expected {:?}, actual {:?}",
            fx.expected_findings, actual.findings
        ));
    }
    // a risk-bearing (negative) fixture MUST carry non-claims, or it is ceremony
    let risk = !matches!(fx.expected_verdict, FixtureVerdict::Pass);
    if risk && fx.expected_non_claims.is_empty() {
        mismatches.push("KOBOLD-NIST-FIXTURE-NON-CLAIM-REQUIRED: a fail-closed/mismatch fixture must declare non-claims".into());
    }
    let matched = mismatches.is_empty();

    let exp_find = fx
        .expected_findings
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let exp_nc = fx
        .expected_non_claims
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let fixture_json = format!(
        concat!(
            "{{\"schema\":\"kobold-fixture-v1\",\"court\":\"NIST-STYLE-FIXTURE-FORMAT.1\",",
            "\"fixture_id\":{},\"target_court\":{},\"description\":{},\"nist_conformance\":false,",
            "\"target_court_casefile\":{},",
            "\"inputs\":{{\"copybook_sha256\":{},\"record_sha256\":{},\"profile\":{}}},",
            "\"expected\":{{\"verdict\":{},\"findings\":[{}],\"non_claims\":[{}]}},",
            "\"actual\":{{\"verdict\":{},\"findings\":[{}]}},",
            "\"matched\":{},\"mismatches\":[{}],",
            "\"negative_capabilities\":[\"NEG.FIXTURE.NOT_NIST_CONFORMANCE\",\"NEG.FIXTURE.NOT_LANGUAGE_SUITE\",",
            "\"NEG.FIXTURE.NOT_CERTIFICATION\",\"NEG.FIXTURE.EXPECTED_NOT_ORACLE\",\"NEG.FIXTURE.PASS_NOT_BUSINESS_TRUTH\",",
            "\"NEG.FIXTURE.SYNTHETIC_NOT_CUSTOMER_DATA\"]}}\n"
        ),
        jstr(fx.fixture_id), jstr(fx.court), jstr(fx.description),
        jstr(&format!("reports/casefiles/{}/casefile.json", fx.court)),
        jstr(&sha256_hex(fx.copybook.as_bytes())), jstr(&sha256_hex(fx.record)),
        if fx.profile.is_empty() { "null".to_string() } else { jstr(&sha256_hex(fx.profile.as_bytes())) },
        jstr(verdict_str(fx.expected_verdict)), arr(&exp_find), arr(&exp_nc),
        jstr(verdict_str(actual.verdict)), arr(&actual.findings),
        matched, arr(&mismatches),
    );
    FixtureResult {
        matched,
        fixture_json,
        mismatches,
    }
}
