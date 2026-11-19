//! Distro-aware branding. Kiln artwork shows only on Kiln; every other
//! distribution gets the neutral Glaze mark plus its own name as text.

use std::path::PathBuf;

use crate::config::BrandingConfig;

/// Fields of os-release(5) that Glaze uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsRelease {
    pub id: String,
    pub id_like: Vec<String>,
    pub name: String,
    pub pretty_name: String,
    pub version_id: String,
    /// Icon-theme name from `LOGO=`.
    pub logo: String,
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    let b = v.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        let inner = &v[1..v.len() - 1];
        if b[0] == b'"' {
            // os-release allows backslash escapes inside double quotes.
            let mut out = String::new();
            let mut it = inner.chars();
            while let Some(c) = it.next() {
                if c == '\\' {
                    if let Some(n) = it.next() {
                        out.push(n);
                    }
                } else {
                    out.push(c);
                }
            }
            return out;
        }
        return inner.to_string();
    }
    v.to_string()
}

impl OsRelease {
    /// Parse os-release text. Unknown keys, comments and junk lines are ignored.
    pub fn parse(text: &str) -> OsRelease {
        let mut o = OsRelease::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = unquote(v);
            match k.trim() {
                "ID" => o.id = v.to_ascii_lowercase(),
                "ID_LIKE" => {
                    o.id_like = v
                        .split_whitespace()
                        .map(|s| s.to_ascii_lowercase())
                        .collect()
                }
                "NAME" => o.name = v,
                "PRETTY_NAME" => o.pretty_name = v,
                "VERSION_ID" => o.version_id = v,
                "LOGO" => o.logo = v,
                _ => {}
            }
        }
        o
    }

    /// Read `/etc/os-release`, falling back to `/usr/lib/os-release`; an
    /// empty value when neither exists.
    pub fn load() -> OsRelease {
        for p in ["/etc/os-release", "/usr/lib/os-release"] {
            if let Ok(t) = std::fs::read_to_string(p) {
                return OsRelease::parse(&t);
            }
        }
        OsRelease::default()
    }

    pub fn is_kiln(&self) -> bool {
        self.id == "kiln" || self.id_like.iter().any(|i| i == "kiln")
    }

    /// `PRETTY_NAME`, else `NAME`, else empty.
    pub fn display_name(&self) -> &str {
        if !self.pretty_name.is_empty() {
            &self.pretty_name
        } else {
            &self.name
        }
    }
}

/// Which artwork to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Logo {
    Kiln,
    Glaze,
    /// Image file chosen in `[branding] logo`.
    File(PathBuf),
}

/// Resolved branding for this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brand {
    pub logo: Logo,
    /// Text shown next to or instead of the logo.
    pub name: String,
    /// Icon-theme name from os-release `LOGO`, preferred on the Start button
    /// when `logo = auto` and the machine is not Kiln.
    pub os_icon: Option<String>,
    pub is_kiln: bool,
}

impl Brand {
    /// Resolve from os-release data and the `[branding]` section.
    pub fn resolve(os: &OsRelease, cfg: &BrandingConfig) -> Brand {
        let is_kiln = os.is_kiln();
        let choice = cfg.logo.trim();
        let auto = choice.is_empty() || choice.eq_ignore_ascii_case("auto");
        let logo = if auto {
            if is_kiln {
                Logo::Kiln
            } else {
                Logo::Glaze
            }
        } else if choice.eq_ignore_ascii_case("kiln") {
            Logo::Kiln
        } else if choice.eq_ignore_ascii_case("glaze") {
            Logo::Glaze
        } else {
            Logo::File(PathBuf::from(choice))
        };
        let cname = cfg.name.trim();
        let name = if !cname.is_empty() && !cname.eq_ignore_ascii_case("auto") {
            cname.to_string()
        } else if is_kiln {
            "Kiln GNU/Linux".to_string()
        } else if !os.display_name().is_empty() {
            os.display_name().to_string()
        } else {
            "Linux".to_string()
        };
        let os_icon = (auto && !is_kiln && !os.logo.is_empty()).then(|| os.logo.clone());
        Brand {
            logo,
            name,
            os_icon,
            is_kiln,
        }
    }

    /// Resolve from the live system.
    pub fn detect(cfg: &BrandingConfig) -> Brand {
        Brand::resolve(&OsRelease::load(), cfg)
    }

    /// Short name for headers: "Kiln GNU/Linux" -> "Kiln",
    /// "Debian GNU/Linux 13 (trixie)" -> "Debian".
    pub fn short_name(&self) -> String {
        let n = self.name.trim();
        let n = n.split(" GNU/Linux").next().unwrap_or(n);
        n.split_whitespace().next().unwrap_or(n).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KILN: &str = "PRETTY_NAME=\"Kiln GNU/Linux 13 (trixie)\"\nNAME=\"Kiln GNU/Linux\"\nID=kiln\nID_LIKE=debian\nVERSION_ID=\"13\"\n";
    const VOID: &str = "NAME=\"Void\"\nID=\"void\"\nPRETTY_NAME=\"Void Linux\"\nLOGO=void-logo\n";
    const DEBIAN: &str = "PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nNAME=\"Debian GNU/Linux\"\nVERSION_ID=\"13\"\nID=debian\n";
    const FEDORA: &str = "NAME='Fedora Linux'\nVERSION_ID=41\nID=fedora\nPRETTY_NAME=\"Fedora Linux 41 (Workstation Edition)\"\nLOGO=fedora-logo-icon\n# comment\n";
    const DERIVED: &str = "NAME=\"Spin\"\nID=spin\nID_LIKE=\"debian kiln\"\n";

    fn auto() -> BrandingConfig {
        BrandingConfig::default()
    }

    #[test]
    fn kiln_uses_kiln_brand() {
        let os = OsRelease::parse(KILN);
        assert!(os.is_kiln());
        assert_eq!(os.version_id, "13");
        let b = Brand::resolve(&os, &auto());
        assert_eq!(b.logo, Logo::Kiln);
        assert_eq!(b.name, "Kiln GNU/Linux");
        assert_eq!(b.short_name(), "Kiln");
        assert!(b.os_icon.is_none());
    }

    #[test]
    fn id_like_kiln_counts() {
        let os = OsRelease::parse(DERIVED);
        assert_eq!(os.id_like, ["debian", "kiln"]);
        assert_eq!(Brand::resolve(&os, &auto()).logo, Logo::Kiln);
    }

    #[test]
    fn void_is_neutral_with_logo_icon() {
        let os = OsRelease::parse(VOID);
        assert!(!os.is_kiln());
        let b = Brand::resolve(&os, &auto());
        assert_eq!(b.logo, Logo::Glaze);
        assert_eq!(b.name, "Void Linux");
        assert_eq!(b.os_icon.as_deref(), Some("void-logo"));
    }

    #[test]
    fn debian_is_neutral() {
        let b = Brand::resolve(&OsRelease::parse(DEBIAN), &auto());
        assert_eq!(b.logo, Logo::Glaze);
        assert_eq!(b.name, "Debian GNU/Linux 13 (trixie)");
        assert_eq!(b.short_name(), "Debian");
        assert!(b.os_icon.is_none());
    }

    #[test]
    fn fedora_single_quotes_and_comments() {
        let os = OsRelease::parse(FEDORA);
        assert_eq!(os.name, "Fedora Linux");
        assert_eq!(os.version_id, "41");
        let b = Brand::resolve(&os, &auto());
        assert_eq!(b.name, "Fedora Linux 41 (Workstation Edition)");
        assert_eq!(b.os_icon.as_deref(), Some("fedora-logo-icon"));
    }

    #[test]
    fn missing_file_is_neutral() {
        let os = OsRelease::parse("");
        assert_eq!(os, OsRelease::default());
        let b = Brand::resolve(&os, &auto());
        assert_eq!(b.logo, Logo::Glaze);
        assert_eq!(b.name, "Linux");
    }

    #[test]
    fn overrides_win() {
        let os = OsRelease::parse(DEBIAN);
        let cfg = BrandingConfig {
            logo: "kiln".into(),
            name: "My Box".into(),
        };
        let b = Brand::resolve(&os, &cfg);
        assert_eq!(b.logo, Logo::Kiln);
        assert_eq!(b.name, "My Box");
        assert!(b.os_icon.is_none());
        let cfg = BrandingConfig {
            logo: "/opt/x.svg".into(),
            name: "AUTO".into(),
        };
        let b = Brand::resolve(&OsRelease::parse(KILN), &cfg);
        assert_eq!(b.logo, Logo::File("/opt/x.svg".into()));
        assert_eq!(b.name, "Kiln GNU/Linux");
        let k = BrandingConfig {
            logo: "glaze".into(),
            name: "auto".into(),
        };
        assert_eq!(
            Brand::resolve(&OsRelease::parse(KILN), &k).logo,
            Logo::Glaze
        );
    }

    #[test]
    fn escapes_in_double_quotes() {
        let os = OsRelease::parse("NAME=\"A \\\"B\\\" C\"\n");
        assert_eq!(os.name, "A \"B\" C");
    }
}
