//! KOBOLD.LAYOUT.REDEFINES.2 — explicit overlapping-view manifest.
//!
//! **Doctrine.** LAYOUT.REDEFINES.2 admits overlapping storage views as **byte evidence**: multiple
//! `REDEFINES` interpretations of the same bytes may be decoded and audited, but the semantically **active**
//! view is claimed only when a declared discriminator/profile admits it; otherwise `active_view` stays
//! `claimed:false` and business meaning is refused. *A REDEFINES layout proves overlapping byte VIEWS, not
//! which view is semantically active.*

use crate::sha256::sha256_hex;
use crate::{decode_record_encoded, CopyResolver, Encoding, ShimError};

/// A declared discriminator that admits an active view: the value of `field` selects a `view_id`.
pub struct RedefinesDiscriminator<'a> {
    pub field: &'a str,
    pub mapping: &'a [(&'a str, &'a str)],
}

/// The overlapping-view manifest result.
pub struct RedefinesManifest {
    pub manifest_json: String,
    pub casefile_json: String,
    /// `Some(view_id)` only when a declared discriminator admitted it; `None` otherwise.
    pub active_view: Option<String>,
    pub region_count: usize,
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

struct Item {
    level: u16,
    name: String,
    has_pic: bool,
    redefines: Option<String>,
}

/// Parse copybook lines into a flat item list (level, name, elementary?, redefines target).
fn parse_items(copybook: &str) -> Vec<Item> {
    let mut items = Vec::new();
    for line in copybook.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('*') {
            continue;
        }
        let toks: Vec<&str> = t.trim_end_matches('.').split_whitespace().collect();
        if toks.len() < 2 {
            continue;
        }
        let Ok(level) = toks[0].parse::<u16>() else {
            continue;
        };
        if level == 88 {
            continue; // condition names are not storage views
        }
        let name = toks[1].to_string();
        let up: Vec<String> = toks.iter().map(|s| s.to_ascii_uppercase()).collect();
        let has_pic = up.iter().any(|s| s == "PIC" || s == "PICTURE");
        let redefines = up
            .iter()
            .position(|s| s == "REDEFINES")
            .and_then(|i| toks.get(i + 1))
            .map(|s| s.to_string());
        items.push(Item {
            level,
            name,
            has_pic,
            redefines,
        });
    }
    items
}

/// Elementary member field names under item `i` (descendants with a higher level, until the level returns).
fn members(items: &[Item], i: usize) -> Vec<String> {
    if items[i].has_pic {
        return vec![items[i].name.clone()]; // an elementary REDEFINES target/redefiner is its own member
    }
    let lvl = items[i].level;
    let mut out = Vec::new();
    for it in &items[i + 1..] {
        if it.level <= lvl {
            break;
        }
        if it.has_pic {
            out.push(it.name.clone());
        }
    }
    out
}

/// Build the overlapping-view manifest. Every view is decoded over the SAME shared bytes; `active_view`
/// is admitted only by a declared discriminator.
pub fn redefines_manifest(
    copybook: &str,
    record: &[u8],
    resolver: &impl CopyResolver,
    encoding: Encoding,
    discriminator: Option<&RedefinesDiscriminator>,
) -> Result<RedefinesManifest, ShimError> {
    let decoded = decode_record_encoded(copybook, record, resolver, encoding)?;
    let fmap: std::collections::HashMap<&str, (&str, usize, usize)> = decoded
        .fields
        .iter()
        .map(|f| (f.name.as_str(), (f.value.as_str(), f.offset, f.size)))
        .collect();
    let items = parse_items(copybook);

    // Group by REDEFINES target: target_name -> [target item index, redefiner item indices...]
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, it) in items.iter().enumerate() {
        if let Some(target) = &it.redefines {
            let g = groups.entry(target.clone()).or_insert_with(|| {
                order.push(target.clone());
                // seed with the target item itself (the base view)
                items
                    .iter()
                    .position(|x| &x.name == target)
                    .map(|ti| vec![ti])
                    .unwrap_or_default()
            });
            g.push(i);
        }
    }

    // discriminator value (if declared)
    let mut active_view: Option<String> = None;
    let mut declared_by: Option<String> = None;
    if let Some(d) = discriminator {
        if let Some((val, _, _)) = fmap.get(d.field) {
            if let Some((_, vid)) = d.mapping.iter().find(|(v, _)| v == val) {
                active_view = Some(vid.to_string());
                declared_by = Some(format!("discriminator {}={}", d.field, val));
            }
        }
    }

    let mut region_json: Vec<String> = Vec::new();
    for target in &order {
        let idxs = &groups[target];
        // region offset/length from the decoded member fields across all views
        let mut lo = usize::MAX;
        let mut hi = 0usize;
        let mut views_json: Vec<String> = Vec::new();
        for &vi in idxs {
            let view_id = &items[vi].name;
            let mut fields_json: Vec<String> = Vec::new();
            let mut decoded_json: Vec<String> = Vec::new();
            for m in members(&items, vi) {
                if let Some((val, off, sz)) = fmap.get(m.as_str()) {
                    lo = lo.min(*off);
                    hi = hi.max(off + sz);
                    fields_json.push(format!(
                        "{{\"name\":{},\"offset\":{},\"length\":{}}}",
                        jstr(&m),
                        off,
                        sz
                    ));
                    decoded_json.push(format!("{}:{}", jstr(&m), jstr(val)));
                }
            }
            let is_active = active_view.as_deref() == Some(view_id.as_str());
            views_json.push(format!(
                "{{\"view_id\":{},\"redefines\":{},\"layout_valid\":true,\"is_active\":{},\"fields\":[{}],\"decoded\":{{{}}}}}",
                jstr(view_id),
                items[vi].redefines.as_deref().map(jstr).unwrap_or_else(|| "null".into()),
                is_active,
                fields_json.join(","),
                decoded_json.join(","),
            ));
        }
        let (off, len) = if lo == usize::MAX {
            (0, 0)
        } else {
            (lo, hi - lo)
        };
        let raw = record.get(off..off + len).unwrap_or(&[]);
        region_json.push(format!(
            "{{\"storage_region\":{{\"offset\":{},\"length\":{},\"raw_sha256\":{}}},\"views\":[{}]}}",
            off, len, jstr(&sha256_hex(raw)), views_json.join(","),
        ));
    }

    let active_json = match (&active_view, &declared_by) {
        (Some(v), Some(by)) => format!(
            "{{\"claimed\":true,\"view_id\":{},\"declared_by\":{}}}",
            jstr(v),
            jstr(by)
        ),
        _ => "{\"claimed\":false,\"view_id\":null,\"declared_by\":null}".to_string(),
    };
    let manifest_json = format!(
        concat!(
            "{{\"schema\":\"kobold-redefines-view-manifest-v1\",\"court\":\"KOBOLD.LAYOUT.REDEFINES.2\",",
            "\"record_sha256\":{},\"region_count\":{},\"regions\":[{}],\"active_view\":{}}}"
        ),
        jstr(&sha256_hex(record)), order.len(), region_json.join(","), active_json,
    );
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-redefines-forensic-casefile-v1\",\"court\":\"KOBOLD.LAYOUT.REDEFINES.2\",",
            "\"manifest\":{},\"truth_layers\":{{\"byte_view_truth\":true,\"active_view_truth\":{},\"business_meaning\":false}},",
            "\"negative_capabilities\":[\"NEG.REDEFINES.ACTIVE_VIEW_NOT_INFERRED\",\"NEG.REDEFINES.LAYOUT_VALID_NOT_BUSINESS_MEANING\",",
            "\"NEG.REDEFINES.MULTI_VIEW_TRUTH_NOT_CLAIMED\",\"NEG.REDEFINES.DISCRIMINATOR_REQUIRED\",",
            "\"NEG.REDEFINES.WRITE_BACK_NOT_CLAIMED\"]}}\n"
        ),
        manifest_json, active_view.is_some(),
    );

    Ok(RedefinesManifest {
        manifest_json,
        casefile_json,
        active_view,
        region_count: order.len(),
    })
}
