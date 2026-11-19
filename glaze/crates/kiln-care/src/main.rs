//! kiln-care: `check` (report), `fix` (dry-run unless `--fix`), `status` (one line per check).
use clap::{Parser, Subcommand};
use kiln_care::*;
use std::process::{Command, ExitCode};

#[derive(Parser)]
#[command(name = "kiln-care", version, about = "Kiln self-heal and hygiene checks")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Detect problems and print suggested commands. Exit 1 if any finding.
    Check,
    /// Show planned fixes; execute only with --fix. Never removes kernels (list only).
    Fix {
        #[arg(long)]
        fix: bool,
    },
    /// One-line summary per check.
    Status,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).output().ok()?;
    Some(String::from_utf8_lossy(&o.stdout).into_owned())
}

fn busy() -> bool {
    ["apt", "apt-get", "dpkg", "aptitude", "unattended-upgr"].iter()
        .any(|p| Command::new("pgrep").args(["-x", p]).output().map_or(false, |o| o.status.success()))
}

fn collect() -> (Vec<Finding>, Vec<String>, Vec<String>) {
    let mut f = vec![];
    let audit = parse_dpkg_audit(&run("dpkg", &["--audit"]).unwrap_or_default());
    let updates: Vec<String> = std::fs::read_dir("/var/lib/dpkg/updates").map(|d| {
        d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()
    }).unwrap_or_default();
    f.extend(dpkg_interrupted(&audit, &updates));
    let holds = parse_holds(&run("apt-mark", &["showhold"]).unwrap_or_default(), &[]);
    if let Some((a, s)) = run("df", &["-B1", "--output=avail,size", "/"]).and_then(|o| parse_df(&o)) {
        f.extend(disk_finding(a, s));
    }
    let mut conf = std::fs::read_to_string("/etc/systemd/journald.conf").unwrap_or_default();
    if let Ok(d) = std::fs::read_dir("/etc/systemd/journald.conf.d") {
        let mut p: Vec<_> = d.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        p.sort();
        for x in p { conf.push('\n'); conf.push_str(&std::fs::read_to_string(x).unwrap_or_default()); }
    }
    let usage = run("journalctl", &["--disk-usage"]).and_then(|o| parse_journal_usage(&o));
    f.extend(journal_finding(usage, parse_journald_cap(&conf)));
    let pk = run("dpkg-query", &["-W", "-f", "${Package}\n", "linux-image-[0-9]*"]).unwrap_or_default();
    let kernels = old_kernels(&pk, &run("uname", &["-r"]).unwrap_or_default());
    (f, holds, kernels)
}

fn print(f: &[Finding], holds: &[String], kernels: &[String]) {
    for x in f { println!("[{}] {} -> {}", x.id, x.detail, x.suggestion); }
    if !holds.is_empty() { println!("[held] {}", holds.join(", ")); }
    if !kernels.is_empty() { println!("[old-kernels] removable (ask first): {}", kernels.join(", ")); }
}

fn main() -> ExitCode {
    match Cli::parse().cmd {
        Cmd::Check => {
            let (f, h, k) = collect();
            print(&f, &h, &k);
            if f.is_empty() { println!("ok") }
            ExitCode::from(!f.is_empty() as u8)
        }
        Cmd::Status => {
            let (f, h, k) = collect();
            println!("findings={} held={} old_kernels={}", f.len(), h.len(), k.len());
            ExitCode::SUCCESS
        }
        Cmd::Fix { fix } => {
            let (f, _, _) = collect();
            if !fix { println!("dry run (pass --fix to execute):"); }
            if fix && busy() { eprintln!("apt/dpkg running, refusing"); return ExitCode::from(2); }
            for x in &f {
                let steps: Vec<(&str, Vec<&str>)> = match x.id {
                    "dpkg-interrupted" => vec![("dpkg", vec!["--configure", "-a"])],
                    "journal-size" => vec![("journalctl", vec!["--vacuum-size=200M"])],
                    "disk-low" => cleanup_order().iter().filter(|s| !s.list_only)
                        .map(|s| { let mut w = s.cmd.split(' '); (w.next().unwrap(), w.collect()) }).collect(),
                    _ => vec![],
                };
                for (c, a) in steps {
                    println!("{} {} {}", if fix { "run:" } else { "would run:" }, c, a.join(" "));
                    if fix && !Command::new(c).args(&a).status().map_or(false, |s| s.success()) {
                        eprintln!("failed: {c}"); return ExitCode::FAILURE;
                    }
                }
            }
            ExitCode::SUCCESS
        }
    }
}
