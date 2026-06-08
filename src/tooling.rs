//! KOBOLD.TOOLING.EXPORT.1 — generated evidence export for downstream tools (IDE / LSP / auditor UI /
//! migration dashboard).
//!
//! **Doctrine.** TOOLING.EXPORT.1 admits only a *generated evidence export*: it maps decoded fields to
//! copybook provenance, byte ranges, court identities, the witness profile, findings, redaction status,
//! and non-claims, while **refusing to be an LSP, IDE, parser, new truth source, or editor integration**.
//! Everything here is assembled from the existing sealed-court decode + provenance — no new evidence.

use crate::operator::sealed_courts;
use crate::privacy::{DefaultAction, RedactionAction, RedactionPolicy};
use crate::sha256::sha256_hex;
use crate::{CopyResolver, Encoding, ShimError};

/// The tooling-export result (a `kobold-tooling-export-v1` JSON document).
pub struct ToolingExport {
    pub json: String,
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

/// Generate an IDE/tooling-friendly evidence map from the existing decode + provenance. `dialect_profile_id`
/// names the oracle witness (from `DIALECT.PROFILE.1`); an optional `redaction` policy is honored so the
/// export never leaks cleartext for a redacted field.
pub fn tooling_export(
    copybook: &str,
    record: &[u8],
    resolver: &impl CopyResolver,
    encoding: Encoding,
    dialect_profile_id: Option<&str>,
    redaction: Option<&RedactionPolicy>,
) -> Result<ToolingExport, ShimError> {
    let prog = crate::parse_program(copybook, resolver)?;
    let fields = crate::decode_fields(&prog, record, encoding);

    let mut entries: Vec<String> = Vec::new();
    for f in &fields {
        if f.category == "group" {
            continue;
        }
        let meta = prog.meta.get(&f.name);
        let (ft, _fl, cat) = prog
            .attrs
            .get(&f.name)
            .map(|(a, c)| (a.field_type, a.flags, *c))
            .unwrap_or((0, 0, "group"));
        let has_conds = prog.conditions.iter().any(|(parent, _)| parent == &f.name);
        let courts = sealed_courts(ft, cat, prog.used_copy, has_conds, encoding);
        let court_json = courts.iter().map(|c| jstr(c)).collect::<Vec<_>>().join(",");

        // redaction status — honor the declared policy; NEVER emit cleartext for a redacted field
        let (redacted, action_str) = match redaction {
            Some(p) => {
                let rule = p.rules.iter().find(|r| r.field == f.name);
                match rule {
                    Some(r) => (
                        r.action != RedactionAction::Allow,
                        match r.action {
                            RedactionAction::Allow => "allow",
                            RedactionAction::RedactValueKeepHash => "redact_value_keep_hash",
                            RedactionAction::RedactValueAndRawKeepHashes => {
                                "redact_value_and_raw_keep_hashes"
                            }
                            RedactionAction::TokenizeDeterministic => "tokenize_deterministic",
                        },
                    ),
                    None => match p.default_action {
                        DefaultAction::DenyUnlisted => (true, "deny_unlisted"),
                        _ => (false, "allow_unlisted"),
                    },
                }
            }
            None => (false, "not_redacted"),
        };
        let value_json = if redacted {
            format!(
                "\"decoded_value\":null,\"value_sha256\":{}",
                jstr(&sha256_hex(f.value.as_bytes()))
            )
        } else {
            format!("\"decoded_value\":{}", jstr(&f.value))
        };

        // per-field non-claims (truth boundaries this field can be over-read across)
        let mut nc: Vec<&str> =
            vec!["leading-zero identity needs a declared role (NEG.IDENTIFIER.NUMERIC_COERCION)"];
        match cat {
            "numeric" => nc.extend([
                "sign is not polarity (declare KOBOLD.BANK.2)",
                "scale/currency/amount-role need a declared profile (KOBOLD.CURRENCY.PROFILE.1)",
                "date meaning needs a declared format (KOBOLD.DATE.PROFILE.1)",
                "not business truth",
            ]),
            "alphanumeric" => nc.extend([
                "a sentinel/marker is not null/missing/status without a declared profile (KOBOLD.SENTINEL.PROFILE.1)",
                "not business truth",
            ]),
            "edited" => nc.push("presentation string; the oracle numeric is in the audit (GNURUST.16)"),
            _ => {}
        }
        let nc_json = nc.iter().map(|s| jstr(s)).collect::<Vec<_>>().join(",");

        let (pic, usage, copybook_path, line) = meta
            .map(|m| {
                (
                    m.pic.as_str(),
                    m.usage.as_str(),
                    m.source_file.as_str(),
                    m.source_line,
                )
            })
            .unwrap_or(("", "", "", 0));
        entries.push(format!(
            concat!(
                "{{\"qualified_name\":{},\"copybook_path\":{},\"line\":{},\"pic\":{},\"usage\":{},",
                "\"offset\":{},\"length\":{},\"category\":{},{},\"raw_sha256\":{},",
                "\"court_ids\":[{}],\"findings\":[],\"redaction\":{{\"status\":{},\"redacted\":{}}},",
                "\"non_claims\":[{}]}}"
            ),
            jstr(&f.name), jstr(copybook_path), line, jstr(pic), jstr(usage),
            f.offset, f.size, jstr(f.category), value_json, jstr(&sha256_hex(&unhex(&f.raw_hex))),
            court_json, jstr(action_str), redacted, nc_json,
        ));
    }

    let json = format!(
        concat!(
            "{{\"schema\":\"kobold-tooling-export-v1\",\"court\":\"KOBOLD.TOOLING.EXPORT.1\",",
            "\"introduces_new_evidence\":false,",
            "\"source\":{{\"copybook_sha256\":{},\"record_sha256\":{},\"dialect_profile_id\":{}}},",
            "\"fields\":[{}],",
            "\"negative_capabilities\":[\"NEG.TOOLING.NOT_LSP\",\"NEG.TOOLING.NOT_IDE\",\"NEG.TOOLING.NOT_FULL_PARSER\",",
            "\"NEG.TOOLING.NOT_SOURCE_OF_TRUTH\",\"NEG.TOOLING.NO_NEW_EVIDENCE\",\"NEG.TOOLING.REDACTION_STILL_APPLIES\"]}}\n"
        ),
        jstr(&sha256_hex(copybook.as_bytes())),
        jstr(&sha256_hex(record)),
        dialect_profile_id.map(jstr).unwrap_or_else(|| "null".to_string()),
        entries.join(","),
    );
    Ok(ToolingExport { json })
}
