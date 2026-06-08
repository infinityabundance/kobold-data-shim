//! KOBOLD.CURRENCY.PROFILE.1 — declared currency/amount-profile evidence.
//!
//! **Doctrine.** CURRENCY.PROFILE.1 admits only **declared** currency/amount-profile evidence: a named
//! numeric field may be treated as an amount under an explicit profile + optional currency-code field,
//! while PIC scale, V99, symbols, **signs**, rates, FX conversion, rounding policy, legal-tender meaning,
//! accounting treatment, and business correctness remain non-claims. Sign is **not polarity** — `BANK.2`
//! owns debit/credit. This closes the value-profile trio: markers (SENTINEL) → dates (DATE) → money.

use crate::banking::NumericRole;
use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// A declared currency/amount-field profile.
pub struct CurrencyFieldProfile<'a> {
    pub field: &'a str,
    pub numeric_role: NumericRole,
    pub currency_code_field: Option<&'a str>,
    pub declared_scale: u8,
    /// When the observed implied scale != `declared_scale`, emit a finding.
    pub scale_mismatch_is_finding: bool,
}

/// The declared currency profile.
pub struct CurrencyProfile<'a> {
    pub fields: &'a [CurrencyFieldProfile<'a>],
}

/// The currency-validation result.
pub struct CurrencyManifest {
    pub manifest_json: String,
    pub casefile_json: String,
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
fn role_str(r: NumericRole) -> &'static str {
    match r {
        NumericRole::Amount => "amount",
        NumericRole::Rate => "rate",
        NumericRole::Identifier => "identifier",
        NumericRole::Code => "code",
        NumericRole::Sequence => "sequence",
        NumericRole::Count => "count",
        _ => "unknown",
    }
}
/// The implied decimal scale observed in a decoded numeric value (fractional digits).
fn observed_scale(v: &str) -> u8 {
    v.rsplit_once('.').map(|(_, f)| f.len() as u8).unwrap_or(0)
}

/// Validate declared amount fields against their declared currency profile — evidence only.
pub fn currency_validate(
    copybook: &str,
    record: &[u8],
    profile: &CurrencyProfile,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<CurrencyManifest, ShimError> {
    let decoded = decode_record_encoded(copybook, record, resolver, encoding)?;
    let fmap: std::collections::HashMap<&str, &str> = decoded
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str()))
        .collect();

    let mut findings: Vec<(String, String)> = Vec::new();
    let mut entries: Vec<String> = Vec::new();

    for fp in profile.fields {
        let Some(value) = fmap.get(fp.field) else {
            findings.push((
                "KOBOLD-CURRENCY-MISSING-FIELD".into(),
                format!("declared amount field {:?} not found", fp.field),
            ));
            continue;
        };
        if fp.numeric_role != NumericRole::Amount {
            // a rate / percent / identifier is numeric but NEVER money
            findings.push((
                "KOBOLD-CURRENCY-ROLE-NOT-AMOUNT".into(),
                format!(
                    "{:?} declared role={} is not an amount (not admitted as money)",
                    fp.field,
                    role_str(fp.numeric_role)
                ),
            ));
            entries.push(format!(
                "{{\"field\":{},\"numeric_role\":{},\"admitted_as_money\":false}}",
                jstr(fp.field),
                jstr(role_str(fp.numeric_role)),
            ));
            continue;
        }
        let obs = observed_scale(value);
        let scale_match = obs == fp.declared_scale;
        if !scale_match && fp.scale_mismatch_is_finding {
            findings.push((
                "KOBOLD-CURRENCY-SCALE-MISMATCH".into(),
                format!(
                    "{:?}: observed scale {obs} != declared scale {}",
                    fp.field, fp.declared_scale
                ),
            ));
        }
        // currency code is preserved as EVIDENCE, never legal-tender truth
        let ccy_json = match fp.currency_code_field {
            Some(cf) => match fmap.get(cf) {
                Some(code) => format!(",\"currency_code_field\":{},\"currency_code_evidence\":{},\"legal_tender_truth\":false", jstr(cf), jstr(code.trim())),
                None => {
                    findings.push(("KOBOLD-CURRENCY-MISSING-CODE".into(), format!("declared currency-code field {cf:?} not found")));
                    format!(",\"currency_code_field\":{},\"currency_code_evidence\":null,\"legal_tender_truth\":false", jstr(cf))
                }
            },
            None => String::new(),
        };
        entries.push(format!(
            "{{\"field\":{},\"numeric_role\":\"amount\",\"admitted_as_money\":false,\"declared_scale\":{},\"observed_scale\":{},\"scale_match\":{},\"sign_is_polarity\":false,\"money_meaning_claimed\":false{}}}",
            jstr(fp.field), fp.declared_scale, obs, scale_match, ccy_json,
        ));
    }

    let manifest_json = format!(
        "{{\"schema\":\"kobold-currency-manifest-v1\",\"court\":\"KOBOLD.CURRENCY.PROFILE.1\",\"record_sha256\":{},\"fields\":[{}]}}",
        jstr(&sha256_hex(record)), entries.join(","),
    );
    let find_json = findings
        .iter()
        .map(|(r, m)| format!("{{\"ruleId\":{},\"message\":{}}}", jstr(r), jstr(m)))
        .collect::<Vec<_>>()
        .join(",");
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-currency-forensic-casefile-v1\",\"court\":\"KOBOLD.CURRENCY.PROFILE.1\",",
            "\"manifest\":{},\"findings\":[{}],",
            "\"truth_layers\":{{\"amount_scale_evidence\":true,\"money_meaning\":false,\"fx_conversion\":false,",
            "\"legal_tender\":false,\"rounding_policy\":false,\"accounting_treatment\":false,\"business_value\":false}},",
            "\"negative_capabilities\":[\"NEG.CURRENCY.V99_NOT_MONEY\",\"NEG.CURRENCY.MINOR_UNIT_NOT_INFERRED\",",
            "\"NEG.CURRENCY.CODE_NOT_LEGAL_TENDER_TRUTH\",\"NEG.CURRENCY.FX_CONVERSION_NOT_CLAIMED\",",
            "\"NEG.CURRENCY.ROUNDING_POLICY_NOT_CLAIMED\",\"NEG.CURRENCY.SIGN_NOT_POLARITY\",",
            "\"NEG.CURRENCY.RATE_NOT_AMOUNT\",\"NEG.CURRENCY.BUSINESS_VALUE_NOT_CLAIMED\"]}}\n"
        ),
        manifest_json, find_json,
    );
    Ok(CurrencyManifest {
        manifest_json,
        casefile_json,
        findings,
    })
}
