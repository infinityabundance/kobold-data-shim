//! KOBOLD.PILOT-PACKET.1 — a hash-bound PILOT evidence packet for a reviewer/operator.
//!
//! **Doctrine.** PILOT-PACKET.1 is a *generated derived view* over a pilot run's existing court artifacts
//! (EXTRACT.PROFILE.1, the redaction policy, BANK.RECONCILE.1, DIFF.1, TOOLING.EXPORT.1, the SCALE receipt,
//! DSSE verification), each **pinned by sha256**, plus an operator checklist and a **review-notes hash** (the
//! notes are hashed, never embedded). It is a **pilot evidence packet** — *not certification, not compliance,
//! not production approval, not customer acceptance*. It creates **no new truth** and a changed source
//! artifact changes the packet hash.

use crate::sha256::sha256_hex;

/// One pilot evidence artifact, pinned by sha256 (its content is hashed, not embedded in full).
pub struct PilotArtifact<'a> {
    pub name: &'a str,
    pub court: &'a str,
    pub content: &'a str,
}

/// The inputs to a pilot packet: identity + the copybook + the run's artifacts + the operator's review notes.
pub struct PilotInputs<'a> {
    pub pilot_id: &'a str,
    pub business_date: &'a str,
    pub source_system: &'a str,
    pub copybook: &'a str,
    /// Free-text operator review notes — **hashed**, never embedded (no cleartext, no PII leak).
    pub operator_review_notes: &'a str,
    /// The pilot run's court artifacts (e.g. `extract_profile`, `redaction_policy`, `bank_reconcile`,
    /// `diff`, `tooling_export`, `scale_receipt`, `dsse_verification`), each pinned by sha256.
    pub artifacts: &'a [PilotArtifact<'a>],
}

/// The generated pilot packet.
pub struct PilotPacket {
    pub packet_json: String,
    pub packet_md: String,
    pub findings: Vec<(String, String)>,
}

/// The artifact names a defensible pilot packet should carry; absence is flagged (not fatal).
const REQUIRED: &[&str] = &["extract_profile", "redaction_policy", "bank_reconcile"];
/// A reviewer's standing checklist for a pilot run (the packet provides it; the reviewer completes it and the
/// completion is captured in `operator_review_notes`, pinned by hash).
const CHECKLIST: &[&str] = &[
    "declared copybook confirmed current for this extract (EXTRACT.PROFILE.1 copybook freshness is a permanent uncertainty)",
    "redaction policy reviewed and applied before any extract left the secure zone (PRIVACY.REDACTION.1)",
    "BANK.1 declared-vs-observed control totals matched, or the mismatch is acknowledged in the notes",
    "BANK.2 polarity came only from declared value tables; no sign-as-polarity inference",
    "DIFF.1 target oracle-status recorded; a match was read as equality-to-declared, not correctness",
    "no real customer data appears in any artifact shared outside the secure zone",
    "truth boundaries acknowledged: this packet is pilot evidence, not ledger/settlement/business truth",
];

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

/// Generate a pilot evidence packet from a pilot run's existing court artifacts. Each artifact is pinned by
/// sha256; the review notes are pinned by hash (never embedded). Missing required artifacts are flagged.
pub fn pilot_packet(inputs: &PilotInputs) -> PilotPacket {
    let mut findings: Vec<(String, String)> = Vec::new();
    let present: std::collections::HashSet<&str> =
        inputs.artifacts.iter().map(|a| a.name).collect();
    for req in REQUIRED {
        if !present.contains(req) {
            findings.push((
                "KOBOLD-PILOT-MISSING-REQUIRED".into(),
                format!("required pilot artifact {req:?} absent — the packet is incomplete"),
            ));
        }
    }

    let src = inputs
        .artifacts
        .iter()
        .map(|a| {
            format!(
                "{{\"name\":{},\"court\":{},\"sha256\":{}}}",
                jstr(a.name),
                jstr(a.court),
                jstr(&sha256_hex(a.content.as_bytes()))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let checklist = CHECKLIST
        .iter()
        .map(|c| jstr(c))
        .collect::<Vec<_>>()
        .join(",");
    let review_hash = sha256_hex(inputs.operator_review_notes.as_bytes());

    let packet_json = format!(
        concat!(
            "{{\"schema\":\"kobold-pilot-packet-v1\",\"court\":\"PILOT-PACKET.1\",",
            "\"pilot_id\":{},\"business_date\":{},\"source_system\":{},",
            "\"derived_view\":true,\"creates_new_truth\":false,",
            "\"copybook_sha256\":{},\"source_evidence\":[{}],",
            "\"operator_checklist\":[{}],\"review_notes_sha256\":{},\"review_notes_embedded\":false,",
            "\"truth_boundary_summary\":\"pilot evidence over named hash-pinned court artifacts; bytes < record < transform < custody < reconciliation evidence; REFUSED: posting, ledger, settlement, account-balance, business truth, certification, compliance, production approval, customer acceptance\",",
            "\"complete\":{},",
            "\"negative_capabilities\":[\"NEG.PILOT.NOT_CERTIFICATION\",\"NEG.PILOT.NOT_COMPLIANCE\",",
            "\"NEG.PILOT.NOT_PRODUCTION_APPROVAL\",\"NEG.PILOT.NOT_CUSTOMER_ACCEPTANCE\",",
            "\"NEG.PILOT.SNAPSHOT_NOT_LIVE\",\"NEG.PILOT.NO_NEW_TRUTH\"]}}\n"
        ),
        jstr(inputs.pilot_id), jstr(inputs.business_date), jstr(inputs.source_system),
        jstr(&sha256_hex(inputs.copybook.as_bytes())), src,
        checklist, jstr(&review_hash), findings.is_empty(),
    );

    let mut md = String::new();
    md.push_str(&format!(
        "# Pilot evidence packet — {}\n\n",
        inputs.pilot_id
    ));
    md.push_str("> [!IMPORTANT]\n> A **pilot evidence packet** — a generated derived view over named, hash-pinned court artifacts. ");
    md.push_str("**Not** certification, compliance, production approval, or customer acceptance. It creates **no new truth**.\n\n");
    md.push_str(&format!(
        "- business date: {} · source: {}\n",
        inputs.business_date, inputs.source_system
    ));
    md.push_str(&format!(
        "- artifacts: **{}**  ·  complete: **{}**  ·  review-notes hash: `{}…`\n\n",
        inputs.artifacts.len(),
        findings.is_empty(),
        &review_hash[..16]
    ));
    md.push_str(
        "## Source evidence (hash-pinned)\n\n| artifact | court | sha256 |\n|---|---|---|\n",
    );
    for a in inputs.artifacts {
        md.push_str(&format!(
            "| `{}` | `{}` | `{}…` |\n",
            a.name,
            a.court,
            &sha256_hex(a.content.as_bytes())[..16]
        ));
    }
    md.push_str("\n## Operator review checklist\n\n");
    for c in CHECKLIST {
        md.push_str(&format!("- [ ] {c}\n"));
    }
    md.push('\n');

    PilotPacket {
        packet_json,
        packet_md: md,
        findings,
    }
}
