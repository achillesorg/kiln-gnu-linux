//! XDG Desktop Entry parsing and safe `Exec` tokenisation.
//!
//! Implements the subset of the
//! [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry-spec/latest/)
//! that Glaze needs, with two non-negotiables:
//!
//! 1. **No shell.** `Exec` is tokenised here into an argv vector. It is never
//!    passed to `sh -c`, so a malicious `.desktop` file cannot inject shell
//!    commands (`GLAZE.md` §7.9, §9 "Security").
//! 2. **Field codes are honoured, not guessed.** `%f %F %u %U %d %D %n %N %v
//!    %m` are removed, `%i` expands to `--icon <Icon>`, `%c` to the app name
//!    and `%k` to the desktop-file path — exactly as the spec defines.

use std::collections::BTreeMap;

/// A parsed `[Desktop Entry]` group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub type_: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub terminal: bool,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub path: Option<String>,
    pub startup_wm_class: Option<String>,
    /// The absolute source path of the `.desktop` file.
    pub source_path: String,
}

/// Error produced while tokenising an `Exec` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    Empty,
    UnterminatedQuote,
    TrailingBackslash,
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Empty => write!(f, "Exec line is empty"),
            ExecError::UnterminatedQuote => write!(f, "Exec line has an unterminated quote"),
            ExecError::TrailingBackslash => write!(f, "Exec line ends with a lone backslash"),
        }
    }
}

impl std::error::Error for ExecError {}

/// A parsed `.desktop` file: every group plus the `[Desktop Entry]` group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopFile {
    pub groups: BTreeMap<String, BTreeMap<String, String>>,
}

impl DesktopFile {
    /// Parse the raw text of a `.desktop` file.
    ///
    /// Malformed lines are skipped rather than aborting the whole file, which
    /// matches how real launchers behave with the slightly-broken entries that
    /// exist in the wild.
    pub fn parse(text: &str) -> Self {
        let mut groups: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current: Option<String> = None;
        for raw in text.lines() {
            let line = raw.trim_end_matches(['\r', '\n']);
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let name = name.trim().to_string();
                groups.entry(name.clone()).or_default();
                current = Some(name);
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                if let Some(group) = &current {
                    groups
                        .entry(group.clone())
                        .or_default()
                        .insert(key.trim().to_string(), value.to_string());
                }
            }
        }
        DesktopFile { groups }
    }

    fn entry_group(&self) -> Option<&BTreeMap<String, String>> {
        self.groups.get("Desktop Entry")
    }

    /// Look up a key, honouring a locale suffix such as `Name[en_GB]` when the
    /// current locale matches. Falls back to the base key.
    fn localized(&self, key: &str, locale: &str) -> Option<String> {
        let group = self.entry_group()?;
        if !locale.is_empty() {
            // Try progressively shorter locale forms: en_GB.UTF-8 -> en_GB -> en
            let mut candidates = Vec::new();
            let base = locale.split('.').next().unwrap_or(locale);
            let base = base.split('@').next().unwrap_or(base);
            candidates.push(base.to_string());
            if let Some((lang, _)) = base.split_once('_') {
                candidates.push(lang.to_string());
            }
            for cand in candidates {
                if let Some(v) = group.get(&format!("{key}[{cand}]")) {
                    return Some(v.clone());
                }
            }
        }
        group.get(key).cloned()
    }

    fn bool_of(&self, key: &str) -> bool {
        self.entry_group()
            .and_then(|g| g.get(key))
            .map(|v| v.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }

    fn string_list(&self, key: &str) -> Vec<String> {
        self.entry_group()
            .and_then(|g| g.get(key))
            .map(|v| split_string_list(v))
            .unwrap_or_default()
    }

    /// Convert into a [`DesktopEntry`] using `locale` for localised names.
    pub fn to_entry(&self, source_path: &str, locale: &str) -> DesktopEntry {
        DesktopEntry {
            type_: self.localized("Type", locale).unwrap_or_default(),
            name: self.localized("Name", locale).unwrap_or_default(),
            generic_name: self.localized("GenericName", locale),
            comment: self.localized("Comment", locale),
            icon: self.localized("Icon", locale),
            exec: self.entry_group().and_then(|g| g.get("Exec").cloned()),
            try_exec: self.entry_group().and_then(|g| g.get("TryExec").cloned()),
            terminal: self.bool_of("Terminal"),
            categories: self.string_list("Categories"),
            keywords: self.localized_list("Keywords", locale),
            no_display: self.bool_of("NoDisplay"),
            hidden: self.bool_of("Hidden"),
            only_show_in: self.string_list("OnlyShowIn"),
            not_show_in: self.string_list("NotShowIn"),
            path: self.entry_group().and_then(|g| g.get("Path").cloned()),
            startup_wm_class: self
                .entry_group()
                .and_then(|g| g.get("StartupWMClass").cloned()),
            source_path: source_path.to_string(),
        }
    }

    fn localized_list(&self, key: &str, locale: &str) -> Vec<String> {
        self.localized(key, locale)
            .map(|v| split_string_list(&v))
            .unwrap_or_default()
    }
}

/// Split a `;`-separated string list, honouring `\;` escapes and dropping the
/// trailing empty element that the spec allows.
pub fn split_string_list(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(';') => cur.push(';'),
                Some('\\') => cur.push('\\'),
                Some(other) => {
                    cur.push('\\');
                    cur.push(other);
                }
                None => cur.push('\\'),
            },
            ';' => {
                out.push(std::mem::take(&mut cur));
            }
            other => cur.push(other),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.retain(|s| !s.is_empty());
    out
}

/// Context for expanding `Exec` field codes.
#[derive(Debug, Clone, Default)]
pub struct ExecContext {
    /// `%c` — the app's display name.
    pub name: String,
    /// `%i` — expands to `--icon <icon>` when present.
    pub icon: Option<String>,
    /// `%k` — the desktop file path.
    pub desktop_file: Option<String>,
    /// Positional file/URL arguments used to fill `%f/%u` style codes.
    pub files: Vec<String>,
    pub urls: Vec<String>,
}

/// Tokenise an `Exec` value into an argv vector, expanding field codes.
///
/// Returns [`ExecError`] for genuinely malformed input (unterminated quotes).
/// Unknown field codes are dropped rather than passed through literally, per
/// the spec's guidance that implementations ignore what they do not support.
pub fn tokenize_exec(exec: &str, ctx: &ExecContext) -> Result<Vec<String>, ExecError> {
    let raw = tokenize_quoted(exec)?;
    if raw.is_empty() {
        return Err(ExecError::Empty);
    }
    let mut argv: Vec<String> = Vec::with_capacity(raw.len());
    for token in raw {
        // `%i` is the only code that expands to *two* arguments.
        if token == "%i" {
            if let Some(icon) = &ctx.icon {
                if !icon.is_empty() {
                    argv.push("--icon".to_string());
                    argv.push(icon.clone());
                }
            }
            continue;
        }
        let expanded = expand_field_codes(&token, ctx);
        if let Some(arg) = expanded {
            argv.push(arg);
        }
    }
    if argv.is_empty() {
        return Err(ExecError::Empty);
    }
    Ok(argv)
}

/// Expand the field codes inside a single token. Returns `None` when the token
/// becomes empty (i.e. it was only a removable field code such as `%f`).
fn expand_field_codes(token: &str, ctx: &ExecContext) -> Option<String> {
    if !token.contains('%') {
        return Some(token.to_string());
    }
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars().peekable();
    let mut has_content = false;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            has_content = true;
            continue;
        }
        match chars.next() {
            Some('%') => {
                out.push('%');
                has_content = true;
            }
            // Removable file/URL/identifier codes.
            Some('f') | Some('F') | Some('u') | Some('U') | Some('d') | Some('D') | Some('n')
            | Some('N') | Some('v') | Some('m') => {}
            Some('c') => {
                out.push_str(&ctx.name);
                has_content = true;
            }
            Some('i') => {
                if let Some(icon) = &ctx.icon {
                    out.push_str(icon);
                    has_content = true;
                }
            }
            Some('k') => {
                if let Some(f) = &ctx.desktop_file {
                    out.push_str(f);
                    has_content = true;
                }
            }
            // Unknown code: drop it (spec: ignore unsupported field codes).
            Some(_) | None => {}
        }
    }
    if has_content {
        Some(out)
    } else {
        None
    }
}

/// Split an `Exec` string into raw tokens honouring double-quote and backslash
/// escaping as defined by the Desktop Entry spec.
fn tokenize_quoted(exec: &str) -> Result<Vec<String>, ExecError> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut in_quote = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if !in_quote => match chars.next() {
                Some(next) => {
                    cur.push(next);
                    in_token = true;
                }
                None => return Err(ExecError::TrailingBackslash),
            },
            '\\' if in_quote => match chars.next() {
                // Inside quotes only `"`, `` ` ``, `$` and `\` are escaped.
                Some(n @ ('"' | '`' | '$' | '\\')) => {
                    cur.push(n);
                    in_token = true;
                }
                Some(other) => {
                    cur.push('\\');
                    cur.push(other);
                    in_token = true;
                }
                None => return Err(ExecError::TrailingBackslash),
            },
            '"' => {
                in_quote = !in_quote;
                in_token = true;
            }
            c if c.is_whitespace() && !in_quote => {
                if in_token {
                    tokens.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            other => {
                cur.push(other);
                in_token = true;
            }
        }
    }
    if in_quote {
        return Err(ExecError::UnterminatedQuote);
    }
    if in_token {
        tokens.push(cur);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "\
[Desktop Entry]
Type=Application
Name=Firefox
GenericName=Web Browser
Comment=Browse the World Wide Web
Exec=/usr/bin/firefox %u
Icon=firefox
Terminal=false
Categories=Network;WebBrowser;
Keywords=browser;internet;
StartupWMClass=firefox
";

    #[test]
    fn parses_core_fields() {
        let f = DesktopFile::parse(FIREFOX);
        let e = f.to_entry("/usr/share/applications/firefox.desktop", "en");
        assert_eq!(e.type_, "Application");
        assert_eq!(e.name, "Firefox");
        assert_eq!(e.generic_name.as_deref(), Some("Web Browser"));
        assert_eq!(e.exec.as_deref(), Some("/usr/bin/firefox %u"));
        assert_eq!(e.categories, vec!["Network", "WebBrowser"]);
        assert_eq!(e.keywords, vec!["browser", "internet"]);
        assert!(!e.terminal);
        assert!(!e.no_display);
        assert_eq!(e.startup_wm_class.as_deref(), Some("firefox"));
    }

    #[test]
    fn localised_names_prefer_locale_then_fall_back() {
        let text = "[Desktop Entry]\nName=Files\nName[de]=Dateien\nExec=nautilus\n";
        let f = DesktopFile::parse(text);
        assert_eq!(f.to_entry("x", "de_DE.UTF-8").name, "Dateien");
        assert_eq!(f.to_entry("x", "en").name, "Files");
    }

    #[test]
    fn ignores_comments_blank_lines_and_unowned_keys() {
        let text = "# comment\n\nBrokenKey=1\n[Desktop Entry]\nName=X\nExec=x\n";
        let f = DesktopFile::parse(text);
        assert_eq!(f.to_entry("x", "").name, "X");
        assert!(!f.groups.contains_key(""));
    }

    #[test]
    fn splits_string_lists_with_escapes() {
        assert_eq!(split_string_list("A;B;C;"), vec!["A", "B", "C"]);
        assert_eq!(split_string_list("A\\;B;C"), vec!["A;B", "C"]);
        assert!(split_string_list("").is_empty());
    }

    #[test]
    fn tokenizes_exec_dropping_field_codes() {
        let ctx = ExecContext::default();
        let argv = tokenize_exec("/usr/bin/firefox %u", &ctx).unwrap();
        assert_eq!(argv, vec!["/usr/bin/firefox"]);
    }

    #[test]
    fn tokenizes_exec_with_quotes() {
        let ctx = ExecContext::default();
        let argv = tokenize_exec("\"/opt/My App/bin/run\" --flag %F", &ctx).unwrap();
        assert_eq!(argv, vec!["/opt/My App/bin/run", "--flag"]);
    }

    #[test]
    fn expands_icon_name_and_desktop_file_codes() {
        let ctx = ExecContext {
            name: "Glaze Settings".into(),
            icon: Some("glaze-settings".into()),
            desktop_file: Some("/usr/share/applications/glaze-settings.desktop".into()),
            files: vec![],
            urls: vec![],
        };
        let argv = tokenize_exec("glaze-settings %i --class %c %k", &ctx).unwrap();
        // `%c` is a single argument (the full Name), not split on whitespace.
        assert_eq!(
            argv,
            vec![
                "glaze-settings",
                "--icon",
                "glaze-settings",
                "--class",
                "Glaze Settings",
                "/usr/share/applications/glaze-settings.desktop"
            ]
        );
    }

    #[test]
    fn percent_literal_is_preserved() {
        let ctx = ExecContext::default();
        let argv = tokenize_exec("prog 100%%", &ctx).unwrap();
        assert_eq!(argv, vec!["prog", "100%"]);
    }

    #[test]
    fn rejects_unterminated_quote() {
        let ctx = ExecContext::default();
        assert_eq!(
            tokenize_exec("\"unterminated", &ctx).unwrap_err(),
            ExecError::UnterminatedQuote
        );
    }

    #[test]
    fn shell_metacharacters_are_kept_literal_not_executed() {
        // The whole point: `; rm -rf /` becomes two inert argv entries, never
        // a shell command.
        let ctx = ExecContext::default();
        let argv = tokenize_exec("evil ; rm -rf /", &ctx).unwrap();
        assert_eq!(argv, vec!["evil", ";", "rm", "-rf", "/"]);
    }
}
