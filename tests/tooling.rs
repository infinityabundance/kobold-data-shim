//! KOBOLD.TOOLING.EXPORT.1 acceptance: an IDE/tooling evidence map assembled from existing decode +
//! provenance — field path/offset/length/court-id/witness/non-claims present, redacted fields never leak
//! cleartext, deterministic, and it introduces no new evidence.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::{
    tooling_export, DefaultAction, Encoding, FieldRule, NoCopy, RedactionAction, RedactionPolicy,
};

const CB: &str = "       01 REC.\n           05 ACCT-ID PIC 9(6).\n           05 NAME PIC X(8).\n           05 AMOUNT PIC S9(7)V99 COMP-3.\n";
fn rec() -> Vec<u8> {
    let mut r = b"000042".to_vec();
    r.extend_from_slice(b"ANNA    ");
    let pf = build_field("S9(7)V99", Usage::Comp3, false, false).unwrap();
    let sa = FieldAttr {
        field_type: COB_TYPE_NUMERIC_DISPLAY,
        digits: pf.attr.digits,
        scale: pf.attr.scale,
        flags: COB_FLAG_HAVE_SIGN,
    };
    let mut out = vec![0u8; pf.size];
    cob_move(b"012345", &sa, &mut out, &pf.attr).unwrap(); // 0123.45
    r.extend(out);
    r
}

#[test]
fn export_maps_provenance_courts_witness_and_non_claims() {
    let e = tooling_export(
        CB,
        &rec(),
        &NoCopy,
        Encoding::Ascii,
        Some("gnucobol-3.2.0-default"),
        None,
    )
    .unwrap();
    let j = &e.json;
    assert!(
        j.contains("\"schema\":\"kobold-tooling-export-v1\"")
            && j.contains("\"introduces_new_evidence\":false")
    );
    // witness profile + source hashes
    assert!(j.contains("\"dialect_profile_id\":\"gnucobol-3.2.0-default\""));
    assert!(j.contains("\"copybook_sha256\":") && j.contains("\"record_sha256\":"));
    // field path / offset / length / pic / court ids
    assert!(
        j.contains("\"qualified_name\":\"AMOUNT\"") && j.contains("\"offset\":14,\"length\":5")
    );
    assert!(j.contains("\"pic\":\"S9(7)V99\""));
    assert!(
        j.contains("PIC (GNURUST.3)")
            && j.contains("LAYOUT (GNURUST.4)")
            && j.contains("COMP-3 MOVE (GNURUST.2)")
    );
    // per-field non-claims attached (numeric -> sign is not polarity)
    assert!(j.contains("sign is not polarity"));
    assert!(j.contains("NEG.TOOLING.NOT_LSP") && j.contains("NEG.TOOLING.NO_NEW_EVIDENCE"));
}

#[test]
fn redacted_field_never_leaks_cleartext() {
    let rules = [FieldRule {
        field: "NAME",
        action: RedactionAction::RedactValueKeepHash,
    }];
    let pol = RedactionPolicy {
        rules: &rules,
        default_action: DefaultAction::AllowUnlisted,
        token_scope: "casefile",
    };
    let e = tooling_export(
        CB,
        &rec(),
        &NoCopy,
        Encoding::Ascii,
        Some("gnucobol-3.2.0-default"),
        Some(&pol),
    )
    .unwrap();
    let j = &e.json;
    // NAME is redacted -> no cleartext, value_sha256 instead
    assert!(
        !j.contains("ANNA"),
        "redacted field must not leak cleartext"
    );
    assert!(j.contains("\"qualified_name\":\"NAME\"") && j.contains("\"redacted\":true"));
    assert!(j.contains("\"status\":\"redact_value_keep_hash\""));
    // ACCT-ID (not redacted) still shows its value
    assert!(j.contains("\"qualified_name\":\"ACCT-ID\"") && j.contains("\"redacted\":false"));
}

#[test]
fn deterministic() {
    let a = tooling_export(CB, &rec(), &NoCopy, Encoding::Ascii, Some("p"), None).unwrap();
    let b = tooling_export(CB, &rec(), &NoCopy, Encoding::Ascii, Some("p"), None).unwrap();
    assert_eq!(a.json, b.json);
}
