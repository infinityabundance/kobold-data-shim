//! KOBOLD.BATCH.CHAIN.1 — tamper-evident batch custody chain.
//!
//! **Doctrine.** A batch pipeline produces a sequence of runs, each with a declared input file, a control
//! result (e.g. a `BATCH.CONTROL.1` casefile), and an output artifact. BATCH.CHAIN.1 seals each batch into a
//! **custody link** whose hash binds those declared artifact hashes **and its predecessor's link hash**, so
//! the links form a chain: altering any bound artifact, dropping a batch, or reordering the sequence breaks
//! the chain at a detectable point. This is tamper-**evident**, not tamper-proof: it proves the bound bytes
//! are **unchanged since sealing and the order is intact** — it does **not** prove the batch is correct,
//! complete, authorized, or that the inputs are authentic. *A batch chain proves custody integrity over
//! declared artifacts, not batch correctness, authenticity, or authorization.*

use crate::sha256::sha256_hex;

/// The genesis predecessor hash for the first batch in a chain.
pub const GENESIS: &str = "GENESIS";

/// One declared batch's artifact hashes (each a sha256 hex of the bound artifact).
pub struct BatchLink<'a> {
    pub batch_id: &'a str,
    pub input_sha256: &'a str,
    pub control_sha256: &'a str,
    pub output_sha256: &'a str,
}

/// A sealed custody link: the declared hashes plus the chain `prev_hash` and the computed `link_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedLink {
    pub batch_id: String,
    pub input_sha256: String,
    pub control_sha256: String,
    pub output_sha256: String,
    pub prev_hash: String,
    pub link_hash: String,
}

/// The custody-chain result.
pub struct BatchCustody {
    pub manifest_json: String,
    pub casefile_json: String,
    pub sealed: Vec<SealedLink>,
    /// the last link's hash (the chain head).
    pub chain_head: String,
}

/// The verification verdict for a sealed chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainVerdict {
    pub intact: bool,
    /// the first index whose link hash or predecessor link is broken, if any.
    pub broken_at: Option<usize>,
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

/// Deterministic link hash binding the declared artifact hashes and the predecessor hash. Uses a record
/// separator (`0x1e`) so the concatenation is unambiguous.
fn link_hash(batch_id: &str, input: &str, control: &str, output: &str, prev: &str) -> String {
    let bound = format!("{batch_id}\u{1e}{input}\u{1e}{control}\u{1e}{output}\u{1e}{prev}");
    sha256_hex(bound.as_bytes())
}

/// Seal a sequence of declared batches into a custody chain. The first batch's predecessor is `genesis_prev`
/// (use [`GENESIS`] for a fresh chain, or the head of an existing chain to extend it).
pub fn seal_chain(links: &[BatchLink], genesis_prev: &str) -> BatchCustody {
    let mut prev = genesis_prev.to_string();
    let mut sealed: Vec<SealedLink> = Vec::new();
    for l in links {
        let h = link_hash(
            l.batch_id,
            l.input_sha256,
            l.control_sha256,
            l.output_sha256,
            &prev,
        );
        sealed.push(SealedLink {
            batch_id: l.batch_id.to_string(),
            input_sha256: l.input_sha256.to_string(),
            control_sha256: l.control_sha256.to_string(),
            output_sha256: l.output_sha256.to_string(),
            prev_hash: prev.clone(),
            link_hash: h.clone(),
        });
        prev = h;
    }
    let chain_head = sealed
        .last()
        .map(|s| s.link_hash.clone())
        .unwrap_or_default();

    let links_json: Vec<String> = sealed
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                concat!(
                    "{{\"index\":{},\"batch_id\":{},\"input_sha256\":{},\"control_sha256\":{},",
                    "\"output_sha256\":{},\"prev_hash\":{},\"link_hash\":{}}}"
                ),
                i,
                jstr(&s.batch_id),
                jstr(&s.input_sha256),
                jstr(&s.control_sha256),
                jstr(&s.output_sha256),
                jstr(&s.prev_hash),
                jstr(&s.link_hash),
            )
        })
        .collect();
    let manifest_json = format!(
        concat!(
            "{{\"schema\":\"kobold-batch-chain-manifest-v1\",\"court\":\"KOBOLD.BATCH.CHAIN.1\",",
            "\"genesis_prev\":{},\"batch_count\":{},\"links\":[{}],\"chain_head\":{}}}"
        ),
        jstr(genesis_prev),
        sealed.len(),
        links_json.join(","),
        jstr(&chain_head),
    );
    let casefile_json = format!(
        concat!(
            "{{\"schema\":\"kobold-batch-chain-forensic-casefile-v1\",\"court\":\"KOBOLD.BATCH.CHAIN.1\",",
            "\"manifest\":{},\"truth_layers\":{{\"custody_integrity_truth\":true,\"order_truth\":true,",
            "\"authenticity\":false,\"batch_correctness\":false,\"authorization\":false}},",
            "\"negative_capabilities\":[\"NEG.BATCH_CHAIN.TAMPER_EVIDENT_NOT_TAMPER_PROOF\",",
            "\"NEG.BATCH_CHAIN.NO_AUTHENTICITY\",\"NEG.BATCH_CHAIN.NO_BATCH_CORRECTNESS\",",
            "\"NEG.BATCH_CHAIN.REQUIRES_DECLARED_ARTIFACTS\",\"NEG.BATCH_CHAIN.NO_TIMESTAMP_TRUTH\",",
            "\"NEG.BATCH_CHAIN.WRITE_BACK_NOT_CLAIMED\"]}}\n"
        ),
        manifest_json,
    );

    BatchCustody {
        manifest_json,
        casefile_json,
        sealed,
        chain_head,
    }
}

/// Verify a sealed chain: each link's hash must recompute from its bound artifacts + recorded `prev_hash`, and
/// each `prev_hash` must equal the predecessor's `link_hash`. Returns the first broken index, if any.
pub fn verify_chain(sealed: &[SealedLink], genesis_prev: &str) -> ChainVerdict {
    let mut expected_prev = genesis_prev.to_string();
    for (i, s) in sealed.iter().enumerate() {
        if s.prev_hash != expected_prev {
            return ChainVerdict {
                intact: false,
                broken_at: Some(i),
            };
        }
        let recomputed = link_hash(
            &s.batch_id,
            &s.input_sha256,
            &s.control_sha256,
            &s.output_sha256,
            &s.prev_hash,
        );
        if recomputed != s.link_hash {
            return ChainVerdict {
                intact: false,
                broken_at: Some(i),
            };
        }
        expected_prev = s.link_hash.clone();
    }
    ChainVerdict {
        intact: true,
        broken_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Vec<SealedLink> {
        let links = [
            BatchLink {
                batch_id: "B1",
                input_sha256: "aa",
                control_sha256: "bb",
                output_sha256: "cc",
            },
            BatchLink {
                batch_id: "B2",
                input_sha256: "dd",
                control_sha256: "ee",
                output_sha256: "ff",
            },
            BatchLink {
                batch_id: "B3",
                input_sha256: "11",
                control_sha256: "22",
                output_sha256: "33",
            },
        ];
        seal_chain(&links, GENESIS).sealed
    }

    #[test]
    fn fresh_chain_verifies_intact() {
        let c = chain();
        assert_eq!(
            verify_chain(&c, GENESIS),
            ChainVerdict {
                intact: true,
                broken_at: None
            }
        );
        // links are actually chained: each prev_hash == prior link_hash
        assert_eq!(c[1].prev_hash, c[0].link_hash);
        assert_eq!(c[2].prev_hash, c[1].link_hash);
    }

    #[test]
    fn tampering_a_bound_artifact_breaks_the_chain() {
        let mut c = chain();
        // tamper with batch 2's input hash without recomputing its link hash
        c[1].input_sha256 = "TAMPERED".to_string();
        let v = verify_chain(&c, GENESIS);
        assert!(!v.intact);
        assert_eq!(v.broken_at, Some(1));
    }

    #[test]
    fn reordering_breaks_the_chain() {
        let mut c = chain();
        c.swap(1, 2); // reorder batches
        let v = verify_chain(&c, GENESIS);
        assert!(!v.intact);
        assert_eq!(v.broken_at, Some(1));
    }

    #[test]
    fn extending_an_existing_chain_continues_it() {
        let base = chain();
        let head = base.last().unwrap().link_hash.clone();
        let more = [BatchLink {
            batch_id: "B4",
            input_sha256: "44",
            control_sha256: "55",
            output_sha256: "66",
        }];
        let ext = seal_chain(&more, &head);
        assert_eq!(ext.sealed[0].prev_hash, head);
        assert!(verify_chain(&ext.sealed, &head).intact);
    }
}
