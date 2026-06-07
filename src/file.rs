//! Fixed-record container ingest (`KOBOLD.FILE.1`): split a raw byte stream into records of an
//! explicit, caller-declared length, with stable offsets, named trailing/partial policies, a
//! file-level audit, and stable exit codes.
//!
//! **Doctrine.** KOBOLD.FILE.1 admits only explicit fixed-record container ingest: bytes are split by
//! a caller-declared record length with stable offsets, policies, manifests, and exit codes, while
//! GnuCOBOL file I/O, indexed files, line-sequential runtime behavior, auto-resynchronization, and
//! silent repair remain outside the claim.
//!
//! This is **ingest reliability**, not GnuCOBOL file-organization parity. The default is **strict**:
//! a partial trailing record or an unexpected trailing newline is an error, never silently absorbed.

use crate::sha256::sha256_hex;

/// What to do with a single trailing `\n` (`0x0A`) at the very end of the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TrailingNewline {
    /// A final `\n` is **not** allowed — its presence is an invalid input shape (the strict default).
    #[default]
    Reject,
    /// A single final `\n` is tolerated and removed before splitting (success).
    AllowFinalLf,
    /// A single final `\n` is removed before splitting and recorded as stripped in the audit.
    StripFinalLf,
}

/// What to do with a trailing partial record (`input_len % record_len != 0`, after newline handling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PartialRecord {
    /// A partial trailing record is an invalid input shape (the strict default).
    #[default]
    Reject,
    /// A partial trailing record is preserved as marked evidence (verdict downgrades to warnings).
    Evidence,
}

/// Stable process exit codes (`KOBOLD.FILE.1`). Documented and frozen — operators script against these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[repr(i32)]
pub enum ExitCode {
    /// Clean ingest/decode.
    Success = 0,
    /// Decoded, but evidence warnings exist (a preserved partial record, or dirty fields).
    DecodedWithEvidenceWarnings = 1,
    /// The input shape is invalid (record_len 0, partial record under strict, unexpected trailing LF).
    InvalidInputShape = 2,
    /// An unsupported COBOL surface was required.
    UnsupportedCobolSurface = 3,
    /// An internal invariant failed (a bug — should never happen).
    InternalInvariantFailure = 4,
    /// An I/O or configuration error (missing file, missing required flag, bad encoding).
    IoOrConfigError = 5,
}

impl ExitCode {
    /// The numeric code (for `std::process::exit`).
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// Why an ingest was rejected, carrying the [`ExitCode`] an operator should see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestError {
    pub message: String,
    pub exit: ExitCode,
}

impl core::fmt::Display for IngestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} (exit {})", self.message, self.exit.code())
    }
}
impl std::error::Error for IngestError {}

/// The ingest policy (all explicit — nothing auto-detected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestPolicy {
    pub record_len: usize,
    pub trailing_newline: TrailingNewline,
    pub partial_record: PartialRecord,
}

impl IngestPolicy {
    /// Strict defaults: reject a trailing newline and a partial record.
    pub fn strict(record_len: usize) -> Self {
        IngestPolicy {
            record_len,
            trailing_newline: TrailingNewline::Reject,
            partial_record: PartialRecord::Reject,
        }
    }
}

/// One record's position in the original byte stream. `offset` is the **true file offset**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordSpan {
    pub index: usize,
    pub offset: usize,
    pub len: usize,
    /// `true` if this is a preserved partial trailing record (shorter than `record_len`).
    pub partial: bool,
}

/// The result of ingesting a fixed-record container.
#[derive(Debug, Clone)]
pub struct Ingest {
    pub record_len: usize,
    pub input_len: usize,
    pub input_sha256: String,
    pub spans: Vec<RecordSpan>,
    pub stripped_final_lf: bool,
    pub partial_present: bool,
    pub verdict: ExitCode,
    pub trailing_policy: TrailingNewline,
    pub partial_policy: PartialRecord,
}

impl Ingest {
    /// The bytes of record `i` (sliced from the caller's original `data`).
    pub fn record<'a>(&self, data: &'a [u8], i: usize) -> Option<&'a [u8]> {
        let s = self.spans.get(i)?;
        data.get(s.offset..s.offset + s.len)
    }

    /// The canonical-JSON file-level audit manifest (byte-stable for a given input + policy).
    pub fn file_audit_json(&self) -> String {
        let policy = |t: TrailingNewline| match t {
            TrailingNewline::Reject => "reject",
            TrailingNewline::AllowFinalLf => "allow-final-lf",
            TrailingNewline::StripFinalLf => "strip-final-lf",
        };
        let ppolicy = match self.partial_policy {
            PartialRecord::Reject => "reject",
            PartialRecord::Evidence => "evidence",
        };
        let (first, last) = match (self.spans.first(), self.spans.last()) {
            (Some(f), Some(l)) => (f.offset as i64, l.offset as i64),
            _ => (-1, -1),
        };
        format!(
            "{{\"schema\":\"kobold-file-ingest-v1\",\"file\":{{\"input_sha256\":\"{}\",\"input_len\":{},\
             \"record_len\":{},\"record_count\":{},\"trailing_policy\":\"{}\",\"partial_record_policy\":\"{}\",\
             \"stripped_final_lf\":{},\"partial_record_present\":{},\"offsets\":{{\"first\":{},\"last\":{}}},\
             \"verdict_exit_code\":{}}}}}",
            self.input_sha256,
            self.input_len,
            self.record_len,
            self.spans.len(),
            policy(self.trailing_policy),
            ppolicy,
            self.stripped_final_lf,
            self.partial_present,
            first,
            last,
            self.verdict.code(),
        )
    }
}

/// Ingest `data` into fixed-length records under `policy`. Offsets are true file offsets; nothing is
/// auto-detected or silently repaired.
pub fn ingest(data: &[u8], policy: &IngestPolicy) -> Result<Ingest, IngestError> {
    if policy.record_len == 0 {
        return Err(IngestError {
            message: "record_len must be > 0".into(),
            exit: ExitCode::IoOrConfigError,
        });
    }
    let input_len = data.len();
    let input_sha256 = sha256_hex(data);

    // Trailing-newline handling operates on the *effective* length used for splitting; the recorded
    // offsets remain true offsets into the original stream (the stripped LF is simply never a record).
    let mut eff_len = input_len;
    let mut stripped_final_lf = false;
    let ends_lf = data.last() == Some(&b'\n');
    if ends_lf {
        match policy.trailing_newline {
            TrailingNewline::Reject => {
                // Only an error if the LF is what makes the shape wrong is too lenient; strict means a
                // final LF is simply not allowed.
                return Err(IngestError {
                    message: "unexpected trailing newline (policy: reject)".into(),
                    exit: ExitCode::InvalidInputShape,
                });
            }
            TrailingNewline::AllowFinalLf => {
                eff_len -= 1;
            }
            TrailingNewline::StripFinalLf => {
                eff_len -= 1;
                stripped_final_lf = true;
            }
        }
    }

    let rl = policy.record_len;
    let full = eff_len / rl;
    let rem = eff_len % rl;
    let mut spans: Vec<RecordSpan> = (0..full)
        .map(|i| RecordSpan {
            index: i,
            offset: i * rl,
            len: rl,
            partial: false,
        })
        .collect();

    let mut partial_present = false;
    let mut verdict = ExitCode::Success;
    if rem != 0 {
        match policy.partial_record {
            PartialRecord::Reject => {
                return Err(IngestError {
                    message: format!(
                        "partial trailing record: {rem} byte(s) beyond {full} full records of {rl} (policy: reject)"
                    ),
                    exit: ExitCode::InvalidInputShape,
                });
            }
            PartialRecord::Evidence => {
                spans.push(RecordSpan {
                    index: full,
                    offset: full * rl,
                    len: rem,
                    partial: true,
                });
                partial_present = true;
                verdict = ExitCode::DecodedWithEvidenceWarnings;
            }
        }
    }

    Ok(Ingest {
        record_len: rl,
        input_len,
        input_sha256,
        spans,
        stripped_final_lf,
        partial_present,
        verdict,
        trailing_policy: policy.trailing_newline,
        partial_policy: policy.partial_record,
    })
}
