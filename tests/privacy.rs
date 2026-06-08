//! KOBOLD.PRIVACY.REDACTION.1 acceptance: declared field-level redaction preserves hashes/provenance/
//! offsets/court while withholding or tokenizing sensitive values; unlisted fields fail closed under a
//! deny-unlisted policy; no anonymization/compliance/reversibility/public-safety claim.

use kobold_data_shim::{
    redact_record, DefaultAction, Encoding, FieldRule, NoCopy, RedactionAction, RedactionPolicy,
};

const CB: &str = "       01 R.\n           05 ACCOUNT-ID PIC 9(6).\n           05 CUSTOMER PIC X(4).\n           05 AMOUNT PIC 9(3).\n";
const REC: &[u8] = b"004217ANNA250"; // ACCOUNT-ID 004217, CUSTOMER ANNA, AMOUNT 250

#[test]
fn redacts_value_keeps_hashes_and_provenance() {
    let rules = [
        FieldRule {
            field: "ACCOUNT-ID",
            action: RedactionAction::RedactValueKeepHash,
        },
        FieldRule {
            field: "CUSTOMER",
            action: RedactionAction::TokenizeDeterministic,
        },
        FieldRule {
            field: "AMOUNT",
            action: RedactionAction::Allow,
        },
    ];
    let p = RedactionPolicy {
        rules: &rules,
        default_action: DefaultAction::AllowUnlisted,
        token_scope: "casefile",
    };
    let r = redact_record(CB, REC, &p, &NoCopy, Encoding::Ascii).unwrap();
    // ACCOUNT-ID value withheld, hashes + offset preserved
    assert!(r.json.contains("\"ACCOUNT-ID\":{\"value\":\"[REDACTED]\""));
    assert!(r.json.contains("\"value_sha256\":") && r.json.contains("\"raw_sha256\":"));
    assert!(r.json.contains("\"offset\":0,\"size\":6"));
    // the cleartext account id must NOT appear
    assert!(
        !r.json.contains("\"value\":\"4217\""),
        "redacted value must not leak"
    );
    // CUSTOMER tokenized (deterministic), not the cleartext
    assert!(r.json.contains("\"CUSTOMER\":{\"value\":\"TOK-") && !r.json.contains("ANNA"));
    // AMOUNT allowed (cleartext) + redacted:false
    assert!(
        r.json.contains("\"AMOUNT\":{\"value\":\"250\"") && r.json.contains("\"redacted\":false")
    );
    // non-claims present
    assert!(
        r.json.contains("NEG.REDACTION.NOT_ANONYMIZATION")
            && r.json.contains("\"public_output_claim\":false")
    );
}

#[test]
fn deterministic_token_is_stable_but_not_reversible() {
    let rules = [FieldRule {
        field: "CUSTOMER",
        action: RedactionAction::TokenizeDeterministic,
    }];
    let p = RedactionPolicy {
        rules: &rules,
        default_action: DefaultAction::AllowUnlisted,
        token_scope: "batch-1",
    };
    let a = redact_record(CB, REC, &p, &NoCopy, Encoding::Ascii).unwrap();
    let b = redact_record(CB, REC, &p, &NoCopy, Encoding::Ascii).unwrap();
    // same value+scope -> same token
    let tok = |s: &str| {
        s.split("\"CUSTOMER\":{\"value\":\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string()
    };
    assert_eq!(tok(&a.json), tok(&b.json));
    assert!(tok(&a.json).starts_with("TOK-"));
    // different scope -> different token (not an identity)
    let p2 = RedactionPolicy {
        rules: &rules,
        default_action: DefaultAction::AllowUnlisted,
        token_scope: "batch-2",
    };
    let c = redact_record(CB, REC, &p2, &NoCopy, Encoding::Ascii).unwrap();
    assert_ne!(tok(&a.json), tok(&c.json));
}

#[test]
fn unlisted_field_fails_closed_under_deny_policy() {
    // strict: only AMOUNT declared -> ACCOUNT-ID + CUSTOMER are unlisted -> withheld + findings
    let rules = [FieldRule {
        field: "AMOUNT",
        action: RedactionAction::Allow,
    }];
    let p = RedactionPolicy {
        rules: &rules,
        default_action: DefaultAction::DenyUnlisted,
        token_scope: "x",
    };
    let r = redact_record(CB, REC, &p, &NoCopy, Encoding::Ascii).unwrap();
    assert!(r
        .findings
        .iter()
        .any(|(rule, m)| rule == "KOBOLD-PRIVACY-UNLISTED-FIELD" && m.contains("ACCOUNT-ID")));
    assert!(r.json.contains("\"ACCOUNT-ID\":{\"value\":\"[REDACTED]\""));
    assert!(
        !r.json.contains("ANNA"),
        "unlisted CUSTOMER withheld under deny policy"
    );
}
