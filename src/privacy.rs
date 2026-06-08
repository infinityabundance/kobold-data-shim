//! KOBOLD.PRIVACY.REDACTION.1 — declared evidence-preserving redaction.
//!
//! **Doctrine.** Before real files enter the pipeline, privacy must be a *court*, not a policy footnote.
//! PRIVACY.REDACTION.1 admits declared field-level redaction for generated evidence: selected decoded
//! values may be **withheld or tokenized** while **raw-byte hashes, field provenance, offsets, court
//! identity, and audit structure remain preserved** (so the evidence stays auditable). It does **not**
//! claim anonymization, regulatory compliance, reversibility, or safe public release of customer data.

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// What to do with a declared field's value in generated evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RedactionAction {
    /// Explicitly pass the value through unredacted (the operator declares it non-sensitive).
    Allow,
    /// Hide the value; keep `value_sha256` + `raw_sha256` (and `raw_hex`) for verification.
    RedactValueKeepHash,
    /// Hide the value AND `raw_hex`; keep `value_sha256` + `raw_sha256` only.
    RedactValueAndRawKeepHashes,
    /// Replace the value with a deterministic token (stable within the declared scope). **Never claimed
    /// reversible** — a token is not an identity.
    TokenizeDeterministic,
}

/// What to do with a field that has no declared rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefaultAction {
    /// Strict: an unlisted field fails closed (value withheld + a finding) — the operator must declare
    /// the handling of every field before real data is shown.
    DenyUnlisted,
    /// An unlisted field passes through unredacted.
    AllowUnlisted,
}

/// A declared per-field rule.
pub struct FieldRule<'a> {
    pub field: &'a str,
    pub action: RedactionAction,
}

/// A declared redaction policy. `public_output_claim` is intentionally absent — the court **never**
/// claims an output is safe for public release.
pub struct RedactionPolicy<'a> {
    pub rules: &'a [FieldRule<'a>],
    pub default_action: DefaultAction,
    /// Deterministic-token scope (e.g. `"casefile"` or a batch id). Same value + scope → same token.
    pub token_scope: &'a str,
}

/// The redacted evidence for one record.
pub struct RedactionResult {
    pub json: String,
    pub findings: Vec<(String, String)>,
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

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|k| u8::from_str_radix(&s[k * 2..k * 2 + 2], 16).unwrap_or(0))
        .collect()
}

/// Produce evidence-preserving redacted JSON for one record under a declared policy. Hashes, offsets,
/// provenance, and court identity are always preserved; sensitive values are withheld or tokenized.
pub fn redact_record(
    copybook: &str,
    record: &[u8],
    policy: &RedactionPolicy,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<RedactionResult, ShimError> {
    let rec = decode_record_encoded(copybook, record, resolver, encoding)?;
    let mut findings: Vec<(String, String)> = Vec::new();
    let mut fields = String::new();
    let mut first = true;

    for f in &rec.fields {
        if f.category == "group" {
            continue;
        }
        let action = policy
            .rules
            .iter()
            .find(|r| r.field == f.name)
            .map(|r| r.action)
            .unwrap_or(match policy.default_action {
                DefaultAction::DenyUnlisted => RedactionAction::RedactValueKeepHash,
                DefaultAction::AllowUnlisted => RedactionAction::Allow,
            });
        let unlisted = !policy.rules.iter().any(|r| r.field == f.name);
        if unlisted && policy.default_action == DefaultAction::DenyUnlisted {
            findings.push((
                "KOBOLD-PRIVACY-UNLISTED-FIELD".into(),
                format!("field {:?} has no declared rule under a deny-unlisted policy (withheld, fail closed)", f.name),
            ));
        }
        let value_sha = sha256_hex(f.value.as_bytes());
        let raw_sha = sha256_hex(&unhex(&f.raw_hex));
        if !first {
            fields.push(',');
        }
        first = false;
        let common = format!(
            "\"redacted\":{},\"action\":{},\"offset\":{},\"size\":{},\"value_sha256\":{},\"raw_sha256\":{},\"court\":\"KOBOLD.PRIVACY.REDACTION.1\"",
            action != RedactionAction::Allow,
            jstr(match action {
                RedactionAction::Allow => "allow",
                RedactionAction::RedactValueKeepHash => "redact_value_keep_hash",
                RedactionAction::RedactValueAndRawKeepHashes => "redact_value_and_raw_keep_hashes",
                RedactionAction::TokenizeDeterministic => "tokenize_deterministic",
            }),
            f.offset, f.size, jstr(&value_sha), jstr(&raw_sha),
        );
        let body = match action {
            RedactionAction::Allow => format!(
                "\"value\":{},\"raw_hex\":{},{}",
                jstr(&f.value),
                jstr(&f.raw_hex),
                common
            ),
            RedactionAction::RedactValueKeepHash => format!(
                "\"value\":\"[REDACTED]\",\"raw_hex\":{},{}",
                jstr(&f.raw_hex),
                common
            ),
            RedactionAction::RedactValueAndRawKeepHashes => format!(
                "\"value\":\"[REDACTED]\",\"raw_hex\":\"[REDACTED]\",{}",
                common
            ),
            RedactionAction::TokenizeDeterministic => {
                let token = format!(
                    "TOK-{}",
                    &sha256_hex(format!("{}:{}", policy.token_scope, f.value).as_bytes())[..12]
                );
                format!("\"value\":{},\"token\":true,{}", jstr(&token), common)
            }
        };
        fields.push_str(&format!("{}:{{{}}}", jstr(&f.name), body));
    }

    let json = format!(
        concat!(
            "{{\"schema\":\"kobold-redaction-policy-v1\",\"court\":\"KOBOLD.PRIVACY.REDACTION.1\",",
            "\"mode\":\"evidence_preserving\",\"public_output_claim\":false,",
            "\"record_sha256\":{},\"fields\":{{{}}},",
            "\"non_claims\":[\"NEG.PRIVACY.NO_REAL_CUSTOMER_DATA_PUBLIC\",\"NEG.REDACTION.NOT_ANONYMIZATION\",",
            "\"NEG.REDACTION.NOT_REGULATORY_COMPLIANCE\",\"NEG.REDACTION.HASH_NOT_BUSINESS_VALUE\",",
            "\"NEG.REDACTION.TOKEN_NOT_IDENTITY\",\"NEG.REDACTION.REVERSIBILITY_NOT_CLAIMED\",",
            "\"NEG.REDACTION.UNLISTED_SENSITIVE_FIELD\"]}}\n"
        ),
        jstr(&sha256_hex(record)),
        fields,
    );
    Ok(RedactionResult { json, findings })
}
