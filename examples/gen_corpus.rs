//! Deterministic corpus data generator for KOBOLD.DATA.2. Encodes each field via the sealed
//! gnucobol-rs courts (DISPLAY/COMP-3 zoned + COMP/COMP-5/COMP-X binary through `cob_move`), so the
//! bytes match exactly what the copybooks decode. Writes `recon/<family>/input.dat`. Test infra.

use gnucobol_rs::{
    build_field, cob_move, FieldAttr, Usage, COB_FLAG_HAVE_SIGN, COB_TYPE_NUMERIC_DISPLAY,
};

struct Lcg(u64);
impl Lcg {
    fn step(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 16
    }
    fn below(&mut self, n: u64) -> u64 {
        self.step() % n.max(1)
    }
}

/// Render a numeric literal as a zoned DISPLAY temp of `digits` at `scale`, signed if `signed`.
fn zoned(value: &str, digits: u16, scale: i16, signed: bool) -> Vec<u8> {
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (int_part, frac) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = int_part
        .bytes()
        .chain(frac.bytes())
        .map(|b| b - b'0')
        .collect();
    let have = frac.len() as i16;
    if scale > have {
        d.resize(d.len() + (scale - have) as usize, 0);
    } else if scale < have {
        d.truncate(d.len() - (have - scale) as usize);
    }
    while d.len() < digits as usize {
        d.insert(0, 0);
    }
    if d.len() > digits as usize {
        let extra = d.len() - digits as usize;
        d.drain(0..extra);
    }
    let mut out: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    if signed && neg {
        if let Some(l) = out.last_mut() {
            *l |= 0x40;
        }
    }
    out
}

/// Encode one field's value to its storage bytes via the sealed field model + `cob_move`.
fn enc(pic: &str, usage: Usage, signed: bool, value: &str) -> Vec<u8> {
    let pf = build_field(pic, usage, false, false).expect("pic");
    if pf.attr.field_type == gnucobol_rs::pic::COB_TYPE_ALPHANUMERIC {
        let mut v = value.as_bytes().to_vec();
        v.resize(pf.size, b' ');
        v.truncate(pf.size);
        return v;
    }
    let src = zoned(value, pf.attr.digits, pf.attr.scale, signed);
    let sattr = FieldAttr {
        field_type: COB_TYPE_NUMERIC_DISPLAY,
        digits: pf.attr.digits,
        scale: pf.attr.scale,
        flags: if signed { COB_FLAG_HAVE_SIGN } else { 0 },
    };
    let mut out = vec![0u8; pf.size];
    cob_move(&src, &sattr, &mut out, &pf.attr).expect("cob_move");
    out
}

/// COBOL numeric→edited editing for the admitted decode subset (test infra only — NOT a sealed court;
/// the *decode* of these bytes is GNURUST.16, proven against `cobc` by `edited_sweep.sh`, and the
/// exact pics used here are in that sweep). Handles `Z 9 , .`, a leading `+`/`-`, and trailing `CR`/`DB`.
fn edit(pic: &str, value: &str) -> Vec<u8> {
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (ip, fp) = t.split_once('.').unwrap_or((t, ""));
    let chars: Vec<char> = pic.chars().collect();
    // trailing CR / DB
    let mut crdb: Option<&str> = None;
    let mut body: &[char] = &chars;
    if chars.len() >= 2 {
        let last2: String = chars[chars.len() - 2..].iter().collect();
        if last2 == "CR" {
            crdb = Some("CR");
            body = &chars[..chars.len() - 2];
        } else if last2 == "DB" {
            crdb = Some("DB");
            body = &chars[..chars.len() - 2];
        }
    }
    // leading sign
    let mut lead: Option<char> = None;
    let mut start = 0usize;
    if matches!(body.first(), Some('+') | Some('-')) {
        lead = body.first().copied();
        start = 1;
    }
    let int_pos = body[start..]
        .iter()
        .take_while(|&&c| c != '.')
        .filter(|&&c| c == '9' || c == 'Z')
        .count();
    let frac_pos = match body.iter().position(|&c| c == '.') {
        Some(d) => body[d + 1..]
            .iter()
            .filter(|&&c| c == '9' || c == 'Z')
            .count(),
        None => 0,
    };
    // align value digits to the picture's digit positions
    let mut intd: Vec<u8> = ip.bytes().map(|b| b - b'0').collect();
    while intd.len() < int_pos {
        intd.insert(0, 0);
    }
    let extra = intd.len().saturating_sub(int_pos);
    intd.drain(0..extra);
    let mut fracd: Vec<u8> = fp.bytes().map(|b| b - b'0').collect();
    fracd.resize(frac_pos, 0);
    fracd.truncate(frac_pos);

    let mut out = Vec::new();
    if let Some(s) = lead {
        out.push(if s == '-' {
            if neg {
                b'-'
            } else {
                b' '
            }
        } else if neg {
            b'-'
        } else {
            b'+'
        });
    }
    let (mut ii, mut fi, mut suppress, mut after_dot) = (0usize, 0usize, true, false);
    for &c in &body[start..] {
        match c {
            '9' if after_dot => {
                out.push(b'0' + fracd[fi]);
                fi += 1;
            }
            '9' => {
                out.push(b'0' + intd[ii]);
                ii += 1;
                suppress = false;
            }
            'Z' => {
                let d = intd[ii];
                ii += 1;
                if suppress && d == 0 {
                    out.push(b' ');
                } else {
                    out.push(b'0' + d);
                    suppress = false;
                }
            }
            ',' => out.push(if suppress { b' ' } else { b',' }),
            '.' => {
                out.push(b'.');
                after_dot = true;
                suppress = false;
            }
            _ => {}
        }
    }
    if let Some(s) = crdb {
        out.extend_from_slice(if neg { s.as_bytes() } else { b"  " });
    }
    out
}

/// `(pic, usage, signed, value)`. An edited picture (one `pic::build_field` rejects) routes to `edit`.
type Field<'a> = (&'a str, Usage, bool, String);

fn is_edited(pic: &str) -> bool {
    build_field(pic, Usage::Display, false, false).is_err()
}

fn record(fields: &[Field]) -> Vec<u8> {
    let mut out = Vec::new();
    for (pic, usage, signed, value) in fields {
        if is_edited(pic) {
            out.extend_from_slice(&edit(pic, value));
        } else {
            out.extend_from_slice(&enc(pic, *usage, *signed, value));
        }
    }
    out
}

fn account(n: usize) -> Vec<u8> {
    let mut rng = Lcg(0xACC0_0001);
    let mut out = Vec::new();
    let statuses = ["A", "C", "D", "A", "A"];
    let tiers = ["G", "S", "S"];
    for i in 0..n {
        let bal = format!(
            "{}{}.{:02}",
            if rng.below(4) == 0 { "-" } else { "" },
            rng.below(9_999_999),
            rng.below(100)
        );
        out.extend_from_slice(&record(&[
            ("9(6)", Usage::Display, false, format!("{:06}", 100000 + i)),
            (
                "X",
                Usage::Display,
                false,
                statuses[i % statuses.len()].into(),
            ),
            ("S9(7)V99", Usage::Comp3, true, bal),
            ("X(20)", Usage::Display, false, format!("CUSTOMER {i:04}")),
            ("X", Usage::Display, false, tiers[i % tiers.len()].into()),
            (
                "9(4)",
                Usage::Comp,
                false,
                format!("{}", 1000 + rng.below(8999)),
            ),
            (
                "9(6)",
                Usage::CompX,
                false,
                format!("{}", rng.below(999999)),
            ),
            (
                "S9(9)",
                Usage::Comp5,
                true,
                format!(
                    "{}{}",
                    if rng.below(3) == 0 { "-" } else { "" },
                    rng.below(999999999)
                ),
            ),
            (
                "ZZ,ZZ9.99",
                Usage::Display,
                false,
                format!("{}.{:02}", rng.below(99999), rng.below(100)),
            ),
        ]));
    }
    out
}

fn payroll(n: usize) -> Vec<u8> {
    let mut rng = Lcg(0x9A70_0001);
    let mut out = Vec::new();
    let depts = ["ENG", "SALE", "OPS", "HR"];
    let types = ["S", "H", "S", "H", "S"];
    for i in 0..n {
        out.extend_from_slice(&record(&[
            ("9(5)", Usage::Display, false, format!("{:05}", 10000 + i)),
            ("X(4)", Usage::Display, false, depts[i % depts.len()].into()),
            ("X", Usage::Display, false, types[i % types.len()].into()),
            (
                "S9(7)V99",
                Usage::Comp3,
                true,
                format!("{}.{:02}", 2000 + rng.below(80000), rng.below(100)),
            ),
            (
                "S9(5)V99",
                Usage::Comp3,
                true,
                format!("{}.{:02}", rng.below(9000), rng.below(100)),
            ),
            ("9(6)", Usage::Comp, false, format!("{}", 100000 + i)),
            (
                "S9(4)",
                Usage::Comp5,
                true,
                format!(
                    "{}{}",
                    if rng.below(4) == 0 { "-" } else { "" },
                    rng.below(2000)
                ),
            ),
            (
                "ZZ9.99CR",
                Usage::Display,
                true,
                format!(
                    "{}{}.{:02}",
                    if rng.below(3) == 0 { "-" } else { "" },
                    rng.below(999),
                    rng.below(100)
                ),
            ),
        ]));
    }
    out
}

fn insurance(n: usize) -> Vec<u8> {
    let mut rng = Lcg(0x1A50_0001);
    let mut out = Vec::new();
    for i in 0..n {
        out.extend_from_slice(&record(&[
            ("X(10)", Usage::Display, false, format!("POL{:07}", i)),
            ("9", Usage::Display, false, (rng.below(9) + 1).to_string()),
            (
                "S9(6)V99",
                Usage::Comp3,
                true,
                format!("{}.{:02}", rng.below(900000), rng.below(100)),
            ),
            (
                "9(3)",
                Usage::Display,
                false,
                format!("{:03}", 12 + rng.below(48)),
            ),
            ("9(10)", Usage::CompX, false, format!("{}", 1_000_000 + i)),
            ("9(4)", Usage::Comp, false, format!("{}", rng.below(50))),
            (
                "ZZZ,ZZ9.99",
                Usage::Display,
                false,
                format!("{}.{:02}", rng.below(999999), rng.below(100)),
            ),
        ]));
    }
    out
}

fn main() {
    std::fs::write("recon/account/input.dat", account(120)).unwrap();
    std::fs::write("recon/payroll/input.dat", payroll(120)).unwrap();
    std::fs::write("recon/insurance/input.dat", insurance(120)).unwrap();
    eprintln!("wrote 3 x 120 = 360 records (with COMP/COMP-5/COMP-X binary fields)");
}
