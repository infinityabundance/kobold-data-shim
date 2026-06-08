//! KOBOLD.NIST-STYLE-FIXTURE-FORMAT.1 acceptance: a named fixture replays a real court and matches its
//! expected verdict/findings/non-claims; a wrong expectation fails; a changed record changes the hash; a
//! risk-bearing fixture without non-claims fails; nothing claims NIST conformance.

use kobold_data_shim::{
    date_validate, replay_fixture, DateFieldProfile, DateFormat, DateProfile, Encoding, Fixture,
    FixtureOutcome, FixtureVerdict, NoCopy,
};

const CB: &str = "       01 REC.\n           05 D PIC 9(8).\n";

// run DATE.PROFILE.1 on a record and map its real result into a FixtureOutcome
fn run_date(record: &[u8]) -> FixtureOutcome {
    static F: &[DateFieldProfile] = &[DateFieldProfile {
        field: "D",
        format: DateFormat::Yyyymmdd,
        require_sentinel_profile: true,
    }];
    let m = date_validate(
        CB,
        record,
        &DateProfile { fields: F },
        &[],
        &NoCopy,
        Encoding::Ascii,
    )
    .unwrap();
    let findings: Vec<String> = m.findings.iter().map(|(r, _)| r.clone()).collect();
    let verdict = if findings.is_empty() {
        FixtureVerdict::Pass
    } else {
        FixtureVerdict::FailClosed
    };
    FixtureOutcome { verdict, findings }
}

#[test]
fn positive_fixture_replays_and_matches() {
    let fx = Fixture {
        fixture_id: "date-valid-001",
        court: "KOBOLD.DATE.PROFILE.1",
        description: "valid YYYYMMDD passes",
        copybook: CB,
        record: b"20240229",
        profile: "", // 2024 is a leap year -> valid
        expected_verdict: FixtureVerdict::Pass,
        expected_findings: &[],
        expected_non_claims: &[],
    };
    let r = replay_fixture(&fx, &run_date(b"20240229"));
    assert!(
        r.matched,
        "valid leap-year date should pass: {:?}",
        r.mismatches
    );
    assert!(
        r.fixture_json.contains("\"nist_conformance\":false")
            && r.fixture_json.contains("\"matched\":true")
    );
    assert!(r.fixture_json.contains("NEG.FIXTURE.NOT_NIST_CONFORMANCE"));
    assert!(r.fixture_json.contains(
        "\"target_court_casefile\":\"reports/casefiles/KOBOLD.DATE.PROFILE.1/casefile.json\""
    ));
}

#[test]
fn negative_fixture_is_first_class() {
    let fx = Fixture {
        fixture_id: "date-zero-undeclared-001",
        court: "KOBOLD.DATE.PROFILE.1",
        description: "00000000 is not a date and not a declared sentinel -> fail closed",
        copybook: CB,
        record: b"00000000",
        profile: "",
        expected_verdict: FixtureVerdict::FailClosed,
        expected_findings: &[
            "KOBOLD-DATE-INVALID-CALENDAR-DATE",
            "KOBOLD-DATE-SENTINEL-UNDECLARED",
        ],
        expected_non_claims: &["PIC 9(8) is not a date", "a zero-date is not null"],
    };
    let r = replay_fixture(&fx, &run_date(b"00000000"));
    assert!(
        r.matched,
        "negative fixture should match: {:?}",
        r.mismatches
    );
    assert!(r.fixture_json.contains("\"verdict\":\"fail_closed\""));
}

#[test]
fn wrong_expected_finding_fails() {
    let fx = Fixture {
        fixture_id: "date-wrong-expect-001",
        court: "KOBOLD.DATE.PROFILE.1",
        description: "wrong expectation",
        copybook: CB,
        record: b"20230229",
        profile: "", // 2023 NOT a leap year -> invalid calendar
        expected_verdict: FixtureVerdict::Pass,
        expected_findings: &[],
        expected_non_claims: &[],
    };
    let r = replay_fixture(&fx, &run_date(b"20230229"));
    assert!(!r.matched, "a wrong expectation must fail");
    assert!(r
        .mismatches
        .iter()
        .any(|m| m.contains("VERDICT-MISMATCH") || m.contains("FINDING-MISMATCH")));
}

#[test]
fn risk_fixture_without_non_claims_fails_and_changed_record_changes_hash() {
    // fail-closed fixture with NO declared non-claims -> rejected as ceremony
    let fx = Fixture {
        fixture_id: "date-no-nonclaim-001",
        court: "KOBOLD.DATE.PROFILE.1",
        description: "missing non-claims",
        copybook: CB,
        record: b"00000000",
        profile: "",
        expected_verdict: FixtureVerdict::FailClosed,
        expected_findings: &[
            "KOBOLD-DATE-INVALID-CALENDAR-DATE",
            "KOBOLD-DATE-SENTINEL-UNDECLARED",
        ],
        expected_non_claims: &[],
    };
    let r = replay_fixture(&fx, &run_date(b"00000000"));
    assert!(
        !r.matched
            && r.mismatches
                .iter()
                .any(|m| m.contains("NON-CLAIM-REQUIRED"))
    );
    // changed input record changes the fixture's record_sha256
    let a = replay_fixture(
        &Fixture {
            record: b"20240229",
            ..fx_lite()
        },
        &run_date(b"20240229"),
    );
    let b = replay_fixture(
        &Fixture {
            record: b"20240228",
            ..fx_lite()
        },
        &run_date(b"20240228"),
    );
    let h = |j: &str| j.split("\"record_sha256\":").nth(1).unwrap()[..20].to_string();
    assert_ne!(
        h(&a.fixture_json),
        h(&b.fixture_json),
        "changed record must change record_sha256"
    );
}
fn fx_lite() -> Fixture<'static> {
    Fixture {
        fixture_id: "x",
        court: "KOBOLD.DATE.PROFILE.1",
        description: "x",
        copybook: CB,
        record: b"20240229",
        profile: "",
        expected_verdict: FixtureVerdict::Pass,
        expected_findings: &[],
        expected_non_claims: &[],
    }
}
