//! KOBOLD.BANK.1 synthetic corpus: a header/detail/trailer banking batch file (fixed 30-byte records,
//! discriminated by byte 0 = H/D/T). The trailer carries DECLARED control totals (count, debit total,
//! credit total); detail records carry a declared DR/CR indicator + a COMP-3 amount. Writes a balanced
//! file and a tampered file (trailer debit off by 1 cent) for the mismatch fixture. Test infra only.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};

const RL: usize = 30;

fn comp3(pic: &str, value: &str) -> Vec<u8> {
    let pf = build_field(pic, Usage::Comp3, false, false).expect("pic");
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = ip.bytes().chain(fp.bytes()).map(|b| b - b'0').collect();
    let have = fp.len() as i16;
    let scale = pf.attr.scale;
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
    cob_move(&src, &sattr, &mut out, &pf.attr).expect("cob_move");
    out
}
fn disp(pic: &str, value: &str) -> Vec<u8> {
    let pf = build_field(pic, Usage::Display, false, false).expect("pic");
    let mut v = value.as_bytes().to_vec();
    v.resize(pf.size, b'0');
    v.truncate(pf.size);
    if pic.contains('X') {
        for b in v.iter_mut() {
            if *b == b'0' {
                *b = b' ';
            }
        }
    }
    v
}
fn pad(mut r: Vec<u8>) -> Vec<u8> {
    r.resize(RL, b' ');
    r
}

fn build(tamper: bool) -> Vec<u8> {
    let mut out = Vec::new();
    // header: 'H' BATCH-ID X(10) BUS-DATE 9(8)
    let mut h = vec![b'H'];
    h.extend(disp("X(10)", "BATCH00001"));
    h.extend(disp("9(8)", "20260608"));
    out.extend(pad(h));
    let details: [(&str, &str); 6] = [
        ("D", "100.00"),
        ("D", "250.50"),
        ("C", "75.25"),
        ("D", "10.00"),
        ("C", "300.00"),
        ("C", "5.55"),
    ];
    let (mut deb, mut cred) = (0i64, 0i64);
    for (i, (ind, amt)) in details.iter().enumerate() {
        let cents = {
            let (ip, fp) = amt.split_once('.').unwrap_or((amt, "0"));
            ip.parse::<i64>().unwrap() * 100 + fp.parse::<i64>().unwrap()
        };
        if *ind == "D" {
            deb += cents;
        } else {
            cred += cents;
        }
        let mut d = vec![b'D'];
        d.extend(disp("9(10)", &format!("{:010}", 1000000 + i)));
        d.push(ind.as_bytes()[0]);
        d.extend(comp3("S9(7)V99", amt));
        d.extend(comp3("S9(1)V9(4)", "1.0500")); // RATE -- numeric but NOT money (declared role: rate)
        out.extend(pad(d));
    }
    // trailer: 'T' TRL-COUNT 9(6) TRL-DEBIT S9(9)V99 COMP-3 TRL-CREDIT S9(9)V99 COMP-3
    let deb_s = if tamper { deb + 1 } else { deb }; // tamper: declared debit off by 1 cent
    let mut t = vec![b'T'];
    t.extend(disp("9(6)", &format!("{:06}", details.len())));
    t.extend(comp3(
        "S9(9)V99",
        &format!("{}.{:02}", deb_s / 100, deb_s % 100),
    ));
    t.extend(comp3(
        "S9(9)V99",
        &format!("{}.{:02}", cred / 100, cred % 100),
    ));
    out.extend(pad(t));
    out
}

fn main() {
    std::fs::write("recon/banking/input.dat", build(false)).unwrap();
    std::fs::write("recon/banking/input-tampered.dat", build(true)).unwrap();
    eprintln!(
        "wrote banking H/D/T files ({} bytes each, {}-byte records)",
        build(false).len(),
        RL
    );
}
