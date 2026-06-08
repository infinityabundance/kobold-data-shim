//! KOBOLD.PILOT-PACKET.1 acceptance: a pilot packet bundles a run's court artifacts by sha256 + an operator
//! checklist + a review-notes HASH (never embedded); a changed artifact changes the packet; missing required
//! artifacts are flagged; it claims pilot evidence only (not certification/compliance/approval/acceptance).

use kobold_data_shim::{pilot_packet, PilotArtifact, PilotInputs};

fn arts() -> Vec<PilotArtifact<'static>> {
    vec![
        PilotArtifact {
            name: "extract_profile",
            court: "KOBOLD.EXTRACT.PROFILE.1",
            content: "{\"schema\":\"kobold-extract-provenance-v1\"}",
        },
        PilotArtifact {
            name: "redaction_policy",
            court: "KOBOLD.PRIVACY.REDACTION.1",
            content: "{\"rules\":[\"NAME\"]}",
        },
        PilotArtifact {
            name: "bank_reconcile",
            court: "KOBOLD.BANK.RECONCILE.1",
            content: "{\"verdict\":\"matched\"}",
        },
        PilotArtifact {
            name: "diff",
            court: "KOBOLD.DIFF.1",
            content: "{\"oracle_status\":\"not_oracle\"}",
        },
        PilotArtifact {
            name: "tooling_export",
            court: "KOBOLD.TOOLING.EXPORT.1",
            content: "{\"fields\":[]}",
        },
        PilotArtifact {
            name: "scale_receipt",
            court: "KOBOLD.SCALE.1",
            content: "{\"records\":1000000}",
        },
    ]
}
fn inputs<'a>(a: &'a [PilotArtifact<'a>], notes: &'a str) -> PilotInputs<'a> {
    PilotInputs {
        pilot_id: "pilot-2026-001",
        business_date: "2026-06-08",
        source_system: "DDA",
        copybook: "01 REC.\n  05 ACCT PIC 9(6).\n",
        operator_review_notes: notes,
        artifacts: a,
    }
}

#[test]
fn packet_binds_artifacts_and_hashes_review_notes() {
    let a = arts();
    let secret = "operator notes: account 123456 looked off; escalated to ops lead";
    let p = pilot_packet(&inputs(&a, secret));
    assert!(
        p.findings.is_empty(),
        "all required artifacts present: {:?}",
        p.findings
    );
    // derived view, no new truth
    assert!(
        p.packet_json.contains("\"derived_view\":true")
            && p.packet_json.contains("\"creates_new_truth\":false")
    );
    // every artifact pinned by sha256 under its court
    assert!(p.packet_json.contains(
        "\"name\":\"extract_profile\",\"court\":\"KOBOLD.EXTRACT.PROFILE.1\",\"sha256\":"
    ));
    assert!(
        p.packet_json.contains("\"name\":\"bank_reconcile\"")
            && p.packet_json.contains("\"name\":\"diff\"")
    );
    // review notes are HASHED, never embedded -> the cleartext must not appear
    assert!(p.packet_json.contains("\"review_notes_embedded\":false"));
    assert!(
        !p.packet_json.contains("123456") && !p.packet_md.contains("123456"),
        "review notes must not leak"
    );
    assert!(p.packet_json.contains("\"complete\":true"));
    assert!(
        p.packet_json.contains("NEG.PILOT.NOT_CERTIFICATION")
            && p.packet_json.contains("NEG.PILOT.NO_NEW_TRUTH")
    );
}

#[test]
fn changed_artifact_changes_the_packet() {
    let a1 = arts();
    let p1 = pilot_packet(&inputs(&a1, "ok"));
    let mut a2 = arts();
    a2[2].content = "{\"verdict\":\"mismatch\"}"; // a different bank_reconcile report
    let p2 = pilot_packet(&inputs(&a2, "ok"));
    assert_ne!(
        p1.packet_json, p2.packet_json,
        "a changed source artifact must change the packet"
    );
}

#[test]
fn missing_required_artifact_is_flagged() {
    let a = vec![PilotArtifact {
        name: "diff",
        court: "KOBOLD.DIFF.1",
        content: "{}",
    }]; // no extract/redaction/bank
    let p = pilot_packet(&inputs(&a, "x"));
    assert!(
        p.findings
            .iter()
            .filter(|(r, _)| r == "KOBOLD-PILOT-MISSING-REQUIRED")
            .count()
            == 3
    );
    assert!(p.packet_json.contains("\"complete\":false"));
}
