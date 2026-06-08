//! KOBOLD.EXTRACT.PROFILE.1 (CUSTODY.1) — declared extraction provenance + copybook freshness.
//!
//! **Doctrine.** Every real migration depends on *how the bytes were obtained*. EXTRACT.PROFILE.1
//! records the **declared** extraction provenance (file organization, extract method, record-length
//! source, copybook source, any code-set conversion done before KOBOLD, source-system cutoff, operator
//! assumptions) and binds it to the data + copybook hashes — while refusing **extraction truth** and
//! treating **copybook freshness as a permanent uncertainty** (a stale copybook can decode bytes
//! plausibly wrong). KOBOLD proves decoded *extracted* bytes, not that the extraction or the copybook is
//! production truth.

use crate::sha256::sha256_hex;

/// How the source file was organized on the mainframe (declared, not detected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileOrganization {
    Sequential,
    LineSequential,
    Indexed,
    Relative,
    Vsam,
    Unknown,
}
/// How the bytes were extracted to the file KOBOLD sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtractMethod {
    UnloadedFixedRecord,
    ProgramExport,
    VendorTool,
    Unknown,
}
/// Where the record length came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordLengthSource {
    Copybook,
    Operator,
    Trailer,
    Unknown,
}

fn org(o: FileOrganization) -> &'static str {
    match o {
        FileOrganization::Sequential => "sequential",
        FileOrganization::LineSequential => "line_sequential",
        FileOrganization::Indexed => "indexed",
        FileOrganization::Relative => "relative",
        FileOrganization::Vsam => "vsam",
        FileOrganization::Unknown => "unknown",
    }
}
fn meth(m: ExtractMethod) -> &'static str {
    match m {
        ExtractMethod::UnloadedFixedRecord => "unloaded_fixed_record",
        ExtractMethod::ProgramExport => "program_export",
        ExtractMethod::VendorTool => "vendor_tool",
        ExtractMethod::Unknown => "unknown",
    }
}
fn rls(r: RecordLengthSource) -> &'static str {
    match r {
        RecordLengthSource::Copybook => "copybook",
        RecordLengthSource::Operator => "operator",
        RecordLengthSource::Trailer => "trailer",
        RecordLengthSource::Unknown => "unknown",
    }
}

/// The declared extraction profile.
pub struct ExtractProfile<'a> {
    pub source_file_organization: FileOrganization,
    pub extract_method: ExtractMethod,
    pub record_length_source: RecordLengthSource,
    pub copybook_source: &'a str,
    /// Any code-set conversion (e.g. EBCDIC→ASCII) done by a tool **before** KOBOLD saw the bytes.
    pub code_set_conversion_before_kobold: Option<&'a str>,
    pub source_system_cutoff: Option<&'a str>,
    pub business_date: Option<&'a str>,
    pub operator_declared_assumptions: &'a [&'a str],
}

/// The extraction-provenance custody result.
pub struct ExtractManifest {
    pub manifest_json: String,
    pub casefile_json: String,
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
fn opt(s: Option<&str>) -> String {
    s.map(jstr).unwrap_or_else(|| "null".to_string())
}

/// Record the declared extraction provenance for `data` decoded under `copybook`, with copybook freshness
/// held as an explicit non-claim. Pure metadata custody — no decode, no extraction-truth claim.
pub fn extract_manifest(copybook: &str, data: &[u8], profile: &ExtractProfile) -> ExtractManifest {
    let assumptions = profile
        .operator_declared_assumptions
        .iter()
        .map(|a| jstr(a))
        .collect::<Vec<_>>()
        .join(",");
    let manifest_json = format!(
        concat!(
            "{{\"schema\":\"kobold-extract-profile-v1\",\"file_sha256\":{},\"copybook_sha256\":{},",
            "\"source_file_organization\":{},\"extract_method\":{},\"record_length_source\":{},",
            "\"copybook_source\":{},\"code_set_conversion_before_kobold\":{},\"source_system_cutoff\":{},",
            "\"business_date\":{},\"operator_declared_assumptions\":[{}]}}"
        ),
        jstr(&sha256_hex(data)),
        jstr(&sha256_hex(copybook.as_bytes())),
        jstr(org(profile.source_file_organization)),
        jstr(meth(profile.extract_method)),
        jstr(rls(profile.record_length_source)),
        jstr(profile.copybook_source),
        opt(profile.code_set_conversion_before_kobold),
        opt(profile.source_system_cutoff),
        opt(profile.business_date),
        assumptions,
    );
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-extract-forensic-casefile-v1\",\"court\":\"KOBOLD.EXTRACT.PROFILE.1\",",
            "\"extraction_provenance\":{},",
            "\"copybook_freshness\":{{\"claimed\":false,\"copybook_sha256\":{},\"copybook_source\":{},",
            "\"evidence\":\"hash + declared provenance only\",\"risk\":\"a stale copybook may decode bytes plausibly wrong\"}},",
            "\"extraction_truth\":{{\"claimed\":false}},\"currentness\":{{\"claimed\":false}},",
            "\"negative_capabilities\":[\"NEG.EXTRACT.EXTRACTION_TRUTH\",\"NEG.EXTRACT.VSAM_NOT_CLAIMED\",",
            "\"NEG.EXTRACT.INDEXED_BACKEND_NOT_CLAIMED\",\"NEG.EXTRACT.FILE_STATUS_NOT_INTERPRETED\",",
            "\"NEG.CODESET.FILE_IO_CONVERSION\",\"NEG.COPYBOOK.STALE\",\"NEG.CURRENTNESS\"]}}\n"
        ),
        manifest_json,
        jstr(&sha256_hex(copybook.as_bytes())),
        jstr(profile.copybook_source),
    );
    ExtractManifest {
        manifest_json,
        casefile_json,
    }
}
