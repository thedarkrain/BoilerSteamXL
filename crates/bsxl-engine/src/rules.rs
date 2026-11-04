//! Permanent exclusion rules and the allowlist.
//!
//! Matching is case-insensitive and separator-agnostic (see [`crate::norm_path`]).
//! Rules marked `inside_game_only` only match folders *inside* a game folder, so a rule
//! for an internal folder such as `Tools` never hides a game that is itself called
//! "Tools".

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

use crate::{norm_name, norm_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// A folder name, or a sequence such as `Engine/Extras/Redist`.
    DirName,
    /// A full folder path; everything below it is excluded.
    DirPath,
    /// An executable file name; `*` and `?` wildcards allowed.
    FileName,
    /// A glob on the full path (`*`, `?`, `**`). Without a `/` it matches the file name.
    Glob,
    /// A regular expression on the full normalized path (advanced).
    Regex,
    /// A known executable category, see [`Category`].
    Category,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Uninstaller,
    Installer,
    Redistributable,
    Verification,
    LanguageSelector,
    CrashReporter,
    Updater,
    AntiCheat,
    Helper,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Uninstaller,
        Category::Installer,
        Category::Redistributable,
        Category::Verification,
        Category::LanguageSelector,
        Category::CrashReporter,
        Category::Updater,
        Category::AntiCheat,
        Category::Helper,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Category::Uninstaller => "uninstaller",
            Category::Installer => "installer",
            Category::Redistributable => "redistributable",
            Category::Verification => "verification",
            Category::LanguageSelector => "language_selector",
            Category::CrashReporter => "crash_reporter",
            Category::Updater => "updater",
            Category::AntiCheat => "anti_cheat",
            Category::Helper => "helper",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Uninstaller => "uninstaller",
            Category::Installer => "installer / setup",
            Category::Redistributable => "redistributable",
            Category::Verification => "verification utility",
            Category::LanguageSelector => "language selector",
            Category::CrashReporter => "crash reporter",
            Category::Updater => "updater / patcher",
            Category::AntiCheat => "anti-cheat service",
            Category::Helper => "helper process",
        }
    }

    fn from_key(key: &str) -> Option<Category> {
        let key = norm_name(key);
        Category::ALL.into_iter().find(|c| norm_name(c.key()) == key)
    }
}

/// The category an executable belongs to, judged from its file stem and (when the file
/// has one) its version-resource description. Never runs the file.
pub fn categorize(stem: &str, description: Option<&str>) -> Option<Category> {
    let s = norm_name(stem);
    let d = description.map(norm_name).unwrap_or_default();
    let any = |needles: &[&str]| needles.iter().any(|n| s.contains(n) || d.contains(n));

    if s.starts_with("unins") || any(&["uninstall"]) {
        return Some(Category::Uninstaller);
    }
    if any(&[
        "vcredist", "redist", "dxsetup", "dxwebsetup", "directx", "dotnetfx", "ndp4", "ndp35",
        "physx", "oalinst", "xnafx", "ueprereq", "ue4prereq", "prerequisite", "windowsdesktopruntime",
    ]) {
        return Some(Category::Redistributable);
    }
    if any(&["quicksfv", "quickpar", "checksum", "md5", "sfvcheck", "verifier", "hashcheck"])
        || s == "sfv"
        || s.ends_with("sfv")
    {
        return Some(Category::Verification);
    }
    if s.contains("language")
        && (s.contains("select") || s.contains("chang") || s.contains("setup") || s.contains("switch") || s == "language")
        || s == "langselect"
        || s == "langselector"
    {
        return Some(Category::LanguageSelector);
    }
    if s == "setup"
        || s.starts_with("setup")
        || s.ends_with("setup")
        || s.starts_with("install")
        || any(&["installer"])
    {
        return Some(Category::Installer);
    }
    if any(&[
        "crashreport", "crashhandler", "crashpad", "crashsender", "bugsplat", "bugreport",
        "errorreport", "crashuploader",
    ]) {
        return Some(Category::CrashReporter);
    }
    if s == "update" || s.starts_with("updater") || s.ends_with("updater") || s.contains("autoupdate") || s.ends_with("patcher") {
        return Some(Category::Updater);
    }
    if any(&["easyanticheat", "battleye", "beservice", "anticheat", "eaclauncher"]) || s == "eac" {
        return Some(Category::AntiCheat);
    }
    if any(&[
        "cefsubprocess", "cefprocess", "webhelper", "unrealcefsubprocess", "createdump",
        "notificationhelper", "awesomiumprocess", "dxdiag", "vconsole",
    ]) {
        return Some(Category::Helper);
    }
    None
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub kind: RuleKind,
    pub pattern: String,
    pub enabled: bool,
    /// Shipped with the app; "Reset to recommended" restores these.
    #[serde(default)]
    pub builtin: bool,
    /// Only match folders strictly inside a game folder.
    #[serde(default)]
    pub inside_game_only: bool,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
    /// Normalized executable paths that are never excluded.
    #[serde(default)]
    pub allowlist: Vec<String>,
}

/// Why an executable was excluded, for display in the review section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exclusion {
    pub rule_id: String,
    pub reason: String,
}

fn rule(id: &str, kind: RuleKind, pattern: &str, inside: bool, note: &str) -> Rule {
    Rule {
        id: id.to_string(),
        kind,
        pattern: pattern.to_string(),
        enabled: true,
        builtin: true,
        inside_game_only: inside,
        note: note.to_string(),
    }
}

impl RuleSet {
    /// The recommended defaults.
    pub fn recommended() -> RuleSet {
        use RuleKind::{DirName, FileName};
        let mut rules = vec![
            rule("d-redist", DirName, "_Redist", false, "Redistributable installers"),
            rule("d-commonredist", DirName, "_CommonRedist", false, "Steam-style shared redistributables"),
            rule("d-ue-redist", DirName, "Engine/Extras/Redist", false, "Unreal Engine prerequisites"),
            rule("d-redist2", DirName, "Redist", true, "Redistributables inside a game"),
            rule("d-directx", DirName, "DirectX", false, "DirectX installers"),
            rule("d-installers", DirName, "Installers", false, "Installer folders"),
            rule("d-installer", DirName, "__Installer", false, "Installer data"),
            rule("d-uninstall", DirName, "Uninstall", false, "Uninstaller folders"),
            rule("d-support", DirName, "Support", true, "Support tools inside a game"),
            rule("d-prereq", DirName, "Prerequisites", false, "Prerequisite installers"),
            rule("d-vcredist", DirName, "vcredist", false, "Visual C++ runtimes"),
            rule("d-eac", DirName, "EasyAntiCheat", false, "Anti-cheat service"),
            rule("d-battleye", DirName, "BattlEye", false, "Anti-cheat service"),
            rule("d-ue-thirdparty", DirName, "Engine/Binaries/ThirdParty", false, "Unreal Engine helper binaries"),
            rule("d-tools", DirName, "Tools", true, "Tools shipped inside a game"),
            rule("f-quicksfv", FileName, "QuickSFV.exe", false, "File verification utility"),
            rule("f-unins", FileName, "unins*.exe", false, "Inno Setup uninstaller"),
            rule("f-vcredist", FileName, "vc_redist*.exe", false, "Visual C++ runtime installer"),
            rule("f-dxsetup", FileName, "DXSETUP.exe", false, "DirectX installer"),
            rule("f-setup", FileName, "setup.exe", false, "Installer"),
            rule("f-langsel", FileName, "Language Selector.exe", false, "Language selection utility"),
        ];
        for category in Category::ALL {
            rules.push(Rule {
                id: format!("c-{}", category.key()),
                kind: RuleKind::Category,
                pattern: category.key().to_string(),
                enabled: true,
                builtin: true,
                inside_game_only: false,
                note: format!("Any {}", category.label()),
            });
        }
        RuleSet {
            rules,
            allowlist: Vec::new(),
        }
    }

    pub fn is_allowlisted(&self, exe: &str) -> bool {
        let n = norm_path(exe);
        self.allowlist.iter().any(|a| norm_path(a) == n)
    }

    /// Checks one executable. `game_dir` is the game folder it belongs to, when known;
    /// `inside_game_only` rules are skipped while it is unknown.
    pub fn check(
        &self,
        exe: &str,
        game_dir: Option<&str>,
        description: Option<&str>,
    ) -> Option<Exclusion> {
        if self.is_allowlisted(exe) {
            return None;
        }
        self.rules
            .iter()
            .filter(|r| r.enabled)
            .find_map(|r| rule_matches(r, exe, game_dir, description))
    }

    /// Only the rules that need the game folder (`inside_game_only`).
    pub fn check_inside_game(&self, exe: &str, game_dir: &str) -> Option<Exclusion> {
        if self.is_allowlisted(exe) {
            return None;
        }
        self.rules
            .iter()
            .filter(|r| r.enabled && r.inside_game_only)
            .find_map(|r| rule_matches(r, exe, Some(game_dir), None))
    }

    /// Only the rules that do not need the game folder.
    pub fn check_global(&self, exe: &str, description: Option<&str>) -> Option<Exclusion> {
        if self.is_allowlisted(exe) {
            return None;
        }
        self.rules
            .iter()
            .filter(|r| r.enabled && !r.inside_game_only)
            .find_map(|r| rule_matches(r, exe, None, description))
    }

    /// Fills in recommended rules a user's saved set is missing (new in an update) without
    /// touching rules the user edited or disabled.
    pub fn merge_new_builtins(&mut self) {
        for builtin in RuleSet::recommended().rules {
            if !self.rules.iter().any(|r| r.id == builtin.id) {
                self.rules.push(builtin);
            }
        }
    }
}

/// Validates a pattern before it is saved, so a typo never silently matches nothing.
pub fn validate(kind: RuleKind, pattern: &str) -> Result<(), String> {
    if pattern.trim().is_empty() {
        return Err("The pattern is empty.".into());
    }
    match kind {
        RuleKind::Regex => RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map(|_| ())
            .map_err(|e| format!("Not a valid regular expression: {e}")),
        RuleKind::Category => Category::from_key(pattern)
            .map(|_| ())
            .ok_or_else(|| format!("Unknown category \"{pattern}\".")),
        _ => Ok(()),
    }
}

fn rule_matches(
    rule: &Rule,
    exe: &str,
    game_dir: Option<&str>,
    description: Option<&str>,
) -> Option<Exclusion> {
    let path = norm_path(exe);
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
    let pattern = norm_path(&rule.pattern);

    let hit = match rule.kind {
        RuleKind::DirName => {
            let scope = if rule.inside_game_only {
                let game = norm_path(game_dir?);
                // Strictly inside the game folder: the game folder itself never matches.
                dir.strip_prefix(&game)?.trim_start_matches('/').to_string()
            } else {
                dir.to_string()
            };
            contains_component_sequence(&scope, &pattern)
        }
        RuleKind::DirPath => path.starts_with(&format!("{pattern}/")),
        RuleKind::FileName => wildcard(&pattern, file),
        RuleKind::Glob => {
            if pattern.contains('/') {
                glob(&pattern, &path)
            } else {
                wildcard(&pattern, file)
            }
        }
        RuleKind::Regex => RegexBuilder::new(&rule.pattern)
            .case_insensitive(true)
            .build()
            .is_ok_and(|re| re.is_match(&path)),
        RuleKind::Category => {
            let stem = file.strip_suffix(".exe").unwrap_or(file);
            let wanted = Category::from_key(&rule.pattern)?;
            categorize(stem, description) == Some(wanted)
        }
    };

    hit.then(|| Exclusion {
        rule_id: rule.id.clone(),
        reason: describe(rule),
    })
}

fn describe(rule: &Rule) -> String {
    match rule.kind {
        RuleKind::Category => {
            let label = Category::from_key(&rule.pattern).map_or("excluded category", |c| c.label());
            format!("Ignored: {label}")
        }
        RuleKind::DirName => format!("Ignored: inside \"{}\" folder", rule.pattern),
        RuleKind::DirPath => format!("Ignored: under {}", rule.pattern),
        _ if !rule.note.is_empty() => format!("Ignored: {}", rule.note.to_lowercase()),
        _ => format!("Ignored: matches \"{}\"", rule.pattern),
    }
}

/// Whether `/`-separated `needle` appears as consecutive whole components of `haystack`.
fn contains_component_sequence(haystack: &str, needle: &str) -> bool {
    let hay: Vec<&str> = haystack.split('/').filter(|c| !c.is_empty()).collect();
    let need: Vec<&str> = needle.split('/').filter(|c| !c.is_empty()).collect();
    !need.is_empty() && hay.windows(need.len()).any(|w| w == need.as_slice())
}

/// `*` and `?` on a single path component.
fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        match p.get(pi) {
            Some('?') => {
                pi += 1;
                ti += 1;
            }
            Some('*') => {
                star = Some(pi);
                mark = ti;
                pi += 1;
            }
            Some(c) if Some(c) == t.get(ti) => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                Some(s) => {
                    pi = s + 1;
                    mark += 1;
                    ti = mark;
                }
                None => return false,
            },
        }
    }
    p.get(pi..).is_some_and(|rest| rest.iter().all(|c| *c == '*'))
}

/// Glob over whole paths: `**` crosses folders, `*`/`?` stay within one component.
fn glob(pattern: &str, path: &str) -> bool {
    fn go(p: &[&str], t: &[&str]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some(&"**"), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some(pc), Some(tc)) => wildcard(pc, tc) && go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    let p: Vec<&str> = pattern.split('/').collect();
    let t: Vec<&str> = path.split('/').collect();
    go(&p, &t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rs() -> RuleSet {
        RuleSet::recommended()
    }

    #[test]
    fn excludes_redist_folders_anywhere() {
        let r = rs();
        assert!(r.check(r"D:\Games\A Way Out\_Redist\vcredist_x64.exe", None, None).is_some());
        assert!(r.check(r"D:\Games\X\_CommonRedist\DirectX\Jun2010\DXSETUP.exe", None, None).is_some());
        assert!(r
            .check(r"D:\Games\X\Engine\Extras\Redist\en-us\UE4PrereqSetup_x64.exe", None, None)
            .is_some());
    }

    #[test]
    fn excludes_named_utilities_case_insensitively() {
        let r = rs();
        assert!(r.check(r"D:\Games\A Way Out\QuickSFV.EXE", None, None).is_some());
        assert!(r.check(r"D:\Games\A Way Out\unins000.exe", None, None).is_some());
        assert!(r.check(r"D:\Games\Haze1\Language Selector.exe", None, None).is_some());
        assert!(r.check(r"D:\Games\Halo\haloce.exe", None, None).is_none());
    }

    #[test]
    fn allowlist_beats_rules() {
        let mut r = rs();
        let exe = r"D:\Games\Haze1\Language Selector.exe";
        r.allowlist.push(exe.to_string());
        assert!(r.check(exe, None, None).is_none());
        assert!(r.check("d:/games/haze1/language selector.EXE", None, None).is_none());
    }

    #[test]
    fn inside_game_rules_do_not_hide_a_game_named_like_the_folder() {
        let r = rs();
        // A game literally called "Tools" is its own game folder: kept.
        assert!(r.check_inside_game(r"D:\Games\Tools\Tools.exe", r"D:\Games\Tools").is_none());
        // A Tools folder inside another game: excluded.
        assert!(r
            .check_inside_game(r"D:\Games\Halo\Tools\editor.exe", r"D:\Games\Halo")
            .is_some());
    }

    #[test]
    fn categories_cover_common_utilities() {
        assert_eq!(categorize("unins001", None), Some(Category::Uninstaller));
        assert_eq!(categorize("QuickSFV", None), Some(Category::Verification));
        assert_eq!(categorize("Language Selector", None), Some(Category::LanguageSelector));
        assert_eq!(categorize("UnityCrashHandler64", None), Some(Category::CrashReporter));
        assert_eq!(categorize("vc_redist.x64", None), Some(Category::Redistributable));
        assert_eq!(categorize("AWayOut", None), None);
        assert_eq!(categorize("haloce", None), None);
        assert_eq!(categorize("CoDWaW", None), None);
        assert_eq!(categorize("game", Some("Microsoft Visual C++ Redistributable")), Some(Category::Redistributable));
    }

    #[test]
    fn glob_and_regex_rules() {
        let mut r = RuleSet::default();
        r.rules.push(Rule {
            id: "g".into(),
            kind: RuleKind::Glob,
            pattern: "d:/games/**/benchmark/*.exe".into(),
            enabled: true,
            builtin: false,
            inside_game_only: false,
            note: String::new(),
        });
        r.rules.push(Rule {
            id: "x".into(),
            kind: RuleKind::Regex,
            pattern: r"server\.exe$".into(),
            enabled: true,
            builtin: false,
            inside_game_only: false,
            note: String::new(),
        });
        assert!(r.check(r"D:\Games\Foo\Bin\Benchmark\run.exe", None, None).is_some());
        assert!(r.check(r"D:\Games\Foo\DedicatedServer.exe", None, None).is_some());
        assert!(r.check(r"D:\Games\Foo\Foo.exe", None, None).is_none());
    }

    #[test]
    fn disabled_rules_do_not_match() {
        let mut r = rs();
        for rule in &mut r.rules {
            rule.enabled = false;
        }
        assert!(r.check(r"D:\Games\X\QuickSFV.exe", None, None).is_none());
    }

    #[test]
    fn validates_patterns() {
        assert!(validate(RuleKind::Regex, "(").is_err());
        assert!(validate(RuleKind::Category, "nonsense").is_err());
        assert!(validate(RuleKind::Category, "language_selector").is_ok());
        assert!(validate(RuleKind::FileName, " ").is_err());
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("unins*.exe", "unins000.exe"));
        assert!(wildcard("vc_redist*.exe", "vc_redist.x64.exe"));
        assert!(!wildcard("unins*.exe", "game.exe"));
        assert!(wildcard("?ame.exe", "game.exe"));
    }
}
