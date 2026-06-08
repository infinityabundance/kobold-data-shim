//! KOBOLD.POSTING.1 — declared posting-unit custody manifest.
//!
//! **Doctrine.** KOBOLD.POSTING.1 admits only custody over a *declared* posting unit: batch identity,
//! business date, extraction metadata, record order, sequence evidence, duplicate evidence, and
//! hash-chain continuity are recorded as forensic evidence, while ledger acceptance, settlement
//! finality, account balance correctness, and business truth remain non-claims.
//!
//! It binds the banking spine (`BANK.1` totals, `BANK.2` polarity, `DB2HOST.1` indicators) into one
//! custody record answering *which exact records, in which order, were reconciled* — without claiming
//! the unit was posted, accepted, settled, or business-true.

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// A declared posting-unit profile. `sequence_field`/`txn_id_field` are optional; `sequence_contiguous`
/// enables gap detection only when the caller declares the sequence is meant to be contiguous.
pub struct PostingProfile<'a> {
    pub posting_unit_id: &'a str,
    pub business_date: &'a str,
    pub extract_time_utc: &'a str,
    pub source_system: &'a str,
    pub sequence_field: Option<&'a str>,
    pub sequence_contiguous: bool,
    pub txn_id_field: Option<&'a str>,
}

/// The posting-unit custody result.
pub struct PostingManifest {
    pub manifest_json: String,
    pub casefile_json: String,
    pub record_count: usize,
    pub seq_duplicates: Vec<u64>,
    pub seq_gaps: Vec<u64>,
    pub txn_duplicates: Vec<String>,
    pub last_chain_hash: String,
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

fn field_value(
    copybook: &str,
    rec: &[u8],
    name: &str,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Option<String> {
    decode_record_encoded(copybook, rec, resolver, encoding)
        .ok()?
        .fields
        .into_iter()
        .find(|f| f.name == name)
        .map(|f| f.value)
}

/// Build a declared posting-unit custody manifest over a fixed-record buffer. Records the hash chain over
/// record ORDER, sequence min/max/duplicates/(gaps), and duplicate transaction ids — never posting,
/// ledger, settlement, or business truth.
pub fn posting_manifest(
    copybook: &str,
    data: &[u8],
    record_len: usize,
    profile: &PostingProfile,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<PostingManifest, ShimError> {
    if record_len == 0 {
        return Err(ShimError::Layout("record_len must be > 0".into()));
    }
    if profile.posting_unit_id.is_empty() {
        return Err(ShimError::BadItem("posting_unit_id is required".into()));
    }
    let mut findings: Vec<(String, String)> = Vec::new();
    let mut first_record_hash = String::new();
    // Hash chain over record ORDER: h_0 = sha256(r_0); h_i = sha256(h_{i-1} || r_i). Reordering records
    // changes the chain -> the manifest binds the exact records in the exact order used.
    let mut chain = String::new();
    let mut seq_values: Vec<u64> = Vec::new();
    let mut seq_raw: Vec<String> = Vec::new();
    let mut txn_values: Vec<String> = Vec::new();
    let mut count = 0usize;

    for (i, rec) in data.chunks(record_len).enumerate() {
        count += 1;
        let rh = sha256_hex(rec);
        if i == 0 {
            first_record_hash = rh.clone();
            chain = rh;
        } else {
            let mut buf = chain.into_bytes();
            buf.extend_from_slice(rec);
            chain = sha256_hex(&buf);
        }
        if let Some(sf) = profile.sequence_field {
            match field_value(copybook, rec, sf, resolver, encoding) {
                Some(v) => {
                    seq_raw.push(v.clone());
                    match v.trim().parse::<u64>() {
                        Ok(n) => seq_values.push(n),
                        Err(_) => findings.push((
                            "KOBOLD-POSTING-DIRTY-SEQUENCE".into(),
                            format!("record {i}: sequence {v:?} not numeric"),
                        )),
                    }
                }
                None => findings.push((
                    "KOBOLD-POSTING-NO-SEQUENCE".into(),
                    format!("record {i}: declared sequence field {sf:?} not found"),
                )),
            }
        }
        if let Some(tf) = profile.txn_id_field {
            if let Some(v) = field_value(copybook, rec, tf, resolver, encoding) {
                txn_values.push(v.trim().to_string());
            }
        }
    }

    // sequence min/max/duplicates/gaps
    let (mut smin, mut smax) = (None, None);
    let mut seq_duplicates: Vec<u64> = Vec::new();
    if !seq_values.is_empty() {
        let mut sorted = seq_values.clone();
        sorted.sort_unstable();
        smin = Some(sorted[0]);
        smax = Some(*sorted.last().unwrap());
        for w in sorted.windows(2) {
            if w[0] == w[1] && !seq_duplicates.contains(&w[0]) {
                seq_duplicates.push(w[0]);
            }
        }
    }
    let mut seq_gaps: Vec<u64> = Vec::new();
    if profile.sequence_contiguous {
        if let (Some(lo), Some(hi)) = (smin, smax) {
            let seen: std::collections::HashSet<u64> = seq_values.iter().copied().collect();
            for n in lo..=hi {
                if !seen.contains(&n) {
                    seq_gaps.push(n);
                }
            }
        }
    }
    if !seq_duplicates.is_empty() {
        findings.push((
            "KOBOLD-POSTING-DUPLICATE-SEQUENCE".into(),
            format!("duplicate sequence value(s): {seq_duplicates:?}"),
        ));
    }
    if !seq_gaps.is_empty() {
        findings.push((
            "KOBOLD-POSTING-SEQUENCE-GAP".into(),
            format!("missing sequence value(s) in a declared-contiguous unit: {seq_gaps:?}"),
        ));
    }

    // duplicate transaction ids
    let mut txn_duplicates: Vec<String> = Vec::new();
    {
        let mut seen = std::collections::HashSet::new();
        for t in &txn_values {
            if !seen.insert(t.clone()) && !txn_duplicates.contains(t) {
                txn_duplicates.push(t.clone());
            }
        }
    }
    if !txn_duplicates.is_empty() {
        findings.push((
            "KOBOLD-POSTING-DUPLICATE-TXN-ID".into(),
            format!("duplicate transaction id(s): {txn_duplicates:?}"),
        ));
    }

    let arr_u = |v: &[u64]| {
        v.iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let arr_s = |v: &[String]| v.iter().map(|s| jstr(s)).collect::<Vec<_>>().join(",");
    let seq_json = match profile.sequence_field {
        Some(sf) => format!(
            ",\"sequence\":{{\"field\":{},\"min\":{},\"max\":{},\"contiguous\":{},\"duplicates\":[{}],\"gaps\":[{}]}}",
            jstr(sf),
            smin.map(|n| n.to_string()).unwrap_or("null".into()),
            smax.map(|n| n.to_string()).unwrap_or("null".into()),
            profile.sequence_contiguous,
            arr_u(&seq_duplicates),
            arr_u(&seq_gaps),
        ),
        None => String::new(),
    };
    let txn_json = match profile.txn_id_field {
        Some(tf) => format!(
            ",\"transaction_id\":{{\"field\":{},\"duplicates\":[{}]}}",
            jstr(tf),
            arr_s(&txn_duplicates)
        ),
        None => String::new(),
    };

    let manifest_json = format!(
        concat!(
            "{{\"schema\":\"kobold-posting-unit-manifest-v1\",\"posting_unit_id\":{},\"business_date\":{},",
            "\"extract_time_utc\":{},\"source_system\":{},\"file_hash\":{},\"record_count\":{}{}{},",
            "\"record_order_hash_chain\":{{\"algorithm\":\"sha256\",\"first_record_hash\":{},\"last_chain_hash\":{}}},",
            "\"truth_layers\":{{\"posting_truth\":{{\"claimed\":false}},\"ledger_truth\":{{\"claimed\":false}},",
            "\"business_truth\":{{\"claimed\":false}}}}}}"
        ),
        jstr(profile.posting_unit_id), jstr(profile.business_date), jstr(profile.extract_time_utc),
        jstr(profile.source_system), jstr(&sha256_hex(data)), count, seq_json, txn_json,
        jstr(&first_record_hash), jstr(&chain),
    );

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
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-posting-forensic-casefile-v1\",\"court\":\"KOBOLD.POSTING.1\",",
            "\"manifest\":{},\"findings\":[{}],",
            "\"negative_capabilities\":[\"NEG.POSTING.UNIT\",\"NEG.POSTING.LEDGER_ACCEPTANCE\",",
            "\"NEG.POSTING.SETTLEMENT_FINALITY\",\"NEG.POSTING.ACCOUNT_BALANCE_TRUTH\",",
            "\"NEG.POSTING.SEQUENCE_CONTIGUITY_UNDECLARED\",\"NEG.POSTING.DUPLICATE_NOT_BUSINESS_DUPLICATE\",",
            "\"NEG.CURRENTNESS\"]}}\n"
        ),
        manifest_json, find_json,
    );

    Ok(PostingManifest {
        manifest_json,
        casefile_json,
        record_count: count,
        seq_duplicates,
        seq_gaps,
        txn_duplicates,
        last_chain_hash: chain,
        findings,
    })
}
