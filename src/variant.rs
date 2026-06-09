//! KOBOLD.VARIANT.1 — header/detail/trailer record-discriminator routing.
//!
//! **Doctrine.** A flat file is rarely one layout: it interleaves record types (header, detail(s), trailer)
//! distinguished by a **discriminator field** (e.g. a record-type code in column 1). VARIANT.1 routes each
//! record to a **declared** layout by its discriminator value and decodes it under that layout, composing the
//! sealed decode courts. A record whose discriminator matches **no declared type is REFUSED** (`matched:false`,
//! no decode) — it is never guessed onto a layout. VARIANT.1 proves *which declared record type each record's
//! discriminator selects, and the bytes decoded under that layout* — it does **not** infer the type, validate
//! header/trailer control totals, enforce record-ordering rules, or claim record-stream business meaning.
//! *A variant routing proves discriminator-selected layout decoding, not control-total or sequence truth.*

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// One declared record type: the discriminator byte value that selects it and the copybook to decode it.
pub struct VariantType<'a> {
    pub name: &'a str,
    /// The bytes at the discriminator position that select this type (e.g. `b"H"`).
    pub discriminator_value: &'a [u8],
    pub copybook: &'a str,
}

/// The variant spec: where the discriminator lives in every record, and the declared types.
pub struct VariantSpec<'a> {
    pub discriminator_offset: usize,
    pub discriminator_length: usize,
    pub types: &'a [VariantType<'a>],
}

/// The routing result.
pub struct VariantRouting {
    pub manifest_json: String,
    pub casefile_json: String,
    /// records routed to a declared type.
    pub routed: usize,
    /// records whose discriminator matched no declared type (refused / fail closed).
    pub unmatched: usize,
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

/// Render bytes as a printable-ASCII string (non-printable bytes shown as `.`), for the discriminator label.
fn printable(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}

/// Route each record of a multi-type flat file to a declared layout by its discriminator value, decoding the
/// matched records under their declared copybook. Unmatched records are refused (no decode). `records` are the
/// already-split fixed records.
pub fn variant_route(
    records: &[&[u8]],
    spec: &VariantSpec,
    resolver: &impl CopyResolver,
    encoding: Encoding,
) -> Result<VariantRouting, ShimError> {
    let dstart = spec.discriminator_offset;
    let dend = spec.discriminator_offset + spec.discriminator_length;

    let mut routed = 0usize;
    let mut unmatched = 0usize;
    let mut by_type: Vec<(String, usize)> =
        spec.types.iter().map(|t| (t.name.to_string(), 0)).collect();
    let mut rec_json: Vec<String> = Vec::new();
    let mut file_bytes: Vec<u8> = Vec::new();

    for (i, rec) in records.iter().enumerate() {
        file_bytes.extend_from_slice(rec);
        let disc = rec.get(dstart..dend).unwrap_or(&[]);
        let matched = spec.types.iter().find(|t| t.discriminator_value == disc);

        let body = if let Some(t) = matched {
            routed += 1;
            if let Some(slot) = by_type.iter_mut().find(|(n, _)| n == t.name) {
                slot.1 += 1;
            }
            let decoded = decode_record_encoded(t.copybook, rec, resolver, encoding)?;
            let mut fields_json: Vec<String> = Vec::new();
            let mut decoded_json: Vec<String> = Vec::new();
            for f in &decoded.fields {
                if f.category == "group" {
                    continue;
                }
                fields_json.push(format!(
                    "{{\"name\":{},\"offset\":{},\"length\":{}}}",
                    jstr(&f.name),
                    f.offset,
                    f.size
                ));
                decoded_json.push(format!("{}:{}", jstr(&f.name), jstr(&f.value)));
            }
            format!(
                "\"matched\":true,\"record_type\":{},\"fields\":[{}],\"decoded\":{{{}}}",
                jstr(t.name),
                fields_json.join(","),
                decoded_json.join(","),
            )
        } else {
            unmatched += 1;
            "\"matched\":false,\"record_type\":null,\"fields\":[],\"decoded\":null".to_string()
        };

        rec_json.push(format!(
            "{{\"index\":{},\"record_sha256\":{},\"discriminator\":{{\"offset\":{},\"length\":{},\"value\":{},\"hex\":{}}},{}}}",
            i,
            jstr(&sha256_hex(rec)),
            dstart,
            spec.discriminator_length,
            jstr(&printable(disc)),
            jstr(&disc.iter().map(|b| format!("{b:02x}")).collect::<String>()),
            body,
        ));
    }

    let by_type_json: Vec<String> = by_type
        .iter()
        .map(|(n, c)| format!("{}:{}", jstr(n), c))
        .collect();
    let manifest_json = format!(
        concat!(
            "{{\"schema\":\"kobold-variant-routing-manifest-v1\",\"court\":\"KOBOLD.VARIANT.1\",",
            "\"file_sha256\":{},\"record_count\":{},\"routed\":{},\"unmatched\":{},",
            "\"by_type\":{{{}}},\"records\":[{}]}}"
        ),
        jstr(&sha256_hex(&file_bytes)),
        records.len(),
        routed,
        unmatched,
        by_type_json.join(","),
        rec_json.join(","),
    );
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-variant-routing-forensic-casefile-v1\",\"court\":\"KOBOLD.VARIANT.1\",",
            "\"manifest\":{},\"truth_layers\":{{\"routing_truth\":true,\"byte_view_truth\":true,",
            "\"control_total_truth\":false,\"record_order_truth\":false,\"business_meaning\":false}},",
            "\"negative_capabilities\":[\"NEG.VARIANT.ROUTING_REQUIRES_DECLARED_DISCRIMINATOR\",",
            "\"NEG.VARIANT.UNMATCHED_FAILS_CLOSED\",\"NEG.VARIANT.NO_CONTROL_TOTAL_VALIDATION\",",
            "\"NEG.VARIANT.NO_RECORD_ORDER_SEMANTICS\",\"NEG.VARIANT.NO_BUSINESS_MEANING\",",
            "\"NEG.VARIANT.WRITE_BACK_NOT_CLAIMED\"]}}\n"
        ),
        manifest_json,
    );

    Ok(VariantRouting {
        manifest_json,
        casefile_json,
        routed,
        unmatched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoCopy;

    const HEADER: &str = "01 H-REC.\n  05 H-TYPE PIC X.\n  05 H-DATE PIC 9(8).";
    const DETAIL: &str =
        "01 D-REC.\n  05 D-TYPE PIC X.\n  05 D-ACCT PIC X(4).\n  05 D-AMT PIC 9(4).";
    const TRAILER: &str = "01 T-REC.\n  05 T-TYPE PIC X.\n  05 T-COUNT PIC 9(8).";

    fn spec<'a>() -> VariantSpec<'a> {
        VariantSpec {
            discriminator_offset: 0,
            discriminator_length: 1,
            types: &[
                VariantType {
                    name: "header",
                    discriminator_value: b"H",
                    copybook: HEADER,
                },
                VariantType {
                    name: "detail",
                    discriminator_value: b"D",
                    copybook: DETAIL,
                },
                VariantType {
                    name: "trailer",
                    discriminator_value: b"T",
                    copybook: TRAILER,
                },
            ],
        }
    }

    #[test]
    fn routes_each_record_type_and_refuses_unmatched() {
        let recs: Vec<&[u8]> = vec![
            b"H20240101", // header
            b"DACCT0042", // detail: D + ACCT "ACCT" + 0042
            b"DACCT0099", // detail
            b"X12345678", // unmatched discriminator 'X'
            b"T00000003", // trailer count 3
        ];
        let r = variant_route(&recs, &spec(), &NoCopy, Encoding::Ascii).unwrap();
        assert_eq!(r.routed, 4);
        assert_eq!(r.unmatched, 1);
        // the unmatched record is refused, not decoded
        assert!(r
            .manifest_json
            .contains("\"matched\":false,\"record_type\":null"));
        // by-type counts: 1 header, 2 detail, 1 trailer
        assert!(r.manifest_json.contains("\"header\":1"));
        assert!(r.manifest_json.contains("\"detail\":2"));
        assert!(r.manifest_json.contains("\"trailer\":1"));
        // a detail record decoded its amount under the detail layout
        assert!(
            r.manifest_json.contains("\"D-AMT\":\"42\"")
                || r.manifest_json.contains("\"D-AMT\":\"0042\"")
        );
    }

    #[test]
    fn discriminator_can_be_multi_byte_and_offset() {
        // discriminator at offset 2, length 2
        let types = [VariantType {
            name: "ah",
            discriminator_value: b"AH",
            copybook: "01 R.\n  05 PAD PIC X(2).\n  05 K PIC X(2).\n  05 V PIC 9(3).",
        }];
        let s = VariantSpec {
            discriminator_offset: 2,
            discriminator_length: 2,
            types: &types,
        };
        let recs: Vec<&[u8]> = vec![b"xxAH123", b"xxZZ999"];
        let r = variant_route(&recs, &s, &NoCopy, Encoding::Ascii).unwrap();
        assert_eq!(r.routed, 1);
        assert_eq!(r.unmatched, 1);
    }
}
