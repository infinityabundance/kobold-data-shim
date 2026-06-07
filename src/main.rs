//! `kobold-record-dump` — decode a raw COBOL record dump against a copybook, byte-exactly, via the
//! oracle-proven gnucobol-rs courts. Outputs a per-field table or JSON, including the raw bytes
//! (audit trail) and any `unsupported`/short fields (the reconciliation signal).
//!
//! Usage:
//!   kobold-record-dump --copybook CUST.cpy --record dump.bin [--copydir DIR] [--json]

use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage: kobold-record-dump --copybook <file> --record <file> [--copydir <dir>] [--json]"
    );
    ExitCode::from(2)
}

struct DirResolver {
    dir: Option<String>,
}
impl gnucobol_rs::copybook::CopyResolver for DirResolver {
    fn resolve(&self, name: &str) -> Option<String> {
        let dir = self.dir.as_ref()?;
        for base in [name.to_string(), name.to_ascii_lowercase()] {
            for ext in ["", ".cpy", ".CPY", ".cbl", ".cob"] {
                if let Ok(s) = std::fs::read_to_string(format!("{dir}/{base}{ext}")) {
                    return Some(s);
                }
            }
        }
        None
    }
}

fn json_escape(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut copybook = None;
    let mut record = None;
    let mut copydir = None;
    let mut json = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--copybook" => copybook = it.next().cloned(),
            "--record" => record = it.next().cloned(),
            "--copydir" => copydir = it.next().cloned(),
            "--json" => json = true,
            _ => return usage(),
        }
    }
    let (Some(cb_path), Some(rec_path)) = (copybook, record) else {
        return usage();
    };
    let cb = match std::fs::read_to_string(&cb_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading copybook: {e}");
            return ExitCode::from(1);
        }
    };
    let rec = match std::fs::read(&rec_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error reading record: {e}");
            return ExitCode::from(1);
        }
    };

    let resolver = DirResolver { dir: copydir };
    let fields = match kobold_data_shim::decode_with_resolver(&cb, &rec, &resolver) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("decode error: {e}");
            return ExitCode::from(1);
        }
    };

    if json {
        println!("{{\"oracle\":\"GnuCOBOL 3.2.0\",\"shim\":\"kobold-data-shim\",\"fields\":[");
        for (i, f) in fields.iter().enumerate() {
            let comma = if i + 1 < fields.len() { "," } else { "" };
            println!(
                "  {{\"name\":{},\"level\":{},\"offset\":{},\"size\":{},\"category\":{},\"value\":{},\"raw_hex\":{}}}{comma}",
                json_escape(&f.name),
                f.level,
                f.offset,
                f.size,
                json_escape(f.category),
                json_escape(&f.value),
                json_escape(&f.raw_hex),
            );
        }
        println!("]}}");
    } else {
        println!(
            "{:<24} {:>5} {:>5} {:<13} {:<24} RAW",
            "FIELD", "OFF", "SIZE", "CATEGORY", "VALUE"
        );
        for f in &fields {
            let indent = "  ".repeat((f.level as usize / 5).min(6));
            println!(
                "{indent}{:<24} {:>5} {:>5} {:<13} {:<24} {}",
                f.name, f.offset, f.size, f.category, f.value, f.raw_hex
            );
        }
        let unsupported = fields
            .iter()
            .filter(|f| f.category == "unsupported")
            .count();
        if unsupported > 0 {
            eprintln!(
                "note: {unsupported} field(s) unsupported — surface these for reconciliation"
            );
        }
    }
    ExitCode::SUCCESS
}
