//! KOBOLD.SENTINEL.PROFILE.1 acceptance: declared sentinel markers (raw_hex / decoded_value) are recorded
//! as EVIDENCE ONLY; undeclared sentinel-looking values are never inferred; nullness/date/missingness/
//! business meaning remain claimed:false.

use kobold_data_shim::{
    sentinel_scan, Encoding, NoCopy, SentinelMatch, SentinelProfile, SentinelRule,
};

const CB: &str = "       01 REC.\n           05 CLOSE-DATE PIC 9(8).\n           05 STATUS-CODE PIC X(4).\n           05 AMOUNT PIC 9(5).\n";
// CLOSE-DATE "00000000", STATUS-CODE 0xFFFFFFFF, AMOUNT "00000" (undeclared zeroes)
fn rec() -> Vec<u8> {
    let mut r = b"00000000".to_vec();
    r.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    r.extend_from_slice(b"00000");
    r
}

fn profile() -> SentinelProfile<'static> {
    // declared rules only; AMOUNT is intentionally NOT declared
    static CLOSE: &[SentinelRule] = &[SentinelRule {
        sentinel_id: "ZERO-DATE",
        match_on: SentinelMatch::DecodedValue("0"),
    }];
    static STATUS: &[SentinelRule] = &[SentinelRule {
        sentinel_id: "HIGH-VALUES",
        match_on: SentinelMatch::RawHex("ffffffff"),
    }];
    static FIELDS: &[(&str, &[SentinelRule])] = &[("CLOSE-DATE", CLOSE), ("STATUS-CODE", STATUS)];
    SentinelProfile { fields: FIELDS }
}

#[test]
fn declared_markers_match_as_evidence_only() {
    let m = sentinel_scan(CB, &rec(), &profile(), &NoCopy, Encoding::Ascii).unwrap();
    assert!(m.hits.contains(&("CLOSE-DATE".into(), "ZERO-DATE".into())));
    assert!(m
        .hits
        .contains(&("STATUS-CODE".into(), "HIGH-VALUES".into())));
    // declared decoded_value + raw_hex matches both rendered
    assert!(m
        .manifest_json
        .contains("\"sentinel_id\":\"ZERO-DATE\",\"matched_on\":\"decoded_value\""));
    assert!(m
        .manifest_json
        .contains("\"sentinel_id\":\"HIGH-VALUES\",\"matched_on\":\"raw_hex\""));
    // every hit is marker-only + business meaning refused
    assert!(m
        .manifest_json
        .contains("\"meaning_label\":\"declared_marker_only\",\"business_meaning_claimed\":false"));
    assert!(
        m.casefile_json.contains("\"nullness\":false")
            && m.casefile_json.contains("\"date_meaning\":false")
    );
    assert!(m.casefile_json.contains("\"business_status\":false"));
    assert!(m.casefile_json.contains("NEG.SENTINEL.LOW_VALUES_NOT_NULL"));
}

#[test]
fn undeclared_sentinel_looking_value_is_not_inferred() {
    let m = sentinel_scan(CB, &rec(), &profile(), &NoCopy, Encoding::Ascii).unwrap();
    // AMOUNT decodes to "0" (all zeroes) but is NOT declared -> no hit, no inference
    assert!(!m.hits.iter().any(|(f, _)| f == "AMOUNT"));
    assert!(m.manifest_json.contains("\"undeclared_inference\":false"));
    assert_eq!(m.hits.len(), 2);
}

#[test]
fn declared_field_missing_fails_closed() {
    static R: &[SentinelRule] = &[SentinelRule {
        sentinel_id: "X",
        match_on: SentinelMatch::DecodedValue("0"),
    }];
    static F: &[(&str, &[SentinelRule])] = &[("NO-SUCH", R)];
    let p = SentinelProfile { fields: F };
    let m = sentinel_scan(CB, &rec(), &p, &NoCopy, Encoding::Ascii).unwrap();
    assert!(m
        .findings
        .iter()
        .any(|(r, _)| r == "KOBOLD-SENTINEL-NO-FIELD"));
    assert!(m.hits.is_empty());
}
