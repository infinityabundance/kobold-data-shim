//! KOBOLD.DATA.3 corpus generator: a cp500 (EBCDIC) account family. Text DISPLAY fields are encoded
//! as EBCDIC via the inverse of the sealed `GNURUST.15` cp500 table; packed/binary fields are encoded
//! as their raw storage (ASCII-host) bytes — i.e. they are *not* text-converted, exactly as they will
//! pass through the shim. Writes `recon/account-cp500/input.ebc`. Test infra.

use gnucobol_rs::{
    build_field, cob_move, translate_byte, CodePage, FieldAttr, Usage, COB_FLAG_HAVE_SIGN,
    COB_TYPE_NUMERIC_DISPLAY,
};

struct Lcg(u64);
impl Lcg {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 16) % n.max(1)
    }
}

/// ASCII byte -> EBCDIC byte (inverse of the sealed cp500 decode table; the table is a bijection).
fn ascii_to_ebcdic(c: u8) -> u8 {
    for e in 0u16..256 {
        if translate_byte(CodePage::Cp500, e as u8).unwrap() == c {
            return e as u8;
        }
    }
    0x40 // space
}

/// Encode an alphanumeric value as `width` EBCDIC bytes (space-padded, then translated to cp500).
fn ebcdic_text(value: &str, width: usize) -> Vec<u8> {
    let mut a = value.as_bytes().to_vec();
    a.resize(width, b' ');
    a.truncate(width);
    a.iter().map(|&c| ascii_to_ebcdic(c)).collect()
}

/// Encode a numeric value into raw packed/binary storage via the sealed `cob_move` (NOT EBCDIC).
fn enc_numeric(pic: &str, usage: Usage, signed: bool, value: &str) -> Vec<u8> {
    let pf = build_field(pic, usage, false, false).expect("pic");
    let neg = value.starts_with('-');
    let t = value.trim_start_matches(['-', '+']);
    let (int_part, frac) = t.split_once('.').unwrap_or((t, ""));
    let mut d: Vec<u8> = int_part
        .bytes()
        .chain(frac.bytes())
        .map(|b| b - b'0')
        .collect();
    let have = frac.len() as i16;
    let scale = pf.attr.scale;
    if scale > have {
        d.resize(d.len() + (scale - have) as usize, 0);
    } else if scale < have {
        d.truncate(d.len() - (have - scale) as usize);
    }
    while d.len() < pf.attr.digits as usize {
        d.insert(0, 0);
    }
    let extra = d.len().saturating_sub(pf.attr.digits as usize);
    d.drain(0..extra);
    let mut src: Vec<u8> = d.iter().map(|x| b'0' + x).collect();
    if signed && neg {
        if let Some(l) = src.last_mut() {
            *l |= 0x40;
        }
    }
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

fn main() {
    let mut rng = Lcg(0xC050_0001);
    let statuses = ["A", "C", "D", "A", "A"];
    let tiers = ["G", "S", "S"];
    let mut out = Vec::new();
    for i in 0..120usize {
        // text DISPLAY fields -> EBCDIC
        out.extend_from_slice(&ebcdic_text(&format!("{:06}", 100000 + i), 6)); // ACCOUNT-ID
        out.extend_from_slice(&ebcdic_text(statuses[i % statuses.len()], 1)); // STATUS-CODE
        out.extend_from_slice(&ebcdic_text(&format!("CUST {i:04}"), 12)); // CUSTOMER-NAME
        out.extend_from_slice(&ebcdic_text(tiers[i % tiers.len()], 1)); // CUST-TIER
                                                                        // packed/binary -> raw storage (passthrough; NOT EBCDIC)
        let bal = format!(
            "{}{}.{:02}",
            if rng.below(4) == 0 { "-" } else { "" },
            rng.below(9_999_999),
            rng.below(100)
        );
        out.extend_from_slice(&enc_numeric("S9(7)V99", Usage::Comp3, true, &bal));
        out.extend_from_slice(&enc_numeric(
            "9(4)",
            Usage::Comp,
            false,
            &format!("{}", 1000 + rng.below(8999)),
        ));
        out.extend_from_slice(&enc_numeric(
            "9(6)",
            Usage::CompX,
            false,
            &format!("{}", rng.below(999999)),
        ));
        out.extend_from_slice(&enc_numeric(
            "S9(9)",
            Usage::Comp5,
            true,
            &format!(
                "{}{}",
                if rng.below(3) == 0 { "-" } else { "" },
                rng.below(999999999)
            ),
        ));
    }
    std::fs::write("recon/account-cp500/input.ebc", &out).unwrap();
    eprintln!(
        "wrote {} cp500 records ({} bytes, {} each)",
        120,
        out.len(),
        out.len() / 120
    );
}
