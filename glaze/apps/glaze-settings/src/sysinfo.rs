//! System information and system-tool adapters (Displays, Sound, Network,
//! Power, About). Everything degrades gracefully: a missing tool yields
//! `None`/empty and the UI shows an explanatory empty state. Parsers are pure
//! functions with unit tests; the thin `probe_*` functions run the tools.
//!
//! No root is needed for anything here: `swaymsg`/`wlr-randr` talk to the
//! user's compositor, `wpctl`/`pactl` to the user's sound server, `nmcli` to
//! NetworkManager (radio switching is allowed to the active local session by
//! its default polkit policy), and power data is read from sysfs.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// Run a program with a timeout; `None` when it is missing, fails or hangs.
pub fn run(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = child.wait_with_output().ok()?;
                return status
                    .success()
                    .then(|| String::from_utf8_lossy(&out.stdout).to_string());
            }
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Run a program and report its exit status and stderr (for actions).
pub fn act(program: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if err.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            err
        };
        Err(format!("{program} failed: {msg}"))
    }
}

/// True when `program` is on `PATH`.
#[cfg(test)]
pub fn has(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
        .unwrap_or(false)
}

const T: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------- about ----

#[derive(Debug, Clone, Default, PartialEq)]
pub struct About {
    pub os_name: String,
    pub os_version: String,
    pub kernel: String,
    pub arch: String,
    pub cpu: String,
    pub cores: usize,
    pub memory_kib: u64,
    pub hostname: String,
}

/// `PRETTY_NAME` (or `NAME VERSION`) and `VERSION_ID` from os-release.
pub fn parse_os_release(text: &str) -> (String, String) {
    let mut pretty = None;
    let mut name = None;
    let mut version = None;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "PRETTY_NAME" => pretty = Some(v),
            "NAME" => name = Some(v),
            "VERSION_ID" => version = Some(v),
            _ => {}
        }
    }
    (
        pretty.or(name).unwrap_or_else(|| "Unknown".into()),
        version.unwrap_or_default(),
    )
}

pub fn parse_meminfo_kib(text: &str) -> u64 {
    text.lines()
        .find(|l| l.starts_with("MemTotal:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// CPU model from `lscpu` output ("Model name:"), with the vendor when the
/// model is only a code name (arm64).
pub fn parse_lscpu(text: &str) -> (String, usize) {
    let mut model = String::new();
    let mut vendor = String::new();
    let mut cpus = 0;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "Model name" if model.is_empty() && v != "-" => model = v.to_string(),
            "Vendor ID" => vendor = v.to_string(),
            "CPU(s)" => cpus = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    let name = match (vendor.is_empty(), model.is_empty()) {
        (_, true) => vendor,
        (true, false) => model,
        (false, false) if model.contains(&vendor) => model,
        (false, false) => format!("{vendor} {model}"),
    };
    (name, cpus)
}

pub fn human_kib(kib: u64) -> String {
    let gib = kib as f64 / 1024.0 / 1024.0;
    if gib >= 1.0 {
        format!("{gib:.1} GiB")
    } else {
        format!("{} MiB", kib / 1024)
    }
}

pub fn probe_about() -> About {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    let (os_name, os_version) = parse_os_release(&read("/etc/os-release"));
    let (cpu, cores) = run("lscpu", &[], T)
        .map(|t| parse_lscpu(&t))
        .unwrap_or_default();
    About {
        os_name,
        os_version,
        kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu,
        cores: if cores > 0 {
            cores
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        },
        memory_kib: parse_meminfo_kib(&read("/proc/meminfo")),
        hostname: read("/proc/sys/kernel/hostname").trim().to_string(),
    }
}

/// The session's compositor, from the environment and `swaymsg`.
pub fn compositor_name() -> String {
    if std::env::var_os("SWAYSOCK").is_some() {
        if let Some(v) = run("swaymsg", &["-t", "get_version", "-r"], T) {
            if let Ok(j) = serde_json::from_str::<serde_json::Value>(&v) {
                if let Some(s) = j.get("human_readable").and_then(|x| x.as_str()) {
                    return format!("Sway {s}");
                }
            }
        }
        return "Sway".into();
    }
    if std::env::var_os("GLAZE_COMPOSITOR_SOCKET").is_some()
        || glaze_common::paths::runtime_dir()
            .join("glaze/compositor.sock")
            .exists()
    {
        return "glaze-compositor".into();
    }
    match std::env::var("XDG_CURRENT_DESKTOP") {
        Ok(s) if !s.is_empty() => s,
        _ => "Unknown Wayland compositor".into(),
    }
}

// ------------------------------------------------------------- displays ----

#[derive(Debug, Clone, PartialEq)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    /// mHz
    pub refresh: i32,
}

impl Mode {
    pub fn label(&self) -> String {
        if self.refresh > 0 {
            format!(
                "{} × {} @ {:.0} Hz",
                self.width,
                self.height,
                self.refresh as f64 / 1000.0
            )
        } else {
            format!("{} × {}", self.width, self.height)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub name: String,
    pub description: String,
    pub active: bool,
    pub current: Option<Mode>,
    pub modes: Vec<Mode>,
    pub scale: f64,
    /// `normal`, `90`, `180`, `270`, `flipped`, ...
    pub transform: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Deserialize)]
struct SwayMode {
    width: i32,
    height: i32,
    #[serde(default)]
    refresh: i32,
}

#[derive(Deserialize)]
struct SwayRect {
    x: i32,
    y: i32,
}

#[derive(Deserialize)]
struct SwayOutput {
    name: String,
    #[serde(default)]
    make: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    current_mode: Option<SwayMode>,
    #[serde(default)]
    modes: Vec<SwayMode>,
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default)]
    transform: Option<String>,
    rect: SwayRect,
}

fn mode(m: &SwayMode) -> Mode {
    Mode {
        width: m.width,
        height: m.height,
        refresh: m.refresh,
    }
}

/// Parse `swaymsg -t get_outputs -r`.
pub fn parse_sway_outputs(json: &str) -> Option<Vec<Output>> {
    let outs: Vec<SwayOutput> = serde_json::from_str(json).ok()?;
    Some(
        outs.into_iter()
            .map(|o| {
                let mut modes: Vec<Mode> = o.modes.iter().map(mode).collect();
                modes.dedup();
                Output {
                    description: format!("{} {}", o.make, o.model).trim().to_string(),
                    name: o.name,
                    active: o.active,
                    current: o.current_mode.as_ref().map(mode),
                    modes,
                    scale: o.scale.filter(|s| *s > 0.0).unwrap_or(1.0),
                    transform: o.transform.unwrap_or_else(|| "normal".into()),
                    x: o.rect.x,
                    y: o.rect.y,
                }
            })
            .collect(),
    )
}

/// Which tool configures displays in this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayTool {
    Sway,
    WlrRandr,
}

pub fn probe_outputs() -> Option<(DisplayTool, Vec<Output>)> {
    if std::env::var_os("SWAYSOCK").is_some() {
        if let Some(j) = run("swaymsg", &["-t", "get_outputs", "-r"], T) {
            if let Some(o) = parse_sway_outputs(&j) {
                return Some((DisplayTool::Sway, o));
            }
        }
    }
    if let Some(j) = run("wlr-randr", &["--json"], T) {
        if let Some(o) = parse_wlr_randr(&j) {
            return Some((DisplayTool::WlrRandr, o));
        }
    }
    None
}

#[derive(Deserialize)]
struct WlrMode {
    width: i32,
    height: i32,
    #[serde(default)]
    refresh: f64,
    #[serde(default)]
    current: bool,
}

#[derive(Deserialize)]
struct WlrPos {
    x: i32,
    y: i32,
}

#[derive(Deserialize)]
struct WlrOutput {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    modes: Vec<WlrMode>,
    #[serde(default)]
    position: Option<WlrPos>,
    #[serde(default)]
    transform: Option<String>,
    #[serde(default)]
    scale: Option<f64>,
}

/// Parse `wlr-randr --json` (wlr-randr 0.4+).
pub fn parse_wlr_randr(json: &str) -> Option<Vec<Output>> {
    let outs: Vec<WlrOutput> = serde_json::from_str(json).ok()?;
    Some(
        outs.into_iter()
            .map(|o| {
                let to = |m: &WlrMode| Mode {
                    width: m.width,
                    height: m.height,
                    refresh: (m.refresh * 1000.0).round() as i32,
                };
                Output {
                    current: o.modes.iter().find(|m| m.current).map(to),
                    modes: o.modes.iter().map(to).collect(),
                    name: o.name,
                    description: o.description,
                    active: o.enabled,
                    scale: o.scale.unwrap_or(1.0),
                    transform: o.transform.unwrap_or_else(|| "normal".into()),
                    x: o.position.as_ref().map(|p| p.x).unwrap_or(0),
                    y: o.position.as_ref().map(|p| p.y).unwrap_or(0),
                }
            })
            .collect(),
    )
}

/// What to change on one output.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputChange {
    pub mode: Option<Mode>,
    pub scale: Option<f64>,
    pub transform: Option<String>,
    pub position: Option<(i32, i32)>,
}

/// The command line that applies `change` (program, args).
pub fn output_command(tool: DisplayTool, name: &str, c: &OutputChange) -> (String, Vec<String>) {
    match tool {
        DisplayTool::Sway => {
            let mut a = vec!["output".to_string(), name.to_string()];
            if let Some(m) = &c.mode {
                a.push("mode".into());
                if m.refresh > 0 {
                    a.push(format!(
                        "{}x{}@{:.3}Hz",
                        m.width,
                        m.height,
                        m.refresh as f64 / 1000.0
                    ));
                } else {
                    a.push(format!("{}x{}", m.width, m.height));
                }
            }
            if let Some(s) = c.scale {
                a.push("scale".into());
                a.push(format!("{s}"));
            }
            if let Some(t) = &c.transform {
                a.push("transform".into());
                a.push(t.clone());
            }
            if let Some((x, y)) = c.position {
                a.push("position".into());
                a.push(x.to_string());
                a.push(y.to_string());
            }
            ("swaymsg".into(), a)
        }
        DisplayTool::WlrRandr => {
            let mut a = vec!["--output".to_string(), name.to_string()];
            if let Some(m) = &c.mode {
                a.push("--mode".into());
                if m.refresh > 0 {
                    a.push(format!(
                        "{}x{}@{:.3}Hz",
                        m.width,
                        m.height,
                        m.refresh as f64 / 1000.0
                    ));
                } else {
                    a.push(format!("{}x{}", m.width, m.height));
                }
            }
            if let Some(s) = c.scale {
                a.push("--scale".into());
                a.push(format!("{s}"));
            }
            if let Some(t) = &c.transform {
                a.push("--transform".into());
                a.push(t.clone());
            }
            if let Some((x, y)) = c.position {
                a.push("--pos".into());
                a.push(format!("{x},{y}"));
            }
            ("wlr-randr".into(), a)
        }
    }
}

/// The change that restores `o` exactly (for the "keep these settings?" revert).
pub fn revert_change(o: &Output) -> OutputChange {
    OutputChange {
        mode: o.current.clone(),
        scale: Some(o.scale),
        transform: Some(o.transform.clone()),
        position: Some((o.x, o.y)),
    }
}

// ---------------------------------------------------------------- sound ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundTool {
    Wpctl,
    Pactl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    /// 0.0..=1.5
    pub level: f64,
    pub muted: bool,
}

/// `wpctl get-volume @DEFAULT_AUDIO_SINK@` -> `Volume: 0.40 [MUTED]`.
pub fn parse_wpctl(text: &str) -> Option<Volume> {
    let rest = text.trim().strip_prefix("Volume:")?.trim();
    let level: f64 = rest.split_whitespace().next()?.parse().ok()?;
    Some(Volume {
        level,
        muted: rest.contains("[MUTED]"),
    })
}

/// `pactl get-sink-volume @DEFAULT_SINK@` (first channel percentage) plus
/// `pactl get-sink-mute @DEFAULT_SINK@` (`Mute: yes`).
pub fn parse_pactl(volume: &str, mute: &str) -> Option<Volume> {
    let pct = volume.split('/').map(str::trim).find_map(|s| {
        s.strip_suffix('%')
            .and_then(|n| n.trim().parse::<f64>().ok())
    })?;
    Some(Volume {
        level: pct / 100.0,
        muted: mute.to_ascii_lowercase().contains("yes"),
    })
}

pub fn probe_volume() -> Option<(SoundTool, Volume)> {
    if let Some(t) = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"], T) {
        if let Some(v) = parse_wpctl(&t) {
            return Some((SoundTool::Wpctl, v));
        }
    }
    let v = run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"], T)?;
    let m = run("pactl", &["get-sink-mute", "@DEFAULT_SINK@"], T).unwrap_or_default();
    parse_pactl(&v, &m).map(|v| (SoundTool::Pactl, v))
}

pub fn set_volume(tool: SoundTool, level: f64) -> Result<(), String> {
    let l = level.clamp(0.0, 1.5);
    match tool {
        SoundTool::Wpctl => act(
            "wpctl",
            &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{l:.2}")],
        ),
        SoundTool::Pactl => act(
            "pactl",
            &[
                "set-sink-volume",
                "@DEFAULT_SINK@",
                &format!("{}%", (l * 100.0).round() as i64),
            ],
        ),
    }
}

pub fn set_mute(tool: SoundTool, muted: bool) -> Result<(), String> {
    let v = if muted { "1" } else { "0" };
    match tool {
        SoundTool::Wpctl => act("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", v]),
        SoundTool::Pactl => act("pactl", &["set-sink-mute", "@DEFAULT_SINK@", v]),
    }
}

// -------------------------------------------------------------- network ----

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub device: String,
    pub kind: String,
    pub state: String,
    pub connection: String,
    pub addresses: Vec<String>,
}

/// `nmcli -t -f DEVICE,TYPE,STATE,CONNECTION device` (colon separated,
/// `\:` escapes a colon inside a field).
pub fn parse_nmcli(text: &str) -> Vec<Link> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut fields = Vec::new();
        let mut cur = String::new();
        let mut esc = false;
        for c in line.chars() {
            match (esc, c) {
                (true, c) => {
                    cur.push(c);
                    esc = false;
                }
                (false, '\\') => esc = true,
                (false, ':') => fields.push(std::mem::take(&mut cur)),
                (false, c) => cur.push(c),
            }
        }
        fields.push(cur);
        if fields.len() >= 4 && fields[1] != "loopback" {
            out.push(Link {
                device: fields[0].clone(),
                kind: fields[1].clone(),
                state: fields[2].clone(),
                connection: fields[3].clone(),
                addresses: Vec::new(),
            });
        }
    }
    out
}

#[derive(Deserialize)]
struct IpAddrInfo {
    #[serde(default)]
    local: String,
    #[serde(default)]
    prefixlen: u32,
    #[serde(default)]
    scope: String,
}

#[derive(Deserialize)]
struct IpLink {
    ifname: String,
    #[serde(default)]
    operstate: String,
    #[serde(default)]
    link_type: String,
    #[serde(default)]
    addr_info: Vec<IpAddrInfo>,
}

/// `ip -j addr` (iproute2), without loopback and link-local addresses.
pub fn parse_ip_addr(json: &str) -> Vec<Link> {
    let links: Vec<IpLink> = serde_json::from_str(json).unwrap_or_default();
    links
        .into_iter()
        .filter(|l| l.link_type != "loopback")
        .map(|l| Link {
            kind: if l.ifname.starts_with("wl") {
                "wifi".into()
            } else {
                "ethernet".into()
            },
            state: l.operstate.to_ascii_lowercase(),
            connection: String::new(),
            addresses: l
                .addr_info
                .iter()
                .filter(|a| a.scope == "global")
                .map(|a| format!("{}/{}", a.local, a.prefixlen))
                .collect(),
            device: l.ifname,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetTool {
    Nmcli,
    Ip,
}

pub fn probe_network() -> Option<(NetTool, Vec<Link>, Option<bool>)> {
    let addrs = run("ip", &["-j", "addr"], T)
        .map(|j| parse_ip_addr(&j))
        .unwrap_or_default();
    if let Some(t) = run(
        "nmcli",
        &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"],
        T,
    ) {
        let mut links = parse_nmcli(&t);
        for l in &mut links {
            if let Some(a) = addrs.iter().find(|a| a.device == l.device) {
                l.addresses = a.addresses.clone();
            }
        }
        let wifi = run("nmcli", &["radio", "wifi"], T).map(|s| s.trim() == "enabled");
        return Some((NetTool::Nmcli, links, wifi));
    }
    if addrs.is_empty() {
        None
    } else {
        Some((NetTool::Ip, addrs, None))
    }
}

// ---------------------------------------------------------------- power ----

#[derive(Debug, Clone, PartialEq)]
pub struct Supply {
    pub name: String,
    pub kind: String,
    pub capacity: Option<u32>,
    pub status: String,
    pub online: Option<bool>,
}

/// Read every supply under a sysfs `power_supply` directory.
pub fn read_supplies(root: &Path) -> Vec<Supply> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut v: Vec<Supply> = rd
        .flatten()
        .map(|e| {
            let p = e.path();
            let r = |f: &str| {
                std::fs::read_to_string(p.join(f))
                    .map(|s| s.trim().to_string())
                    .ok()
            };
            Supply {
                name: e.file_name().to_string_lossy().to_string(),
                kind: r("type").unwrap_or_default(),
                capacity: r("capacity").and_then(|s| s.parse().ok()),
                status: r("status").unwrap_or_default(),
                online: r("online").map(|s| s == "1"),
            }
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

// ---------------------------------------------------------------- tests ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_and_meminfo() {
        let (n, v) = parse_os_release("NAME=\"Debian GNU/Linux\"\nPRETTY_NAME=\"Kiln GNU/Linux 13 (trixie)\"\nVERSION_ID=\"13\"\n");
        assert_eq!(n, "Kiln GNU/Linux 13 (trixie)");
        assert_eq!(v, "13");
        assert_eq!(parse_os_release("NAME=X\n").0, "X");
        assert_eq!(
            parse_meminfo_kib("MemTotal:        6061020 kB\nMemFree: 1 kB\n"),
            6_061_020
        );
        assert_eq!(human_kib(6_061_020), "5.8 GiB");
        assert_eq!(human_kib(512 * 1024), "512 MiB");
    }

    #[test]
    fn lscpu_arm_and_x86() {
        let arm = "Architecture: aarch64\nCPU(s): 4\nVendor ID: Apple\nModel name: -\n";
        assert_eq!(parse_lscpu(arm), ("Apple".into(), 4));
        let x86 = "CPU(s): 8\nVendor ID: GenuineIntel\nModel name: Intel(R) Core(TM) i7\n";
        assert_eq!(parse_lscpu(x86).0, "GenuineIntel Intel(R) Core(TM) i7");
        let generic = "CPU(s): 2\nVendor ID: ARM\nModel name: Cortex-A72\n";
        assert_eq!(parse_lscpu(generic), ("ARM Cortex-A72".into(), 2));
    }

    #[test]
    fn sway_outputs() {
        let j = r#"[{"name":"HEADLESS-1","make":"headless","model":"headless","active":true,
            "current_mode":{"width":1280,"height":800,"refresh":60000},"modes":[],
            "scale":1.0,"transform":"normal","rect":{"x":0,"y":0,"width":1280,"height":800}},
            {"name":"Virtual-1","make":"Red Hat","model":"QEMU","active":true,
            "current_mode":{"width":1920,"height":1080,"refresh":60000},
            "modes":[{"width":1920,"height":1080,"refresh":60000},{"width":1280,"height":800,"refresh":60000}],
            "scale":1.5,"transform":"90","rect":{"x":1280,"y":0,"width":720,"height":1280}}]"#;
        let o = parse_sway_outputs(j).unwrap();
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].current.as_ref().unwrap().label(), "1280 × 800 @ 60 Hz");
        assert_eq!(o[1].modes.len(), 2);
        assert_eq!(o[1].transform, "90");
        assert_eq!((o[1].x, o[1].scale), (1280, 1.5));
        assert!(parse_sway_outputs("nope").is_none());
    }

    #[test]
    fn wlr_randr_outputs() {
        let j = r#"[{"name":"Virtual-1","description":"QEMU","enabled":true,
          "modes":[{"width":1280,"height":800,"refresh":59.999,"preferred":true,"current":true}],
          "position":{"x":0,"y":0},"transform":"normal","scale":1.000000}]"#;
        let o = parse_wlr_randr(j).unwrap();
        assert_eq!(o[0].current.as_ref().unwrap().refresh, 59_999);
    }

    #[test]
    fn output_commands() {
        let c = OutputChange {
            mode: Some(Mode {
                width: 1280,
                height: 800,
                refresh: 60_000,
            }),
            scale: Some(1.25),
            transform: Some("90".into()),
            position: Some((10, 20)),
        };
        let (p, a) = output_command(DisplayTool::Sway, "HDMI-A-1", &c);
        assert_eq!(p, "swaymsg");
        assert_eq!(
            a.join(" "),
            "output HDMI-A-1 mode 1280x800@60.000Hz scale 1.25 transform 90 position 10 20"
        );
        let (p, a) = output_command(DisplayTool::WlrRandr, "HDMI-A-1", &c);
        assert_eq!(p, "wlr-randr");
        assert_eq!(
            a.join(" "),
            "--output HDMI-A-1 --mode 1280x800@60.000Hz --scale 1.25 --transform 90 --pos 10,20"
        );
    }

    #[test]
    fn volume_parsers() {
        assert_eq!(
            parse_wpctl("Volume: 0.40\n"),
            Some(Volume {
                level: 0.4,
                muted: false
            })
        );
        assert_eq!(
            parse_wpctl("Volume: 1.00 [MUTED]"),
            Some(Volume {
                level: 1.0,
                muted: true
            })
        );
        assert!(parse_wpctl("garbage").is_none());
        let v = "Volume: front-left: 26214 /  40% / -23.88 dB,   front-right: 26214 /  40% / -23.88 dB\n";
        assert_eq!(
            parse_pactl(v, "Mute: no"),
            Some(Volume {
                level: 0.4,
                muted: false
            })
        );
        assert!(parse_pactl(v, "Mute: yes").unwrap().muted);
    }

    #[test]
    fn network_parsers() {
        let n = "enp0s1:ethernet:connected:Wired connection 1\nlo:loopback:connected (externally):lo\nwlan0:wifi:disconnected:\nx\\:y:ethernet:unavailable:\n";
        let l = parse_nmcli(n);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].connection, "Wired connection 1");
        assert_eq!(l[2].device, "x:y");
        let j = r#"[{"ifname":"lo","link_type":"loopback","operstate":"UNKNOWN","addr_info":[]},
          {"ifname":"enp0s1","link_type":"ether","operstate":"UP","addr_info":[
          {"family":"inet","local":"10.0.2.15","prefixlen":24,"scope":"global"},
          {"family":"inet6","local":"fe80::1","prefixlen":64,"scope":"link"}]}]"#;
        let a = parse_ip_addr(j);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].addresses, vec!["10.0.2.15/24".to_string()]);
        assert_eq!(a[0].state, "up");
    }

    #[test]
    fn power_supplies_from_a_fake_sysfs() {
        let d = crate::store::tests::tmp_dir("power");
        std::fs::create_dir_all(d.join("BAT0")).unwrap();
        std::fs::write(d.join("BAT0/type"), "Battery\n").unwrap();
        std::fs::write(d.join("BAT0/capacity"), "73\n").unwrap();
        std::fs::write(d.join("BAT0/status"), "Discharging\n").unwrap();
        std::fs::create_dir_all(d.join("AC")).unwrap();
        std::fs::write(d.join("AC/type"), "Mains\n").unwrap();
        std::fs::write(d.join("AC/online"), "0\n").unwrap();
        let s = read_supplies(&d);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "AC");
        assert_eq!(s[0].online, Some(false));
        assert_eq!(s[1].capacity, Some(73));
        assert!(read_supplies(&d.join("missing")).is_empty());
    }

    #[test]
    fn run_handles_missing_programs() {
        assert!(run("definitely-not-a-program-glaze", &[], T).is_none());
        assert!(!has("definitely-not-a-program-glaze"));
    }
}
