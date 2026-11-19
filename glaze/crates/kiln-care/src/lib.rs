//! Pure parsing and planning logic for kiln-care. No I/O here, so it is testable with fixture strings.

/// One detected problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: &'static str,
    pub detail: String,
    /// Suggested command (shown by `check`, executed by `fix --fix` only if `auto`).
    pub suggestion: String,
    pub auto: bool,
}

pub const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const TARGET_FREE_BYTES: u64 = 3 * 1024 * 1024 * 1024;
pub const JOURNAL_CAP_BYTES: u64 = 500 * 1024 * 1024;
pub const JOURNAL_VACUUM: &str = "200M";

/// `dpkg --audit` prints nothing when healthy; any non-empty output means broken packages.
pub fn parse_dpkg_audit(out: &str) -> Vec<String> {
    out.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// Names of files in /var/lib/dpkg/updates/ (non-empty dir = interrupted dpkg run).
pub fn dpkg_interrupted(audit: &[String], updates_entries: &[String]) -> Option<Finding> {
    if audit.is_empty() && updates_entries.is_empty() {
        return None;
    }
    let detail = format!("{} audit line(s), {} pending update file(s)", audit.len(), updates_entries.len());
    Some(Finding { id: "dpkg-interrupted", detail, suggestion: "dpkg --configure -a".into(), auto: true })
}

/// `apt-mark showhold`: one package per line. `known` = holds placed by Kiln (F2), excluded.
pub fn parse_holds(out: &str, known: &[&str]) -> Vec<String> {
    out.lines().map(str::trim).filter(|l| !l.is_empty() && !known.contains(l)).map(String::from).collect()
}

/// Parse sizes like `200M`, `1G`, `512K`, `4096` (bytes), `1.5G`. Journald/systemd style, base 1024.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, mul) = match s.chars().last()? {
        'K' | 'k' => (&s[..s.len() - 1], 1u64 << 10),
        'M' | 'm' => (&s[..s.len() - 1], 1 << 20),
        'G' | 'g' => (&s[..s.len() - 1], 1 << 30),
        'T' | 't' => (&s[..s.len() - 1], 1 << 40),
        'B' => (&s[..s.len() - 1], 1),
        c if c.is_ascii_digit() => (s, 1),
        _ => return None,
    };
    let v: f64 = num.trim().parse().ok()?;
    if v < 0.0 { return None; }
    Some((v * mul as f64) as u64)
}

/// `journalctl --disk-usage`: "Archived and active journals take up 612.0M in the file system."
pub fn parse_journal_usage(out: &str) -> Option<u64> {
    let rest = out.split("take up ").nth(1)?;
    parse_size(rest.split_whitespace().next()?)
}

/// Last `SystemMaxUse=` value in journald.conf text (later files/lines override earlier ones).
pub fn parse_journald_cap(conf: &str) -> Option<u64> {
    conf.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && !l.starts_with(';'))
        .filter_map(|l| l.strip_prefix("SystemMaxUse="))
        .filter_map(parse_size)
        .last()
}

pub fn journal_finding(usage: Option<u64>, cap: Option<u64>) -> Option<Finding> {
    let over = usage.map_or(false, |u| u > JOURNAL_CAP_BYTES);
    let uncapped = cap.map_or(true, |c| c > JOURNAL_CAP_BYTES);
    if !over && !uncapped {
        return None;
    }
    let detail = format!("usage {} MiB, SystemMaxUse {}", usage.unwrap_or(0) >> 20,
        cap.map_or("unset".to_string(), |c| format!("{} MiB", c >> 20)));
    Some(Finding { id: "journal-size", detail,
        suggestion: format!("journalctl --vacuum-size={JOURNAL_VACUUM}; set SystemMaxUse={JOURNAL_VACUUM}"), auto: true })
}

/// `df -B1 --output=avail,size /` (header + one row) -> (avail, size) bytes.
pub fn parse_df(out: &str) -> Option<(u64, u64)> {
    let row = out.lines().filter(|l| !l.trim().is_empty()).nth(1)?;
    let mut it = row.split_whitespace();
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

/// Disk-full threshold from the doc: root < 2 GB or < 5 %.
pub fn disk_low(avail: u64, size: u64) -> bool {
    avail < MIN_FREE_BYTES || (size > 0 && avail * 100 / size < 5)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupStep {
    pub name: &'static str,
    pub cmd: &'static str,
    /// Only listed, never executed automatically (needs user consent).
    pub list_only: bool,
}

/// Safe-to-risky order from the doc, subset implemented in v0.
pub fn cleanup_order() -> Vec<CleanupStep> {
    vec![
        CleanupStep { name: "apt clean", cmd: "apt-get clean", list_only: false },
        CleanupStep { name: "old journals", cmd: "journalctl --vacuum-size=200M", list_only: false },
        CleanupStep { name: "old kernels", cmd: "apt-get autoremove --purge", list_only: true },
    ]
}

pub fn disk_finding(avail: u64, size: u64) -> Option<Finding> {
    if !disk_low(avail, size) {
        return None;
    }
    let order: Vec<_> = cleanup_order().iter().map(|s| s.name).collect();
    Some(Finding { id: "disk-low", detail: format!("{} MiB free of {} MiB", avail >> 20, size >> 20),
        suggestion: format!("cleanup order (stop at 3 GB free): {}", order.join(", ")), auto: false })
}

/// Compare dotted/dashed version strings numerically segment by segment ("6.12.9-amd64" < "6.12.10-amd64").
pub fn ver_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect()
    };
    key(a).cmp(&key(b))
}

/// Installed `linux-image-<ver>` package names (one per line, from dpkg-query) + running `uname -r`.
/// Returns removable old kernels: everything except the running and the newest, only when more than
/// two kernels exist besides the running one.
pub fn old_kernels(pkgs: &str, running: &str) -> Vec<String> {
    let mut vers: Vec<String> = pkgs.lines().map(str::trim)
        .filter_map(|l| l.strip_prefix("linux-image-"))
        .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()))
        .map(String::from).collect();
    vers.sort_by(|a, b| ver_cmp(a, b));
    let others: Vec<String> = vers.into_iter().filter(|v| v != running.trim()).collect();
    if others.len() <= 2 {
        return vec![];
    }
    others[..others.len() - 1].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUDIT_BAD: &str = "The following packages are only half configured, probably due to problems\nconfiguring them the first time.\n libc6\n";

    #[test]
    fn audit_clean_and_bad() {
        assert!(parse_dpkg_audit("\n").is_empty());
        assert_eq!(parse_dpkg_audit(AUDIT_BAD).len(), 3);
        assert!(dpkg_interrupted(&[], &[]).is_none());
        let f = dpkg_interrupted(&[], &["0001".into()]).unwrap();
        assert_eq!(f.suggestion, "dpkg --configure -a");
    }

    #[test]
    fn holds_minus_known() {
        let out = "linux-image-amd64\nnvidia-driver\n\nfoo\n";
        assert_eq!(parse_holds(out, &["nvidia-driver"]), vec!["linux-image-amd64", "foo"]);
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("200M"), Some(200 << 20));
        assert_eq!(parse_size("1.5G"), Some((1.5 * (1u64 << 30) as f64) as u64));
        assert_eq!(parse_size("4096"), Some(4096));
        assert_eq!(parse_size("x"), None);
        assert_eq!(parse_size(""), None);
    }

    #[test]
    fn journal() {
        let out = "Archived and active journals take up 612.0M in the file system.\n";
        assert_eq!(parse_journal_usage(out), Some((612.0 * (1u64 << 20) as f64) as u64));
        assert_eq!(parse_journal_usage("garbage"), None);
        let conf = "[Journal]\n#SystemMaxUse=\nSystemMaxUse=1G\nSystemMaxUse=200M\n";
        assert_eq!(parse_journald_cap(conf), Some(200 << 20));
        assert_eq!(parse_journald_cap("[Journal]\n#SystemMaxUse=50M\n"), None);
        assert!(journal_finding(Some(100 << 20), Some(200 << 20)).is_none());
        assert!(journal_finding(Some(100 << 20), None).is_some()); // uncapped
        assert!(journal_finding(Some(612 << 20), Some(200 << 20)).is_some()); // over
    }

    #[test]
    fn df_and_disk() {
        let out = "      Avail        1B-blocks\n1073741824 53687091200\n";
        let (a, s) = parse_df(out).unwrap();
        assert_eq!((a, s), (1 << 30, 50 << 30));
        assert!(disk_low(a, s));
        assert!(!disk_low(20 << 30, 50 << 30));
        assert!(disk_low(2_900_000_000, 100 << 30)); // < 5 %? 2.7 GiB of 100 GiB
        assert!(parse_df("Avail 1B-blocks\n").is_none());
        let f = disk_finding(a, s).unwrap();
        assert!(f.suggestion.starts_with("cleanup order") && f.suggestion.contains("apt clean, old journals"));
        assert!(!f.auto);
    }

    #[test]
    fn cleanup_order_safe_first() {
        let o = cleanup_order();
        assert_eq!(o[0].name, "apt clean");
        assert!(o.last().unwrap().list_only);
    }

    #[test]
    fn kernels() {
        let pkgs = "linux-image-6.12.9-amd64\nlinux-image-6.12.10-amd64\nlinux-image-6.12.2-amd64\nlinux-image-6.12.11-amd64\nlinux-image-amd64\n";
        // running 6.12.10; others = 2, 9, 11 -> 3 > 2 -> remove all but newest (11): 2 and 9
        assert_eq!(old_kernels(pkgs, "6.12.10-amd64\n"), vec!["6.12.2-amd64", "6.12.9-amd64"]);
        let few = "linux-image-6.12.9-amd64\nlinux-image-6.12.10-amd64\n";
        assert!(old_kernels(few, "6.12.10-amd64").is_empty());
    }
}
