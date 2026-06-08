//! KOBOLD.DATE.PROFILE.1 acceptance: declared date fields validated against an explicit format on the RAW
//! digit string; sentinels delegated to SENTINEL.PROFILE.1; PIC shape alone gets no date claim; business/
//! settlement/maturity/Y2K/arithmetic meaning all refused.

use kobold_data_shim::{
    date_validate, DateFieldProfile, DateFormat, DateProfile, Encoding, NoCopy,
};

const CB: &str = "       01 REC.\n           05 EFF-DATE PIC 9(8).\n           05 CLOSE-DATE PIC 9(8).\n           05 BAD-DATE PIC 9(8).\n           05 UNTOUCHED PIC 9(8).\n";
// EFF-DATE valid, CLOSE-DATE zero (declared sentinel), BAD-DATE month 13, UNTOUCHED not in profile
const REC: &[u8] = b"20240131000000002024133219991231";

fn profile() -> DateProfile<'static> {
    static F: &[DateFieldProfile] = &[
        DateFieldProfile {
            field: "EFF-DATE",
            format: DateFormat::Yyyymmdd,
            require_sentinel_profile: true,
        },
        DateFieldProfile {
            field: "CLOSE-DATE",
            format: DateFormat::Yyyymmdd,
            require_sentinel_profile: true,
        },
        DateFieldProfile {
            field: "BAD-DATE",
            format: DateFormat::Yyyymmdd,
            require_sentinel_profile: true,
        },
    ];
    DateProfile { fields: F }
}

#[test]
fn valid_invalid_and_delegated_sentinel() {
    // CLOSE-DATE is a declared sentinel (from a prior SENTINEL.PROFILE.1 scan)
    let hits = vec![("CLOSE-DATE".to_string(), "ZERO-DATE".to_string())];
    let m = date_validate(CB, REC, &profile(), &hits, &NoCopy, Encoding::Ascii).unwrap();
    let st = |f: &str| {
        m.statuses
            .iter()
            .find(|(x, _)| x == f)
            .map(|(_, s)| s.as_str())
    };
    assert_eq!(st("EFF-DATE"), Some("valid")); // 2024-01-31 valid (leading-zero MM preserved)
    assert_eq!(st("CLOSE-DATE"), Some("declared_sentinel")); // delegated -> not validated as a date
    assert_eq!(st("BAD-DATE"), Some("invalid_calendar")); // month 13
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-DATE-INVALID-CALENDAR-DATE"));
    // a valid date claims format_valid_only, never business meaning
    assert!(m.manifest_json.contains("\"field\":\"EFF-DATE\",\"format\":\"YYYYMMDD\",\"raw\":\"20240131\",\"status\":\"valid\",\"date_meaning_claimed\":\"format_valid_only\""));
    assert!(
        m.casefile_json.contains("\"business_calendar\":false")
            && m.casefile_json.contains("\"date_arithmetic\":false")
    );
    // UNTOUCHED (PIC 9(8) with no profile) gets no date claim at all
    assert!(st("UNTOUCHED").is_none());
    assert!(m.casefile_json.contains("NEG.DATE.PIC9_NOT_DATE"));
}

#[test]
fn undeclared_zero_date_is_not_a_date_and_flags_sentinel_required() {
    // no sentinel hits this time -> CLOSE-DATE "00000000" is invalid + sentinel-undeclared
    let m = date_validate(CB, REC, &profile(), &[], &NoCopy, Encoding::Ascii).unwrap();
    let st = |f: &str| {
        m.statuses
            .iter()
            .find(|(x, _)| x == f)
            .map(|(_, s)| s.as_str())
    };
    assert_eq!(st("CLOSE-DATE"), Some("invalid_calendar")); // 0000-00-00 is not a valid date
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-DATE-SENTINEL-UNDECLARED"));
}

#[test]
fn missing_declared_field_fails_closed() {
    static F: &[DateFieldProfile] = &[DateFieldProfile {
        field: "NO-SUCH",
        format: DateFormat::Yyyymmdd,
        require_sentinel_profile: false,
    }];
    let p = DateProfile { fields: F };
    let m = date_validate(CB, REC, &p, &[], &NoCopy, Encoding::Ascii).unwrap();
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-DATE-MISSING-FIELD"));
}
