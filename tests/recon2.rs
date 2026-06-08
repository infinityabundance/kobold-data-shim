//! KOBOLD.RECON.2 acceptance: declared transformed-record reconciliation. A named sealed transform
//! (SET 88 TRUE / ADD / SUBTRACT) takes input bytes to output bytes; both decode; an audit delta is
//! produced; truth layers above transform-truth stay claimed:false; undeclared transforms fail closed.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};
use kobold_data_shim::{
    decode_record_encoded, reconcile_transform, Encoding, NoCopy, Transform, TransformOp,
};

const CB: &str = "       01 REC.\n           05 STATUS-CODE PIC X.\n               88 ACTIVE VALUE \"A\".\n               88 CLOSED VALUE \"C\".\n           05 BALANCE PIC S9(3)V99 COMP-3.\n";

fn comp3(pic: &str, value: &str) -> Vec<u8> {
    let pf = build_field(pic, Usage::Comp3, false, false).unwrap();
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    let (have, scale) = (fp.len() as i16, pf.attr.scale);
    if scale > have {
        d.resize(d.len() + (scale - have) as usize, 0);
    }
    while d.len() < pf.attr.digits as usize {
        d.insert(0, 0);
    }
    let extra = d.len().saturating_sub(pf.attr.digits as usize);
    d.drain(0..extra);
    let mut src: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    if neg {
        if let Some(l) = src.last_mut() {
            *l |= 0x40;
        }
    }
    let sattr = FieldAttr {
        field_type: COB_TYPE_NUMERIC_DISPLAY,
        digits: pf.attr.digits,
        scale: pf.attr.scale,
        flags: COB_FLAG_HAVE_SIGN,
    };
    let mut out = vec![0u8; pf.size];
    cob_move(&src, &sattr, &mut out, &pf.attr).unwrap();
    out
}

fn record(status: u8, bal: &str) -> Vec<u8> {
    let mut r = vec![status];
    r.extend(comp3("S9(3)V99", bal));
    r
}

#[test]
fn set_88_true_and_add_compose() {
    let input = record(b'C', "100.00"); // CLOSED, 100.00
    let transforms = [
        Transform {
            op: TransformOp::SetTrue,
            target: "ACTIVE",
            amount: None,
        },
        Transform {
            op: TransformOp::Add,
            target: "BALANCE",
            amount: Some("50.00"),
        },
    ];
    let r = reconcile_transform(CB, &input, &transforms, &NoCopy, Encoding::Ascii).unwrap();
    assert!(r.findings.is_empty(), "no findings; got {:?}", r.findings);
    assert_ne!(r.before, r.after);
    // before decodes CLOSED/100.00, after ACTIVE/150.00
    let bd = decode_record_encoded(CB, &r.before, &NoCopy, Encoding::Ascii).unwrap();
    let ad = decode_record_encoded(CB, &r.after, &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(
        bd.fields
            .iter()
            .find(|f| f.name == "STATUS-CODE")
            .unwrap()
            .value,
        "C"
    );
    assert_eq!(
        ad.fields
            .iter()
            .find(|f| f.name == "STATUS-CODE")
            .unwrap()
            .value,
        "A"
    );
    assert_eq!(
        bd.fields
            .iter()
            .find(|f| f.name == "BALANCE")
            .unwrap()
            .value,
        "100.00"
    );
    assert_eq!(
        ad.fields
            .iter()
            .find(|f| f.name == "BALANCE")
            .unwrap()
            .value,
        "150.00"
    );
    // 88 flips
    assert!(!bd
        .conditions
        .iter()
        .any(|c| c.name == "ACTIVE" && c.value == Some(true)));
    assert!(ad
        .conditions
        .iter()
        .any(|c| c.name == "ACTIVE" && c.value == Some(true)));
    // audit delta + truth layers
    assert!(r
        .casefile_json
        .contains("\"STATUS-CODE\":{\"before\":\"C\",\"after\":\"A\"}"));
    assert!(r
        .casefile_json
        .contains("\"BALANCE\":{\"before\":\"100.00\",\"after\":\"150.00\"}"));
    assert!(r
        .casefile_json
        .contains("\"read_truth\":{\"claimed\":true}"));
    assert!(r.casefile_json.contains(
        "\"transform_truth\":{\"claimed\":true,\"scope\":\"declared sealed transform only\"}"
    ));
    assert!(r
        .casefile_json
        .contains("\"business_truth\":{\"claimed\":false}"));
    assert!(r
        .casefile_json
        .contains("\"write_back_truth\":{\"claimed\":false}"));
    assert!(
        r.applied.iter().any(|a| a.contains("GNURUST.12"))
            && r.applied.iter().any(|a| a.contains("GNURUST.7"))
    );
}

#[test]
fn byte_stable_replay() {
    let input = record(b'C', "100.00");
    let transforms = [Transform {
        op: TransformOp::Add,
        target: "BALANCE",
        amount: Some("1.50"),
    }];
    let a = reconcile_transform(CB, &input, &transforms, &NoCopy, Encoding::Ascii).unwrap();
    let b = reconcile_transform(CB, &input, &transforms, &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(a.after, b.after);
    assert_eq!(a.casefile_json, b.casefile_json);
}

#[test]
fn subtract_works() {
    let input = record(b'A', "100.00");
    let transforms = [Transform {
        op: TransformOp::Subtract,
        target: "BALANCE",
        amount: Some("30.00"),
    }];
    let r = reconcile_transform(CB, &input, &transforms, &NoCopy, Encoding::Ascii).unwrap();
    let ad = decode_record_encoded(CB, &r.after, &NoCopy, Encoding::Ascii).unwrap();
    assert_eq!(
        ad.fields
            .iter()
            .find(|f| f.name == "BALANCE")
            .unwrap()
            .value,
        "70.00"
    );
}

#[test]
fn undeclared_transform_fails_closed() {
    let input = record(b'C', "100.00");
    let transforms = [
        Transform {
            op: TransformOp::SetTrue,
            target: "NOSUCH",
            amount: None,
        },
        Transform {
            op: TransformOp::Add,
            target: "NO-FIELD",
            amount: Some("1.00"),
        },
    ];
    let r = reconcile_transform(CB, &input, &transforms, &NoCopy, Encoding::Ascii).unwrap();
    assert!(r
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-RECON2-UNDECLARED-CONDITION"));
    assert!(r
        .findings
        .iter()
        .any(|(rule, _)| rule == "KOBOLD-RECON2-UNDECLARED-FIELD"));
    assert_eq!(r.before, r.after, "an undeclared transform changes nothing");
}
