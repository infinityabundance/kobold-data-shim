//! KOBOLD.EXTRACT.PROFILE.1 acceptance: declared extraction provenance is recorded + bound to the data/
//! copybook hashes; copybook freshness and extraction truth are explicit non-claims.

use kobold_data_shim::{
    extract_manifest, ExtractMethod, ExtractProfile, FileOrganization, RecordLengthSource,
};

#[test]
fn records_provenance_and_refuses_extraction_truth() {
    let cb = "       01 R.\n           05 ID PIC 9(3).\n";
    let data = b"001002003";
    let p = ExtractProfile {
        source_file_organization: FileOrganization::Vsam,
        extract_method: ExtractMethod::VendorTool,
        record_length_source: RecordLengthSource::Copybook,
        copybook_source: "ops-team copybook v3, 2026-06-01",
        code_set_conversion_before_kobold: Some("EBCDIC cp500 -> ASCII by IDCAMS unload"),
        source_system_cutoff: Some("2026-06-08T00:00:00Z"),
        business_date: Some("2026-06-08"),
        operator_declared_assumptions: &["fixed 3-byte records", "no header/trailer"],
    };
    let m = extract_manifest(cb, data, &p);
    // declared provenance recorded
    assert!(m
        .manifest_json
        .contains("\"source_file_organization\":\"vsam\""));
    assert!(m
        .manifest_json
        .contains("\"extract_method\":\"vendor_tool\""));
    assert!(m.manifest_json.contains(
        "\"code_set_conversion_before_kobold\":\"EBCDIC cp500 -> ASCII by IDCAMS unload\""
    ));
    assert!(m
        .manifest_json
        .contains("\"source_system_cutoff\":\"2026-06-08T00:00:00Z\""));
    assert!(m.manifest_json.contains(
        "\"operator_declared_assumptions\":[\"fixed 3-byte records\",\"no header/trailer\"]"
    ));
    // bound to hashes
    assert!(
        m.manifest_json.contains("\"file_sha256\":")
            && m.manifest_json.contains("\"copybook_sha256\":")
    );
    // copybook freshness + extraction truth REFUSED
    assert!(m
        .casefile_json
        .contains("\"copybook_freshness\":{\"claimed\":false"));
    assert!(m
        .casefile_json
        .contains("\"extraction_truth\":{\"claimed\":false}"));
    assert!(
        m.casefile_json.contains("NEG.COPYBOOK.STALE")
            && m.casefile_json.contains("NEG.EXTRACT.EXTRACTION_TRUTH")
    );
}

#[test]
fn optional_fields_null_when_absent() {
    let p = ExtractProfile {
        source_file_organization: FileOrganization::Unknown,
        extract_method: ExtractMethod::Unknown,
        record_length_source: RecordLengthSource::Unknown,
        copybook_source: "unknown",
        code_set_conversion_before_kobold: None,
        source_system_cutoff: None,
        business_date: None,
        operator_declared_assumptions: &[],
    };
    let m = extract_manifest("01 R.", b"x", &p);
    assert!(m
        .manifest_json
        .contains("\"code_set_conversion_before_kobold\":null"));
    assert!(m.manifest_json.contains("\"business_date\":null"));
    assert!(m
        .manifest_json
        .contains("\"operator_declared_assumptions\":[]"));
}
