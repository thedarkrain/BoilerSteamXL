//! Recursive game discovery for user-configured folders.
//!
//! 1. Walk each root (never following symlinks or junctions, never running anything).
//! 2. Apply exclusion rules that don't need context (folders like `_Redist`, utilities).
//! 3. Group the remaining executables into game folders. A folder with no executables of
//!    its own whose sub-folders are separate games (`D:\Games\PLUG&PLAY`) is a collection;
//!    a folder whose executables sit in generic folders (`bin`, `Binaries\Win64`) is a game.
//! 4. Apply rules that only make sense inside a game folder.
//! 5. Score every candidate; the scoring is documented in `docs/SCORING.md` and every
//!    point awarded is listed in the candidate's `reasons`.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Instant, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    norm_name, norm_path,
    rules::RuleSet,
    title::{is_container_name, is_generic_exe, title_from_folder},
};

pub const DEFAULT_MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRoot {
    pub path: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_depth")]
    pub max_depth: usize,
}

fn yes() -> bool {
    true
}
fn default_depth() -> usize {
    DEFAULT_MAX_DEPTH
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub path: String,
    pub file_name: String,
    pub score: i32,
    pub size: u64,
    /// Every adjustment that produced `score`, e.g. "+35 name matches the game folder".
    pub reasons: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Arguments from reliable launcher metadata (GOG play tasks).
    #[serde(default)]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExcludedExe {
    pub path: String,
    pub rule_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredGame {
    pub game_dir: String,
    pub title: String,
    /// "GOG manifest", "Executable metadata" or "Folder name".
    pub title_source: String,
    pub root: String,
    /// Best first.
    pub candidates: Vec<Candidate>,
    pub excluded: Vec<ExcludedExe>,
    pub confidence: u8,
    pub uncertain: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanResult {
    pub games: Vec<DiscoveredGame>,
    /// Excluded executables that belong to no game (a stray `_CommonRedist` folder).
    pub orphan_excluded: Vec<ExcludedExe>,
    pub roots_scanned: Vec<String>,
    pub root_errors: Vec<(String, String)>,
    pub folders_scanned: usize,
    pub exes_found: usize,
    pub exes_excluded: usize,
    pub cancelled: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub phase: &'static str,
    pub folders_scanned: usize,
    pub exes_found: usize,
    pub current: String,
}

/// PE metadata is the slow part of a scan, so it is cached by path, size and mtime.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PeInfo {
    pub size: u64,
    pub mtime: u64,
    pub description: Option<String>,
    pub product: Option<String>,
    pub console: bool,
}

pub type PeCache = HashMap<String, PeInfo>;

struct Exe {
    path: PathBuf,
    size: u64,
    pe: PeInfo,
}

pub fn scan(
    roots: &[ScanRoot],
    rules: &RuleSet,
    cache: &mut PeCache,
    progress: &dyn Fn(ScanProgress),
    cancel: &AtomicBool,
) -> ScanResult {
    let started = Instant::now();
    let mut result = ScanResult::default();
    let mut seen_roots: Vec<(String, usize)> = Vec::new();

    for root in roots.iter().filter(|r| r.enabled && !r.path.trim().is_empty()) {
        let root_path = PathBuf::from(root.path.trim());
        let key = norm_path(&root.path);
        // Skip a root only when one already scanned reaches all of it.
        let covered = seen_roots.iter().any(|(s, depth)| {
            if *s == key {
                return *depth >= root.max_depth;
            }
            key.strip_prefix(&format!("{s}/"))
                .is_some_and(|rest| *depth >= rest.split('/').count() + root.max_depth)
        });
        if covered {
            continue;
        }
        if !root_path.is_dir() {
            result
                .root_errors
                .push((root.path.clone(), "Folder not found or not accessible".into()));
            continue;
        }
        seen_roots.push((key, root.max_depth));

        let exes = walk(&root_path, root.max_depth, cache, progress, cancel, &mut result);
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        result.exes_found += exes.len();
        result.roots_scanned.push(root.path.clone());
        group_and_score(&root_path, exes, rules, &mut result);
    }

    // Overlapping roots can find a game twice; keep one.
    let mut best: HashMap<String, usize> = HashMap::new();
    for (i, g) in result.games.iter().enumerate() {
        let key = norm_path(&g.game_dir);
        let keep = best.get(&key).and_then(|j| result.games.get(*j)).is_none_or(|other| g.candidates.len() > other.candidates.len());
        if keep {
            best.insert(key, i);
        }
    }
    let kept: std::collections::HashSet<usize> = best.into_values().collect();
    let mut index = 0;
    result.games.retain(|_| {
        index += 1;
        kept.contains(&(index - 1))
    });
    result
        .games
        .sort_by_cached_key(|g| (g.title.to_lowercase(), norm_path(&g.game_dir)));
    result.duration_ms = started.elapsed().as_millis();
    result
}

fn walk(
    root: &Path,
    max_depth: usize,
    cache: &mut PeCache,
    progress: &dyn Fn(ScanProgress),
    cancel: &AtomicBool,
    result: &mut ScanResult,
) -> Vec<Exe> {
    let mut exes = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        result.folders_scanned += 1;
        if result.folders_scanned % 50 == 0 {
            progress(ScanProgress {
                phase: "scanning",
                folders_scanned: result.folders_scanned,
                exes_found: result.exes_found + exes.len(),
                current: dir.to_string_lossy().to_string(),
            });
        }

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            // On Windows, junctions report as symlinks too: neither is followed.
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if file_type.is_dir() {
                if depth < max_depth && !skip_dir(&name, &path) {
                    stack.push((path, depth + 1));
                }
            } else if file_type.is_file() && has_exe_extension(&name) {
                let meta = entry.metadata().ok();
                let size = meta.as_ref().map_or(0, |m| m.len());
                let mtime = meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                let key = norm_path(&path.to_string_lossy());
                let pe = match cache.get(&key) {
                    Some(info) if info.size == size && info.mtime == mtime => info.clone(),
                    _ => {
                        let info = read_pe_info(&path, size, mtime);
                        cache.insert(key, info.clone());
                        info
                    }
                };
                exes.push(Exe { path, size, pe });
            }
        }
    }
    exes
}

fn skip_dir(name: &str, path: &Path) -> bool {
    let lower = name.to_lowercase();
    // C:\Windows is skipped; a game's own "Windows" build folder is not.
    let at_drive_root = path.parent().is_some_and(|p| p.parent().is_none());
    name.starts_with('.')
        || matches!(lower.as_str(), "steamapps" | "compatdata" | "$recycle.bin" | "system volume information")
        || (lower == "windows" && at_drive_root)
}

fn has_exe_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

/// Reads the version resource and subsystem from the PE headers. The file is only read.
fn read_pe_info(path: &Path, size: u64, mtime: u64) -> PeInfo {
    let mut info = PeInfo {
        size,
        mtime,
        ..PeInfo::default()
    };
    // Tiny files can't be real programs; huge reads are avoided by memory mapping.
    if size < 1024 {
        return info;
    }
    let Ok(map) = pelite::FileMap::open(path) else {
        return info;
    };
    let Ok(file) = pelite::PeFile::from_bytes(map.as_ref()) else {
        return info;
    };
    use pelite::{pe32::Pe as _, pe64::Pe as _, Wrap};
    info.console = match &file {
        Wrap::T32(f) => f.optional_header().Subsystem == 3,
        Wrap::T64(f) => f.optional_header().Subsystem == 3,
    };
    if let Ok(resources) = file.resources() {
        if let Ok(version) = resources.version_info() {
            if let Some(lang) = version.translation().first() {
                info.description = version
                    .value(*lang, "FileDescription")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                info.product = version
                    .value(*lang, "ProductName")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
            }
        }
    }
    info
}

#[derive(Default)]
struct Node {
    path: PathBuf,
    name: String,
    direct: Vec<usize>,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn all(&self, out: &mut Vec<usize>) {
        out.extend(&self.direct);
        for child in self.children.values() {
            child.all(out);
        }
    }
}

fn group_and_score(root: &Path, exes: Vec<Exe>, rules: &RuleSet, result: &mut ScanResult) {
    let mut kept = Vec::new();
    for exe in exes {
        let path = exe.path.to_string_lossy().to_string();
        match rules.check_global(&path, exe.pe.description.as_deref()) {
            Some(exclusion) => {
                result.exes_excluded += 1;
                result.orphan_excluded.push(ExcludedExe {
                    path,
                    rule_id: exclusion.rule_id,
                    reason: exclusion.reason,
                });
            }
            None => kept.push(exe),
        }
    }

    let mut tree = Node {
        path: root.to_path_buf(),
        name: root
            .file_name()
            .map_or_else(|| root.to_string_lossy().to_string(), |n| n.to_string_lossy().to_string()),
        ..Node::default()
    };
    for (index, exe) in kept.iter().enumerate() {
        let Ok(relative) = exe.path.strip_prefix(root) else {
            continue;
        };
        let mut node = &mut tree;
        let mut current = root.to_path_buf();
        let parent_components: Vec<_> = relative.parent().map(|p| p.components().collect()).unwrap_or_default();
        for component in parent_components {
            let name = component.as_os_str().to_string_lossy().to_string();
            current = current.join(&name);
            let path = current.clone();
            node = node.children.entry(name.to_lowercase()).or_insert_with(|| Node {
                path,
                name: name.clone(),
                ..Node::default()
            });
        }
        node.direct.push(index);
    }

    let mut groups = Vec::new();
    assign(&tree, true, &kept, &mut groups);
    // Deepest folders first, so an excluded file is shown with the game it is inside.
    groups.sort_by_key(|g| std::cmp::Reverse(g.0.components().count()));

    // Excluded executables are shown with the game they belong to.
    let mut orphans = std::mem::take(&mut result.orphan_excluded);
    for (game_dir, title, members, is_collection) in groups {
        let game_dir_str = game_dir.to_string_lossy().to_string();
        let game_key = format!("{}/", norm_path(&game_dir_str));
        let mut excluded: Vec<ExcludedExe> = Vec::new();
        let belongs = |path: &str| -> bool {
            let Some(rest) = norm_path(path).strip_prefix(&game_key).map(str::to_string) else { return false };
            if !is_collection {
                return true;
            }
            let folders: Vec<&str> = rest.split('/').collect();
            folders.iter().take(folders.len().saturating_sub(1)).all(|f| is_generic_dir(f))
        };
        orphans.retain(|o| {
            if belongs(&o.path) {
                excluded.push(o.clone());
                false
            } else {
                true
            }
        });

        let mut inside = Vec::new();
        for index in members {
            let Some(exe) = kept.get(index) else { continue };
            let path = exe.path.to_string_lossy().to_string();
            match rules.check_inside_game(&path, &game_dir_str) {
                Some(exclusion) => {
                    result.exes_excluded += 1;
                    excluded.push(ExcludedExe {
                        path,
                        rule_id: exclusion.rule_id,
                        reason: exclusion.reason,
                    });
                }
                None => inside.push(exe),
            }
        }
        if inside.is_empty() {
            orphans.extend(excluded);
            continue;
        }

        let (title, title_source, candidates) = score_group(&game_dir, &title, &inside);
        let (confidence, uncertain) = confidence(&candidates);
        excluded.sort_by(|a, b| a.path.cmp(&b.path));
        result.games.push(DiscoveredGame {
            game_dir: game_dir_str,
            title,
            title_source,
            root: root.to_string_lossy().to_string(),
            candidates,
            excluded,
            confidence,
            uncertain,
        });
    }
    result.orphan_excluded.extend(orphans);
}

/// Folder names that hold a game's binaries rather than being a game.
pub fn is_generic_dir(name: &str) -> bool {
    matches!(
        norm_name(name).as_str(),
        "bin" | "bin32" | "bin64" | "binaries" | "binary" | "win32" | "win64" | "x64" | "x86"
            | "x8664" | "amd64" | "game" | "data" | "engine" | "system" | "retail" | "shipping"
            | "release" | "exe" | "launcher" | "client" | "app" | "common" | "content"
            | "runtime" | "base" | "main" | "windows" | "windowsnoeditor" | "wingdk" | "win"
            | "build" | "final" | "dist" | "support" | "tools" | "redist" | "program"
            | "executable" | "x64vulkan" | "dx11" | "dx12" | "plugins" | "thirdparty" | "editor"
            | "server" | "dedicatedserver" | "mods" | "modtools" | "mopy" | "sdk" | "benchmark"
            | "32bit" | "64bit" | "opengl" | "vulkan" | "dx9" | "directx11" | "directx12"
            | "redistributables" | "extras" | "utilities" | "utils"
            // Crack and patch folders inside a repack are part of the game, not games.
            | "crack" | "nodvd" | "crackonly" | "fix" | "dlcunlocker" | "unlocker" | "trainer"
    )
}

/// Has executables of its own, or a binaries-style sub-folder.
fn looks_like_game(node: &Node) -> bool {
    !node.direct.is_empty() || node.children.values().any(|c| is_generic_dir(&c.name))
}

/// Has an executable of its own whose name matches the folder ("Oblivion\\Oblivion.exe").
fn has_own_game_exe(node: &Node, exes: &[Exe]) -> bool {
    node.direct
        .iter()
        .filter_map(|i| exes.get(*i))
        .any(|e| name_similarity(&file_stem(&e.path), &node.name).0 >= 25)
}

/// (game folder, folder name, executable indexes, is a collection's own group)
type Group = (PathBuf, String, Vec<usize>, bool);

fn assign(node: &Node, is_root: bool, exes: &[Exe], out: &mut Vec<Group>) {
    let game_children = node
        .children
        .values()
        .filter(|c| !is_generic_dir(&c.name) && looks_like_game(c))
        .count();
    // A collection: a known collection name ("PLUG&PLAY", "Games"), a folder holding two
    // or more games (even if it also has a launcher of its own), or a scan root that holds
    // a game or is not a game itself. Its own executables and generic sub-folders form one
    // group; every other sub-folder is examined on its own.
    let own_game = has_own_game_exe(node, exes);
    let collection = (!is_root && is_container_name(&node.name))
        || (game_children >= 2 && !own_game)
        || (is_root && !own_game && (game_children > 0 || !looks_like_game(node)));
    if collection {
        let mut own = node.direct.clone();
        for child in node.children.values().filter(|c| is_generic_dir(&c.name)) {
            child.all(&mut own);
        }
        if !own.is_empty() {
            out.push((node.path.clone(), node.name.clone(), own, true));
        }
        for child in node.children.values().filter(|c| !is_generic_dir(&c.name)) {
            assign(child, false, exes, out);
        }
        return;
    }

    let mut members = Vec::new();
    node.all(&mut members);
    if looks_like_game(node) || node.children.is_empty() {
        out.push((node.path.clone(), node.name.clone(), members, false));
        return;
    }

    if node.children.len() == 1 {
        // A wrapper folder ("Halo\Halo Combat Evolved") or a game whose files sit in one
        // named sub-folder: whichever name matches the executables better is the game.
        if let Some(child) = node.children.values().next() {
            let stems: Vec<String> = members
                .iter()
                .filter_map(|i| exes.get(*i))
                .map(|e| clean_stem(&file_stem(&e.path)))
                .collect();
            let best = |name: &str| stems.iter().map(|s| name_similarity(s, name).0).max().unwrap_or(0);
            if best(&node.name) >= best(&child.name) && best(&node.name) > 0 {
                out.push((node.path.clone(), node.name.clone(), members, false));
            } else {
                assign(child, false, exes, out);
            }
        }
        return;
    }

    for child in node.children.values() {
        assign(child, false, exes, out);
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Drops build suffixes so `AWayOut-Win64-Shipping` compares as `AWayOut`.
pub fn clean_stem(stem: &str) -> String {
    let mut s = norm_name(stem);
    for suffix in [
        "win64shipping", "win32shipping", "shipping", "win64", "win32", "x64", "x86", "dx11",
        "dx12", "vulkan", "64bit", "32bit", "steam", "gog", "eac", "be",
    ] {
        if s.len() > suffix.len() + 2 {
            if let Some(stripped) = s.strip_suffix(suffix) {
                s = stripped.to_string();
            }
        }
    }
    s
}

fn initials(title: &str) -> String {
    title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            // Numbers stay whole ("Death Stranding 2" → "ds2").
            if w.chars().all(|c| c.is_ascii_digit()) {
                w.to_string()
            } else {
                w.chars().next().map(|c| c.to_lowercase().to_string()).unwrap_or_default()
            }
        })
        .collect()
}

fn bigrams(s: &str) -> Vec<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    chars.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?))).collect()
}

fn dice(a: &str, b: &str) -> f64 {
    let (x, y) = (bigrams(a), bigrams(b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    let mut pool = y.clone();
    let mut hits = 0;
    for pair in &x {
        if let Some(pos) = pool.iter().position(|p| p == pair) {
            pool.swap_remove(pos);
            hits += 1;
        }
    }
    (2.0 * hits as f64) / ((x.len() + y.len()) as f64)
}

/// Points (0–35) for how well an executable stem matches a title, with the reason.
pub fn name_similarity(stem: &str, title: &str) -> (i32, &'static str) {
    let s = clean_stem(stem);
    let t = norm_name(title);
    if s.is_empty() || t.is_empty() {
        return (0, "");
    }
    if s == t {
        return (35, "name matches the game folder");
    }
    if s.len() >= 2 && s == initials(title) {
        return (28, "name is the game's initials");
    }
    if s.len() >= 3 && t.len() >= 3 && (t.contains(&s) || s.contains(&t)) {
        return (25, "name contains the game name");
    }
    let d = dice(&s, &t);
    if d >= 0.5 {
        return ((d * 25.0).round() as i32, "name resembles the game name");
    }
    (0, "")
}

#[derive(Deserialize)]
struct GogInfo {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "playTasks")]
    play_tasks: Vec<GogTask>,
}

#[derive(Deserialize)]
struct GogTask {
    #[serde(default, rename = "isPrimary")]
    is_primary: bool,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
}

/// GOG writes `goggame-<id>.info` with the primary play task: reliable launcher metadata.
fn gog_primary(game_dir: &Path) -> Option<(Option<String>, String, Option<String>)> {
    // One info file per product (DLCs included): use the first with a primary play task.
    for entry in std::fs::read_dir(game_dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !(name.starts_with("goggame-") && name.ends_with(".info")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
        let Ok(info) = serde_json::from_str::<GogInfo>(text.trim_start_matches('\u{feff}')) else { continue };
        let Some(task) = info.play_tasks.iter().find(|t| t.is_primary && t.kind.as_deref() != Some("URLTask")) else { continue };
        let Some(relative) = task.path.as_ref() else { continue };
        let path = game_dir.join(relative);
        return Some((info.name, path.to_string_lossy().to_string(), task.arguments.clone()));
    }
    None
}

fn score_group(game_dir: &Path, folder_name: &str, exes: &[&Exe]) -> (String, String, Vec<Candidate>) {
    let gog = gog_primary(game_dir);
    let folder_title = title_from_folder(folder_name);
    // Product metadata only refines the folder title's spelling; it never swaps in a
    // different edition or sequel ("Marvel vs. Capcom" stays as the folder says).
    let product_title = exes
        .iter()
        .filter_map(|e| e.pe.product.as_ref())
        .find(|p| norm_name(p) == norm_name(&folder_title))
        .cloned();
    let (title, title_source) = match (gog.as_ref().and_then(|g| g.0.clone()), product_title) {
        (Some(name), _) => (name, "GOG manifest"),
        (None, Some(product)) => (product, "Executable metadata"),
        (None, None) => (folder_title, "Folder name"),
    };
    let top_level_stems: Vec<String> = exes
        .iter()
        .filter(|e| e.path.parent() == Some(game_dir))
        .map(|e| clean_stem(&file_stem(&e.path)))
        .collect();

    let candidates_len = exes.len();
    let mut candidates: Vec<Candidate> = exes
        .iter()
        .map(|exe| {
            let stem = file_stem(&exe.path);
            let norm = norm_name(&stem);
            let mut score = 40;
            let mut reasons = vec!["40 base score".to_string()];
            let mut add = |points: i32, why: String| {
                if points != 0 {
                    score += points;
                    reasons.push(format!("{points:+} {why}"));
                }
            };

            let (points, why) = name_similarity(&stem, &title);
            let (folder_points, folder_why) = name_similarity(&stem, folder_name);
            if folder_points > points {
                add(folder_points, folder_why.to_string());
            } else if points > 0 {
                add(points, why.to_string());
            } else if is_generic_exe(&stem) {
                add(5, "generic game executable name; the title comes from the folder".into());
            } else {
                add(-5, "name unrelated to the game folder".into());
            }

            if let Some(text) = exe.pe.product.as_ref().or(exe.pe.description.as_ref()) {
                if name_similarity(text, &title).0 >= 25 {
                    add(15, format!("file description \"{text}\" matches"));
                }
            }

            let depth_parts: Vec<String> = exe
                .path
                .parent()
                .and_then(|p| p.strip_prefix(game_dir).ok())
                .map(|rel| rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect())
                .unwrap_or_default();
            if depth_parts.is_empty() {
                add(10, "in the game's main folder".into());
            } else {
                let penalty: i32 = depth_parts.iter().map(|p| if is_generic_dir(p) { -2 } else { -6 }).sum();
                add(penalty, format!("{} folder(s) deep ({})", depth_parts.len(), depth_parts.join("\\")));
            }

            let mb = exe.size as f64 / 1_048_576.0;
            let size_points = match exe.size {
                s if s >= 50 * 1_048_576 => 12,
                s if s >= 10 * 1_048_576 => 10,
                s if s >= 2 * 1_048_576 => 6,
                s if s >= 512 * 1024 => 2,
                s if s < 100 * 1024 => -8,
                _ => 0,
            };
            add(size_points, format!("file size {mb:.1} MB"));

            if let Some(parent) = exe.path.parent() {
                if parent.join(format!("{stem}_Data")).is_dir() {
                    add(15, format!("Unity player (has {stem}_Data)"));
                }
                if parent.join("steam_appid.txt").is_file() {
                    add(5, "next to steam_appid.txt".into());
                }
            }

            if let Some((_, gog_path, _)) = &gog {
                if norm_path(gog_path) == norm_path(&exe.path.to_string_lossy()) {
                    add(40, "GOG manifest names this as the game".into());
                }
            }

            if norm.ends_with("shipping") && !depth_parts.is_empty() && top_level_stems.contains(&clean_stem(&stem)) {
                add(-6, "Unreal binary; Steam should start the launcher in the main folder".into());
            }
            if norm.contains("launcher") && candidates_len > 1 {
                add(-8, "looks like a launcher".into());
            }
            if ["config", "settings", "options", "setting"].iter().any(|w| norm.contains(w)) {
                add(-20, "looks like a settings tool".into());
            }
            if norm.contains("server") || norm.contains("dedicated") {
                add(-15, "looks like a server".into());
            }
            if ["editor", "sdk", "modtool", "devkit", "worldbuilder", "creationkit", "toolkit", "modmanager"].iter().any(|w| norm.contains(w)) {
                add(-12, "looks like a modding/editor tool".into());
            }
            if norm.contains("benchmark") {
                add(-10, "looks like a benchmark".into());
            }
            // "Webhead[old].exe", "Game - Copy.exe", "game_backup.exe": a spare copy, not the game.
            let lower = stem.to_lowercase();
            if lower.split(|c: char| !c.is_alphanumeric()).any(|t| ["old", "backup", "bak", "orig", "copy"].contains(&t)) {
                add(-25, "looks like an old or backup copy".into());
            }
            if exe.pe.console {
                add(-15, "console program".into());
            }

            let arguments = gog
                .as_ref()
                .filter(|g| norm_path(&g.1) == norm_path(&exe.path.to_string_lossy()))
                .and_then(|g| g.2.clone());

            Candidate {
                path: exe.path.to_string_lossy().to_string(),
                file_name: exe
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                score,
                size: exe.size,
                reasons,
                description: exe.pe.description.clone(),
                arguments,
            }
        })
        .collect();

    candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| norm_path(&a.path).cmp(&norm_path(&b.path))));
    (title, title_source.to_string(), candidates)
}

/// Confidence is the best candidate's score clamped to 0–99. A detection is uncertain
/// when that score is under 65 or the runner-up is within 10 points.
pub fn confidence(candidates: &[Candidate]) -> (u8, bool) {
    let Some(best) = candidates.first() else {
        return (0, true);
    };
    let value = best.score.clamp(0, 99) as u8;
    let close = candidates.get(1).is_some_and(|second| second.score >= best.score - 10);
    (value, best.score < 65 || close)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_examples() {
        assert_eq!(name_similarity("AWayOut", "A Way Out").0, 35);
        assert_eq!(name_similarity("CoDWaW", "Call of Duty World at War").0, 28);
        assert_eq!(name_similarity("DS2", "Death Stranding 2").0, 28);
        assert_eq!(name_similarity("AWayOut-Win64-Shipping", "A Way Out").0, 35);
        assert_eq!(name_similarity("Haze", "Haze1").0, 25);
        assert_eq!(name_similarity("QuickSFV", "A Way Out").0, 0);
    }
}
