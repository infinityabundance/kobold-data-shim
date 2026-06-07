//! KOBOLD.OPERATOR.1 acceptance: explain-field evidence, control totals, JSON key-collision refusal,
//! and dirty-data evidence-vs-strict mode (preserve, never coerce).

use kobold_data_shim::operator::decode_records_json;
use kobold_data_shim::{control_totals, explain_field, DirtyMode, Encoding, NoCopy};

struct Dir(String);
impl kobold_data_shim::CopyResolver for Dir {
    fn resolve(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(format!("{}/{name}.cpy", self.0))
            .or_else(|_| std::fs::read_to_string(format!("{}/{name}", self.0)))
            .ok()
    }
}

#[test]
fn explain_field_is_accountable() {
    let cb = std::fs::read_to_string("recon/account/ACCTREC.cpy").unwrap();
    let data = std::fs::read("recon/account/input.dat").unwrap();
    let json = explain_field(
        &cb,
        &data[..42],
        "BALANCE",
        &Dir("recon/account".into()),
        Encoding::Ascii,
    )
    .unwrap();
    // provenance, sealed courts, value, validity, and the explicit non-claims must all be present.
    assert!(json.contains("\"pic\":\"S9(7)V99\""));
    assert!(json.contains("\"usage\":\"COMP-3\""));
    assert!(json.contains("COMP-3 MOVE (GNURUST.2)"));
    assert!(json.contains("\"decoded_value\":\"5459318.55\""));
    assert!(json.contains("\"valid\":true"));
    assert!(json.contains("\"record_sha256\""));
    assert!(json.contains("no business-truth claim"));
    assert!(json.contains("stale_copybook_risk"));
}

#[test]
fn explain_includes_dependent_conditions() {
    let cb = std::fs::read_to_string("recon/account/ACCTREC.cpy").unwrap();
    let data = std::fs::read("recon/account/input.dat").unwrap();
    let json = explain_field(
        &cb,
        &data[..42],
        "STATUS-CODE",
        &Dir("recon/account".into()),
        Encoding::Ascii,
    )
    .unwrap();
    assert!(json.contains("\"ACTIVE\":true"));
    assert!(json.contains("LEVEL-88 (GNURUST.11)"));
}

#[test]
fn control_totals_accounting() {
    let cb = std::fs::read_to_string("recon/account/ACCTREC.cpy").unwrap();
    let data = std::fs::read("recon/account/input.dat").unwrap();
    let json = control_totals(
        &cb,
        &data,
        42,
        &Dir("recon/account".into()),
        Encoding::Ascii,
    )
    .unwrap();
    assert!(json.contains("\"record_count\":120"));
    assert!(json.contains("\"BALANCE\":")); // a per-field numeric sum
    assert!(json.contains("\"ACTIVE\":")); // a condition true-count
    assert!(json.contains("\"invalid_field_count\":0"));
    assert!(json.contains("\"unsupported_field_count\":0"));
}

#[test]
fn json_key_collision_is_refused() {
    // Two elementary fields named CODE would clobber each other in flat JSON — refuse, never silently.
    let cb = "       01 R.\n           05 CODE PIC X.\n           05 G.\n               10 CODE PIC X.\n";
    let data = b"AB";
    let err = explain_field(cb, data, "CODE", &NoCopy, Encoding::Ascii).unwrap_err();
    assert!(format!("{err}").contains("collision"), "got: {err}");
}

#[test]
fn dirty_data_evidence_vs_strict() {
    // A COMP-3 field whose final nibble is not a valid sign (corrupt legacy byte).
    let cb = "       01 R.\n           05 AMT PIC S9(3)V99 COMP-3.\n";
    let good = [0x12u8, 0x34, 0x5C]; // 123.45 (+)
    let bad = [0x12u8, 0x34, 0x51]; // last nibble 1 = not a sign → invalid
                                    // Evidence mode: clean record lists nothing; dirty record preserved + listed.
    let je =
        decode_records_json(cb, &good, 3, &NoCopy, Encoding::Ascii, DirtyMode::Evidence).unwrap();
    assert!(je.contains("\"invalid_fields\":[]"));
    let jd =
        decode_records_json(cb, &bad, 3, &NoCopy, Encoding::Ascii, DirtyMode::Evidence).unwrap();
    assert!(jd.contains("\"invalid_fields\":[\"AMT\"]"), "got: {jd}");
    assert!(
        jd.contains("\"AMT\":"),
        "evidence mode must still preserve the field, not drop it"
    );
    // Strict mode: the dirty record is a hard error (no coercion).
    let err =
        decode_records_json(cb, &bad, 3, &NoCopy, Encoding::Ascii, DirtyMode::Strict).unwrap_err();
    assert!(format!("{err}").contains("dirty data"), "got: {err}");
}
