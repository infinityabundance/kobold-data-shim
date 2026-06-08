//! KOBOLD.BANK.1 — header/detail/trailer banking court: declared-vs-observed control totals.
//!
//! **Doctrine.** The banking court does not promote decoded COBOL records into banking truth. It
//! preserves byte truth and record truth, routes records by a **declared** variant discriminator, and
//! reconciles the trailer's **declared** control totals against KOBOLD-**observed** totals — so that
//! posting, ledger, and business truth can only be claimed under explicit declared profiles. A balanced
//! file is not a correct file; a matched trailer is not ledger acceptance.
//!
//! Everything here is driven by a caller-**declared** profile (`VariantSpec` + `ControlSpec`). Nothing
//! is auto-detected, and debit/credit polarity is taken only from the **declared** indicator field —
//! never inferred from a numeric sign (`NEG.BANKING.SIGN_IS_NOT_POLARITY`).

use crate::file::ExitCode;
use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// A record-type variant: the discriminator bytes that select it, a name, and its copybook text.
pub struct Variant<'a> {
    pub discriminator: &'a [u8],
    pub name: &'a str,
    pub copybook: &'a str,
}

/// The declared variant-routing profile: where the discriminator lives + the variant table.
pub struct VariantSpec<'a> {
    pub discriminator_offset: usize,
    pub discriminator_len: usize,
    pub variants: &'a [Variant<'a>],
}

/// The declared numeric ROLE of a field (`ACCOUNTING.PROFILE.1`). Only `Amount` fields are summed into
/// debit/credit totals; identifiers, codes, rates, sequences, and counts are numeric but **never money**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericRole {
    Amount,
    Rate,
    Identifier,
    Code,
    Sequence,
    Count,
    Unknown,
}

impl NumericRole {
    fn as_str(self) -> &'static str {
        match self {
            NumericRole::Amount => "amount",
            NumericRole::Rate => "rate",
            NumericRole::Identifier => "identifier",
            NumericRole::Code => "code",
            NumericRole::Sequence => "sequence",
            NumericRole::Count => "count",
            NumericRole::Unknown => "unknown",
        }
    }
}

/// A declared debit/credit polarity profile (`ACCOUNTING.PROFILE.1` / `KOBOLD.BANK.2`). Posting side is
/// derived **only** from `source_field`'s value against the declared value tables — never from a numeric
/// sign, a `CR`/`DB` presentation marker, a transaction code, or an account type. Unknown → fail closed.
pub struct PolarityProfile<'a> {
    pub amount_field: &'a str,
    pub source_field: &'a str,
    pub debit_values: &'a [&'a str],
    pub credit_values: &'a [&'a str],
}

/// The declared accounting profile: numeric roles per field + the polarity profile. Without it, **no
/// accounting totals are claimed** (numeric-role and polarity are refused, not inferred).
pub struct AccountingProfile<'a> {
    pub numeric_roles: &'a [(&'a str, NumericRole)],
    pub polarity: PolarityProfile<'a>,
}

impl AccountingProfile<'_> {
    fn role(&self, field: &str) -> NumericRole {
        self.numeric_roles
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, r)| *r)
            .unwrap_or(NumericRole::Unknown)
    }
}

/// The declared control profile: variant names, trailer-total fields, and the accounting profile.
pub struct ControlSpec<'a> {
    pub detail_variant: &'a str,
    pub trailer_variant: &'a str,
    pub trailer_count_field: &'a str,
    pub trailer_debit_field: &'a str,
    pub trailer_credit_field: &'a str,
    pub accounting: AccountingProfile<'a>,
}

/// The banking reconciliation outcome.
#[non_exhaustive]
pub struct BankingResult {
    pub casefile_json: String,
    pub balanced: bool,
    pub verdict: ExitCode,
    /// SARIF-shaped findings (rule id + message) — non-empty iff the file did not reconcile.
    pub findings: Vec<(String, String)>,
    /// Structured declared-vs-observed numbers (so downstream views read values, not re-parsed JSON).
    pub summary: BankingSummary,
}

/// The declared-vs-observed control numbers, exposed structurally for `BANK.RECONCILE.1`'s operator view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BankingSummary {
    pub declared_count: Option<u64>,
    pub observed_count: u64,
    pub declared_debit_cents: Option<i64>,
    pub observed_debit_cents: i64,
    pub declared_credit_cents: Option<i64>,
    pub observed_credit_cents: i64,
    pub unknown_record_type_count: usize,
    pub unknown_polarity_count: usize,
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

/// Parse a decoded V99 decimal string (e.g. `"-12.30"`, `"100.00"`) to signed integer cents. Returns
/// `None` if the value is not a clean 2-decimal number (kept strict — never coerce dirty data).
fn to_cents(v: &str) -> Option<i64> {
    let neg = v.starts_with('-');
    let t = v.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.')?;
    if fp.len() != 2 || !ip.bytes().chain(fp.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let cents = ip.parse::<i64>().ok()? * 100 + fp.parse::<i64>().ok()?;
    Some(if neg { -cents } else { cents })
}

fn money(cents: i64) -> String {
    format!(
        "{}{}.{:02}",
        if cents < 0 { "-" } else { "" },
        cents.abs() / 100,
        cents.abs() % 100
    )
}

/// Reconcile a header/detail/trailer banking file under a declared variant + control profile.
pub fn reconcile_banking(
    data: &[u8],
    record_len: usize,
    variant: &VariantSpec,
    control: &ControlSpec,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<BankingResult, ShimError> {
    if record_len == 0 {
        return Err(ShimError::Layout("record_len must be > 0".into()));
    }
    let mut findings: Vec<(String, String)> = Vec::new();
    // Profile sanity: the field being summed as money MUST be declared role=Amount (never a rate/id/code).
    if control
        .accounting
        .role(control.accounting.polarity.amount_field)
        != NumericRole::Amount
    {
        findings.push((
            "KOBOLD-BANK-PROFILE-INCONSISTENT".into(),
            format!(
                "polarity.amount_field {:?} is not declared numeric role=amount (refusing to sum)",
                control.accounting.polarity.amount_field
            ),
        ));
    }
    let (mut obs_count, mut obs_debit, mut obs_credit) = (0u64, 0i64, 0i64);
    let mut variant_counts: Vec<(String, u64)> = Vec::new();
    let (mut decl_count, mut decl_debit, mut decl_credit): (Option<u64>, Option<i64>, Option<i64>) =
        (None, None, None);

    let field = |copybook: &str, rec: &[u8], name: &str| -> Option<String> {
        decode_record_encoded(copybook, rec, resolver, encoding)
            .ok()?
            .fields
            .into_iter()
            .find(|f| f.name == name)
            .map(|f| f.value)
    };

    for (idx, rec) in data.chunks(record_len).enumerate() {
        let disc = rec
            .get(
                variant.discriminator_offset
                    ..variant.discriminator_offset + variant.discriminator_len,
            )
            .unwrap_or(&[]);
        let v = variant.variants.iter().find(|v| v.discriminator == disc);
        let Some(v) = v else {
            findings.push((
                "KOBOLD-BANK-UNKNOWN-RECORD-TYPE".into(),
                format!(
                    "record {idx}: unknown discriminator {} (fail closed)",
                    hex(disc)
                ),
            ));
            continue;
        };
        match variant_counts.iter_mut().find(|(n, _)| n == v.name) {
            Some((_, c)) => *c += 1,
            None => variant_counts.push((v.name.to_string(), 1)),
        }
        if v.name == control.detail_variant {
            obs_count += 1;
            let acct = &control.accounting;
            // ONLY the declared Amount field is summed; a numeric rate/identifier/code is never money.
            let amt = field(v.copybook, rec, acct.polarity.amount_field).and_then(|s| to_cents(&s));
            // Polarity comes ONLY from the declared source field + value tables -- never the sign.
            let ind = field(v.copybook, rec, acct.polarity.source_field);
            let ind_t = ind.as_deref().map(str::trim);
            match (amt, ind_t) {
                (Some(c), Some(i)) if acct.polarity.debit_values.contains(&i) => obs_debit += c,
                (Some(c), Some(i)) if acct.polarity.credit_values.contains(&i) => obs_credit += c,
                (Some(_), Some(other)) => findings.push((
                    "KOBOLD-BANK-UNKNOWN-POLARITY".into(),
                    format!("record {idx}: polarity value {other:?} not in declared profile (fail closed)"),
                )),
                _ => findings.push((
                    "KOBOLD-BANK-DIRTY-DETAIL".into(),
                    format!("record {idx}: amount/polarity not cleanly decodable"),
                )),
            }
        } else if v.name == control.trailer_variant {
            decl_count = field(v.copybook, rec, control.trailer_count_field)
                .and_then(|s| s.trim().parse().ok());
            decl_debit =
                field(v.copybook, rec, control.trailer_debit_field).and_then(|s| to_cents(&s));
            decl_credit =
                field(v.copybook, rec, control.trailer_credit_field).and_then(|s| to_cents(&s));
        }
    }

    // declared-vs-observed reconciliation
    let check = |what: &str,
                 decl: Option<i64>,
                 obs: i64,
                 money_fmt: bool,
                 findings: &mut Vec<(String, String)>| {
        match decl {
            Some(d) if d == obs => true,
            Some(d) => {
                let (ds, os) = if money_fmt {
                    (money(d), money(obs))
                } else {
                    (d.to_string(), obs.to_string())
                };
                findings.push((
                    "KOBOLD-BANK-CONTROL-MISMATCH".into(),
                    format!("{what}: declared {ds} != observed {os}"),
                ));
                false
            }
            None => {
                findings.push((
                    "KOBOLD-BANK-NO-TRAILER".into(),
                    format!("{what}: no declared trailer total"),
                ));
                false
            }
        }
    };
    let c_ok = check(
        "record_count",
        decl_count.map(|c| c as i64),
        obs_count as i64,
        false,
        &mut findings,
    );
    let d_ok = check("debit_total", decl_debit, obs_debit, true, &mut findings);
    let r_ok = check("credit_total", decl_credit, obs_credit, true, &mut findings);
    let balanced = c_ok && d_ok && r_ok && findings.is_empty();
    let verdict = if balanced {
        ExitCode::Success
    } else {
        ExitCode::DecodedWithEvidenceWarnings
    };

    // The banking forensic casefile: truth LAYERS, with posting/ledger/business truth explicitly unclaimed.
    let vc = variant_counts
        .iter()
        .map(|(n, c)| format!("{}:{c}", jstr(n).trim_matches('"')))
        .collect::<Vec<_>>()
        .join(",");
    let find_json = findings
        .iter()
        .map(|(r, m)| {
            format!(
                "{{\"ruleId\":{},\"level\":\"warning\",\"message\":{}}}",
                jstr(r),
                jstr(m)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    // The DECLARED accounting profile (ACCOUNTING.PROFILE.1): which fields are amounts (summable) vs
    // identifiers/rates/codes (numeric but never money), and the polarity source -- with the sign and
    // presentation policies stated as explicit refusals.
    let acct = &control.accounting;
    let roles = acct
        .numeric_roles
        .iter()
        .map(|(f, r)| format!("{}:{}", jstr(f), jstr(r.as_str())))
        .collect::<Vec<_>>()
        .join(",");
    let vals = |vs: &[&str]| vs.iter().map(|v| jstr(v)).collect::<Vec<_>>().join(",");
    let acct_json = format!(
        concat!(
            "{{\"schema\":\"kobold-accounting-profile-v1\",\"numeric_roles\":{{{}}},",
            "\"polarity\":{{\"amount_field\":{},\"source_field\":{},\"debit_values\":[{}],",
            "\"credit_values\":[{}],\"unknown_value_policy\":\"fail_closed\",",
            "\"numeric_sign_policy\":\"not_polarity\",\"presentation_marker_policy\":\"not_polarity\",",
            "\"field_name_heuristics\":\"not_used\"}}}}"
        ),
        roles,
        jstr(acct.polarity.amount_field),
        jstr(acct.polarity.source_field),
        vals(acct.polarity.debit_values),
        vals(acct.polarity.credit_values),
    );
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-banking-forensic-casefile-v1\",\"court\":\"KOBOLD.BANK.2 (ACCOUNTING.PROFILE.1)\",",
            "\"input_sha256\":{},\"record_len\":{},\"record_count\":{},",
            "\"byte_truth\":{{\"proven\":true,\"court\":\"KOBOLD.FILE.1 + sealed gnucobol-rs courts\"}},",
            "\"record_truth\":{{\"proven\":true,\"variant_counts\":{{{}}}}},",
            "\"accounting_profile\":{},",
            "\"control_totals\":{{\"declared\":{{\"count\":{},\"debit\":{},\"credit\":{}}},",
            "\"observed\":{{\"count\":{},\"debit\":{},\"credit\":{}}},\"balanced\":{}}},",
            "\"posting_truth\":{{\"claimed\":false,\"requires\":\"declared posting/balancing profile\"}},",
            "\"ledger_truth\":{{\"claimed\":false}},\"business_truth\":{{\"claimed\":false}},",
            "\"negative_capabilities\":[\"NEG.BANKING.SIGN_IS_NOT_POLARITY\",",
            "\"NEG.BANKING.CR_DB_IS_NOT_POSTING_SIDE\",\"NEG.NUMERIC.ROLE\",",
            "\"NEG.IDENTIFIER.NUMERIC_COERCION\",\"NEG.CURRENCY.SCALE\",\"NEG.REVERSAL.INFERENCE\",",
            "\"NEG.BANKING.BALANCED_FILE_IS_NOT_CORRECT_FILE\",",
            "\"NEG.BANKING.TRAILER_MATCH_IS_NOT_LEDGER_ACCEPTANCE\",\"NEG.BANKING.POSTING_TRUTH\"],",
            "\"findings\":[{}],\"verdict\":{}}}\n"
        ),
        jstr(&sha256_hex(data)),
        record_len,
        data.len() / record_len,
        vc,
        acct_json,
        decl_count.map(|c| c.to_string()).unwrap_or("null".into()),
        decl_debit.map(money).map(|s| jstr(&s)).unwrap_or("null".into()),
        decl_credit.map(money).map(|s| jstr(&s)).unwrap_or("null".into()),
        obs_count,
        jstr(&money(obs_debit)),
        jstr(&money(obs_credit)),
        balanced,
        find_json,
        verdict.code(),
    );

    let summary = BankingSummary {
        declared_count: decl_count,
        observed_count: obs_count,
        declared_debit_cents: decl_debit,
        observed_debit_cents: obs_debit,
        declared_credit_cents: decl_credit,
        observed_credit_cents: obs_credit,
        unknown_record_type_count: findings
            .iter()
            .filter(|(r, _)| r == "KOBOLD-BANK-UNKNOWN-RECORD-TYPE")
            .count(),
        unknown_polarity_count: findings
            .iter()
            .filter(|(r, _)| r == "KOBOLD-BANK-UNKNOWN-POLARITY")
            .count(),
    };
    Ok(BankingResult {
        casefile_json,
        balanced,
        verdict,
        findings,
        summary,
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
