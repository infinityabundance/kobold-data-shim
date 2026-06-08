//! KOBOLD.LAYOUT.REDEFINES.2 acceptance: overlapping REDEFINES views are decoded as byte evidence over the
//! SAME shared bytes; active_view stays claimed:false unless a declared discriminator admits it; an unknown
//! discriminator keeps it false; business meaning is refused.

use kobold_data_shim::{redefines_manifest, Encoding, NoCopy, RedefinesDiscriminator};

const CB: &str = "       01 REC.\n           05 REC-TYPE PIC X.\n           05 SHARED PIC X(12).\n           05 ACCOUNT-VIEW REDEFINES SHARED.\n               10 ACCT-ID PIC 9(6).\n               10 ACCT-BR PIC 9(6).\n           05 LOAN-VIEW REDEFINES SHARED.\n               10 LOAN-ID PIC 9(4).\n               10 LOAN-TERM PIC 9(8).\n";
const REC: &[u8] = b"A123456789012";

#[test]
fn overlapping_views_decoded_active_view_false_by_default() {
    let m = redefines_manifest(CB, REC, &NoCopy, Encoding::Ascii, None).unwrap();
    assert_eq!(m.region_count, 1);
    assert!(
        m.active_view.is_none(),
        "no discriminator -> no active view"
    );
    // one storage region (offset 1, length 12) with the base + both overlay views
    assert!(m.manifest_json.contains("\"offset\":1,\"length\":12"));
    assert!(m.manifest_json.contains("\"view_id\":\"SHARED\""));
    assert!(m.manifest_json.contains("\"view_id\":\"ACCOUNT-VIEW\""));
    assert!(m.manifest_json.contains("\"view_id\":\"LOAN-VIEW\""));
    // each view decodes independently over the same bytes
    assert!(
        m.manifest_json.contains("\"ACCT-ID\":\"123456\"")
            && m.manifest_json.contains("\"ACCT-BR\":\"789012\"")
    );
    assert!(
        m.manifest_json.contains("\"LOAN-ID\":\"1234\"")
            && m.manifest_json.contains("\"LOAN-TERM\":\"56789012\"")
    );
    // active_view refused + business meaning refused
    assert!(m
        .manifest_json
        .contains("\"active_view\":{\"claimed\":false"));
    assert!(m.casefile_json.contains("\"business_meaning\":false"));
    assert!(m
        .casefile_json
        .contains("NEG.REDEFINES.ACTIVE_VIEW_NOT_INFERRED"));
}

#[test]
fn region_raw_hash_present_and_deterministic() {
    let m1 = redefines_manifest(CB, REC, &NoCopy, Encoding::Ascii, None).unwrap();
    let m2 = redefines_manifest(CB, REC, &NoCopy, Encoding::Ascii, None).unwrap();
    assert_eq!(m1.manifest_json, m2.manifest_json, "deterministic");
    // every storage region carries one raw_sha256 of the SHARED bytes (same hash backs all views)
    assert_eq!(m1.manifest_json.matches("\"raw_sha256\":").count(), 1);
    assert!(m1.manifest_json.contains("\"record_sha256\":"));
}

#[test]
fn declared_discriminator_admits_active_view() {
    let disc = RedefinesDiscriminator {
        field: "REC-TYPE",
        mapping: &[("A", "ACCOUNT-VIEW"), ("L", "LOAN-VIEW")],
    };
    let m = redefines_manifest(CB, REC, &NoCopy, Encoding::Ascii, Some(&disc)).unwrap();
    assert_eq!(m.active_view.as_deref(), Some("ACCOUNT-VIEW"));
    assert!(m
        .manifest_json
        .contains("\"active_view\":{\"claimed\":true,\"view_id\":\"ACCOUNT-VIEW\""));
    assert!(m.manifest_json.contains("declared_by"));
    // the active view is flagged is_active:true; the others remain layout-valid byte views
    assert!(m.manifest_json.contains("\"view_id\":\"ACCOUNT-VIEW\",\"redefines\":\"SHARED\",\"layout_valid\":true,\"is_active\":true"));
}

#[test]
fn unknown_discriminator_keeps_active_view_false() {
    let disc = RedefinesDiscriminator {
        field: "REC-TYPE",
        mapping: &[("X", "ACCOUNT-VIEW")],
    };
    let m =
        redefines_manifest(CB, b"Z123456789012", &NoCopy, Encoding::Ascii, Some(&disc)).unwrap();
    assert!(
        m.active_view.is_none(),
        "unknown discriminator value -> no active view"
    );
    assert!(m
        .manifest_json
        .contains("\"active_view\":{\"claimed\":false"));
}
