//! Deterministic corpus data generator for KOBOLD.RECON.1. Encodes records with the sealed
//! `value_image` so the bytes match exactly what the copybooks decode. Writes
//! `recon/<family>/input.dat`. Test infrastructure, not shipped.

use gnucobol_rs::{value_image, Usage, Val, ValueItem};

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

fn grp(name: &str) -> ValueItem {
    ValueItem {
        level: 1,
        name: name.into(),
        pic: None,
        value: None,
    }
}
fn fld(name: &str, pic: &str, usage: Usage, v: Val) -> ValueItem {
    ValueItem {
        level: 5,
        name: name.into(),
        pic: Some((pic.into(), usage, false, false)),
        value: Some(v),
    }
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
        let rec = vec![
            grp("ACCOUNT-RECORD"),
            fld(
                "ACCOUNT-ID",
                "9(6)",
                Usage::Display,
                Val::Num(format!("{:06}", 100000 + i)),
            ),
            fld(
                "STATUS-CODE",
                "X",
                Usage::Display,
                Val::Alpha(statuses[i % statuses.len()].into()),
            ),
            fld("BALANCE", "S9(7)V99", Usage::Comp3, Val::Num(bal)),
            fld(
                "CUST-NAME",
                "X(20)",
                Usage::Display,
                Val::Alpha(format!("CUSTOMER {i:04}")),
            ),
            fld(
                "CUST-TIER",
                "X",
                Usage::Display,
                Val::Alpha(tiers[i % tiers.len()].into()),
            ),
        ];
        out.extend_from_slice(&value_image(&rec).expect("account encode"));
    }
    out
}

fn payroll(n: usize) -> Vec<u8> {
    let mut rng = Lcg(0x9A70_0001);
    let mut out = Vec::new();
    let depts = ["ENG", "SALE", "OPS", "HR"];
    let types = ["S", "H", "S", "H", "S"];
    for i in 0..n {
        let gross = format!("{}.{:02}", 2000 + rng.below(80000), rng.below(100));
        let ded = format!("{}.{:02}", rng.below(9000), rng.below(100));
        let rec = vec![
            grp("PAYROLL-RECORD"),
            fld(
                "EMP-ID",
                "9(5)",
                Usage::Display,
                Val::Num(format!("{:05}", 10000 + i)),
            ),
            fld(
                "DEPT",
                "X(4)",
                Usage::Display,
                Val::Alpha(depts[i % depts.len()].into()),
            ),
            fld(
                "PAY-TYPE",
                "X",
                Usage::Display,
                Val::Alpha(types[i % types.len()].into()),
            ),
            fld("GROSS-PAY", "S9(7)V99", Usage::Comp3, Val::Num(gross)),
            fld("DEDUCTIONS", "S9(5)V99", Usage::Comp3, Val::Num(ded)),
        ];
        out.extend_from_slice(&value_image(&rec).expect("payroll encode"));
    }
    out
}

fn insurance(n: usize) -> Vec<u8> {
    let mut rng = Lcg(0x1A50_0001);
    let mut out = Vec::new();
    for i in 0..n {
        let risk = (rng.below(9) + 1).to_string(); // 1..9
        let prem = format!("{}.{:02}", rng.below(900000), rng.below(100));
        let rec = vec![
            grp("POLICY-RECORD"),
            fld(
                "POLICY-NO",
                "X(10)",
                Usage::Display,
                Val::Alpha(format!("POL{:07}", i)),
            ),
            fld("RISK-CLASS", "9", Usage::Display, Val::Num(risk)),
            fld("PREMIUM", "S9(6)V99", Usage::Comp3, Val::Num(prem)),
            fld(
                "TERM-MONTHS",
                "9(3)",
                Usage::Display,
                Val::Num(format!("{:03}", 12 + rng.below(48))),
            ),
        ];
        out.extend_from_slice(&value_image(&rec).expect("insurance encode"));
    }
    out
}

fn main() {
    std::fs::write("recon/account/input.dat", account(120)).unwrap();
    std::fs::write("recon/payroll/input.dat", payroll(120)).unwrap();
    std::fs::write("recon/insurance/input.dat", insurance(120)).unwrap();
    eprintln!("wrote 3 x 120 = 360 records");
}
