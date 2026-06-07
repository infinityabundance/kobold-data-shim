//! `kobold-recon` — end-to-end fixed-record reconciliation (`KOBOLD.RECON.1`). Decodes a record
//! file against a copybook into byte-stable JSONL + audit.json + unsupported.json, using only the
//! sealed gnucobol-rs courts. The CLI is a thin wrapper over `kobold_data_shim::recon::reconcile`,
//! so the CLI and library paths produce identical bytes.
//!
//! Usage:
//!   kobold-recon --fixture NAME --copybook F.cpy --data F.dat --record-len N \
//!                --gnucobol-rs-version X.Y.Z [--copydir DIR] --out DIR

use std::process::ExitCode;

struct DirResolver(Option<String>);
impl kobold_data_shim::CopyResolver for DirResolver {
    fn resolve(&self, name: &str) -> Option<String> {
        let dir = self.0.as_ref()?;
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

fn main() -> ExitCode {
    let mut fixture = None;
    let mut copybook = None;
    let mut data = None;
    let mut record_len = None;
    let mut copydir = None;
    let mut out = None;
    let mut gver = String::from("0.3.2");
    let mut encoding = kobold_data_shim::Encoding::Ascii;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fixture" => fixture = it.next().cloned(),
            "--copybook" => copybook = it.next().cloned(),
            "--data" => data = it.next().cloned(),
            "--record-len" => record_len = it.next().and_then(|s| s.parse::<usize>().ok()),
            "--copydir" => copydir = it.next().cloned(),
            "--out" => out = it.next().cloned(),
            "--gnucobol-rs-version" => {
                if let Some(v) = it.next() {
                    gver = v.clone();
                }
            }
            // Explicit, never auto-detected. Only the oracle-admitted code page (cp500) is accepted.
            "--encoding" => match it.next().map(|s| s.as_str()) {
                Some("ascii") => encoding = kobold_data_shim::Encoding::Ascii,
                Some("cp500") => encoding = kobold_data_shim::Encoding::Cp500,
                other => {
                    eprintln!(
                        "unsupported --encoding {:?} (admitted: ascii, cp500)",
                        other.unwrap_or("")
                    );
                    return ExitCode::from(2);
                }
            },
            _ => {
                eprintln!("unknown arg: {a}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(fixture), Some(cb), Some(df), Some(rlen), Some(outdir)) =
        (fixture, copybook, data, record_len, out)
    else {
        eprintln!("usage: kobold-recon --fixture N --copybook F.cpy --data F.dat --record-len N --out DIR [--copydir DIR]");
        return ExitCode::from(2);
    };

    let copybook = match std::fs::read_to_string(&cb) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("copybook: {e}");
            return ExitCode::from(1);
        }
    };
    let data = match std::fs::read(&df) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("data: {e}");
            return ExitCode::from(1);
        }
    };

    let resolver = DirResolver(copydir);
    let r = match kobold_data_shim::recon::reconcile_encoded(
        &fixture, &copybook, &data, rlen, &gver, &resolver, encoding,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("reconcile: {e}");
            return ExitCode::from(1);
        }
    };

    if std::fs::create_dir_all(&outdir).is_err() {
        eprintln!("could not create out dir");
        return ExitCode::from(1);
    }
    let write = |name: &str, content: &str| std::fs::write(format!("{outdir}/{name}"), content);
    if write("expected.jsonl", &r.jsonl).is_err()
        || write("audit.json", &r.audit_json).is_err()
        || write("unsupported.json", &r.unsupported_json).is_err()
    {
        eprintln!("could not write outputs");
        return ExitCode::from(1);
    }
    eprintln!(
        "reconciled {} records ({} unsupported) -> {outdir}/",
        r.record_count, r.unsupported_count
    );
    ExitCode::SUCCESS
}
