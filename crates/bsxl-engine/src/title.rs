//! Turns machine-style folder names into readable game titles.
//!
//! `marvel-vs-capcom` → "Marvel vs. Capcom", `the-witcher-3-wild-hunt` → "The Witcher 3:
//! Wild Hunt", `Elden.Ring.v1.10-FitGirl.Repack` → "Elden Ring". Pipeline: drop bracketed
//! tags, split on separators, drop known release/repack markers from the end, title-case
//! (only when the folder had no deliberate casing), then add a subtitle colon after a
//! known franchise or a sequel number.

use crate::norm_name;

/// Folders that hold games but are never a game themselves.
pub fn is_container_name(name: &str) -> bool {
    matches!(
        norm_name(name).as_str(),
        "games" | "game" | "pcgames" | "installedgames" | "mygames" | "plugplay" | "plugandplay"
            | "repacks" | "repack" | "fitgirl" | "fitgirlrepacks" | "dodi" | "torrents"
            | "downloads" | "steamlibrary" | "library" | "installed" | "programs" | "programfiles"
            | "programfilesx86" | "apps" | "emulators" | "portable" | "portablegames" | "nonsteam"
            | "nonsteamgames" | "standalone"
    )
}

/// Executable names that say nothing about which game they start.
pub fn is_generic_exe(stem: &str) -> bool {
    matches!(
        norm_name(stem).as_str(),
        "game" | "start" | "launcher" | "launch" | "play" | "run" | "main" | "client" | "app"
            | "gamelauncher" | "startgame" | "rungame" | "playgame" | "bin" | "win64" | "x64"
            | "gamewin64shipping" | "gamewin32shipping"
    )
}

/// Release-group and distribution markers. Only removed as trailing tokens, so a game
/// that happens to contain one of these words mid-title keeps it.
const RELEASE_TAGS: &[&str] = &[
    "fitgirl", "repack", "repacks", "dodi", "elamigos", "kaos", "xatab", "codex", "plaza",
    "skidrow", "reloaded", "cpy", "empress", "tenoke", "rune", "flt", "razor1911", "darksiders",
    "hoodlum", "prophet", "gog", "goty", "steamrip", "rg", "mechanics", "multi", "multi2",
    "multi3", "multi4", "multi5", "multi6", "multi7", "multi8", "multi9", "multi10", "multi11",
    "multi12", "multi13", "multi14", "multi15", "x64", "x86", "win64", "win32", "64bit",
    "32bit", "portable", "incl", "update", "updates", "dlc", "dlcs", "build", "proper",
    "unlocked", "crack", "cracked", "bonus", "ost", "selective", "download", "lossless",
    "eng", "rus",
];

const SMALL_WORDS: &[&str] = &[
    "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into", "nor", "of", "on",
    "or", "the", "to", "vs", "with",
];

/// Franchise names after which a subtitle colon goes ("Batman: Arkham City").
const FRANCHISES: &[&str] = &[
    "call of duty", "batman", "need for speed", "the witcher", "assassins creed",
    "star wars", "the elder scrolls", "grand theft auto", "final fantasy", "mortal kombat",
    "resident evil", "tom clancys", "far cry", "halo", "metal gear solid", "dragon age",
    "mass effect", "middle earth", "the lord of the rings", "lego", "sonic", "tomb raider",
    "dead rising", "devil may cry", "street fighter", "tekken", "dark souls", "gears of war",
    "hitman", "prince of persia", "spider man", "marvels spider man", "sniper elite",
    "the walking dead", "warhammer 40000", "warhammer", "crash bandicoot", "fallout",
    "borderlands", "bioshock", "dishonored", "wolfenstein", "doom", "max payne", "mafia",
];

fn is_version(token: &str) -> bool {
    let t = token.trim_start_matches(['v', 'V']);
    token.len() > 1
        && (token.starts_with(['v', 'V']) || t.contains('.'))
        && t.chars().next().is_some_and(|c| c.is_ascii_digit())
        && t.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn is_roman(token: &str) -> bool {
    let upper = token.to_uppercase();
    !upper.is_empty()
        && upper.len() <= 5
        && upper.chars().all(|c| matches!(c, 'I' | 'V' | 'X'))
        && matches!(
            upper.as_str(),
            "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII" | "IX" | "X" | "XI" | "XII" | "XIII"
        )
}

fn is_number(token: &str) -> bool {
    token.chars().all(|c| c.is_ascii_digit()) || is_roman(token)
}

/// Readable title from a folder name.
pub fn title_from_folder(folder: &str) -> String {
    // 1. Bracketed tags: "[FitGirl Repack]", "(x64)".
    let mut stripped = String::new();
    let mut depth = 0;
    for c in folder.chars() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => stripped.push(c),
            _ => {}
        }
    }

    // 2. Separators. Dots inside version numbers are kept until tags are dropped.
    let had_spaces = stripped.contains(' ');
    let mut tokens: Vec<String> = Vec::new();
    // Hyphens separate words in machine-style names; in spaced names they are part of the
    // title ("Spider-Man") unless they glue on a release tag ("Game-FitGirl").
    for raw in stripped.split(|c: char| c == ' ' || c == '_' || (c == '-' && !had_spaces)) {
        let piece = raw.trim();
        if piece.is_empty() || piece == "-" {
            continue;
        }
        // Dotted initials stay one word: "S.T.A.L.K.E.R.", "F.E.A.R.".
        let initials = piece.split('.').filter(|s| !s.is_empty()).collect::<Vec<_>>();
        if is_version(piece) || (initials.len() >= 2 && piece.contains('.') && initials.iter().all(|s| s.chars().count() == 1 && s.chars().all(char::is_alphabetic))) {
            tokens.push(piece.to_string());
        } else {
            for part in piece.split('.').filter(|s| !s.is_empty()) {
                let numeric = part.chars().all(|c| c.is_ascii_digit());
                match tokens.last_mut() {
                    Some(last)
                        if numeric
                            && !last.is_empty()
                            && (last.chars().all(|c| c.is_ascii_digit() || c == '.') || is_version(last))
                            && piece.contains(&format!("{last}.{part}")) =>
                    {
                        last.push('.');
                        last.push_str(part);
                    }
                    _ => tokens.push(part.to_string()),
                }
            }
        }
    }
    // " - " used as a separator in spaced names: "Game - FitGirl Repack".
    tokens.retain(|t| t != "-");
    // Dots split "v1.10" into "v1" "10": glue the digits back onto the version.
    let mut glued: Vec<String> = Vec::new();
    for t in tokens {
        match glued.last_mut() {
            Some(last) if is_version(last) && t.chars().all(|c| c.is_ascii_digit()) => {
                last.push('.');
                last.push_str(&t);
            }
            _ => glued.push(t),
        }
    }
    let mut tokens = glued;
    if had_spaces {
        let mut split = Vec::new();
        for t in tokens {
            if t.split('-').skip(1).any(|p| RELEASE_TAGS.contains(&norm_name(p).as_str())) {
                split.extend(t.split('-').map(str::to_string));
            } else {
                split.push(t);
            }
        }
        tokens = split;
    }

    // 3. Trailing release markers and versions ("v1.0.2", "Build 1234"). A bare "V2" or
    //    "2.0" is part of many titles ("Sniper Elite V2", "Tron 2.0"), so it is only
    //    removed when it is unmistakably a version or follows a removed release tag.
    while let Some(last) = tokens.last() {
        let n = norm_name(last);
        let is_build_number = n.chars().all(|c| c.is_ascii_digit())
            && tokens.len() >= 2
            && tokens.get(tokens.len() - 2).is_some_and(|p| norm_name(p) == "build");
        let dots = last.matches('.').count();
        let clear_version = is_version(last) && ((last.starts_with(['v', 'V']) && dots >= 1) || dots >= 2);
        if RELEASE_TAGS.contains(&n.as_str()) || n.is_empty() {
            tokens.pop();
        } else if clear_version || is_build_number {
            tokens.pop();
        } else {
            break;
        }
    }
    // Interior release markers that can never be part of a title.
    tokens.retain(|t| !matches!(norm_name(t).as_str(), "fitgirl" | "repack" | "dodi" | "elamigos"));
    if tokens.is_empty() {
        return folder.trim().to_string();
    }

    // 4. Casing: keep deliberate casing ("BioShock", "DOOM"), fix machine-style names.
    let joined: String = tokens.join("");
    let machine_style = joined == joined.to_lowercase() || (joined == joined.to_uppercase() && tokens.len() > 1);
    let mut words: Vec<String> = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if is_roman(t) {
                return t.to_uppercase();
            }
            if !machine_style {
                return t.clone();
            }
            let lower = t.to_lowercase();
            if i > 0 && SMALL_WORDS.contains(&lower.as_str()) {
                return lower;
            }
            let mut chars = lower.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect();
    for w in &mut words {
        if w.eq_ignore_ascii_case("vs") {
            *w = "vs.".to_string();
        }
    }

    // 5. Subtitle colon: after a known franchise (and its number), else after a sequel
    //    number followed by more words. Only for machine-style names, where the colon was
    //    lost to the separators.
    if machine_style && !words.iter().any(|w| w.contains(':')) {
        let lower: Vec<String> = words.iter().map(|w| norm_name(w)).collect();
        let mut colon_after: Option<usize> = None;
        for franchise in FRANCHISES {
            let parts: Vec<String> = franchise.split(' ').map(norm_name).collect();
            if lower.len() > parts.len() && lower.iter().take(parts.len()).eq(parts.iter()) {
                let mut at = parts.len() - 1;
                if words.get(at + 1).is_some_and(|w| is_number(w)) {
                    at += 1;
                }
                if at + 2 < words.len() {
                    colon_after = Some(at);
                }
                break;
            }
        }
        if colon_after.is_none() {
            // Only "Two Or More Words N Two Or More Words" ("Left 4 Dead 2" and
            // "Metro 2033 Redux" keep their numbers mid-title).
            colon_after = words
                .iter()
                .enumerate()
                .skip(2)
                .find(|(i, w)| {
                    is_number(w)
                        && i + 2 < words.len()
                        && !SMALL_WORDS.contains(&norm_name(words.get(i + 1).map_or("", |s| s.as_str())).as_str())
                })
                .map(|(i, _)| i);
        }
        if let Some(at) = colon_after {
            if let Some(word) = words.get_mut(at) {
                word.push(':');
            }
            // The word after a colon is capitalized even if it is a small word.
            if let Some(next) = words.get_mut(at + 1) {
                let mut chars = next.chars();
                if let Some(c) = chars.next() {
                    *next = c.to_uppercase().collect::<String>() + chars.as_str();
                }
            }
        }
    }

    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_style_folders_become_titles() {
        let cases = [
            ("marvel-vs-capcom", "Marvel vs. Capcom"),
            ("resident_evil_4", "Resident Evil 4"),
            ("the-witcher-3-wild-hunt", "The Witcher 3: Wild Hunt"),
            ("call.of.duty.world.at.war", "Call of Duty: World at War"),
            ("batman_arkham_city", "Batman: Arkham City"),
            ("need-for-speed-most-wanted", "Need for Speed: Most Wanted"),
            ("red_dead_redemption_2", "Red Dead Redemption 2"),
        ];
        for (folder, expected) in cases {
            assert_eq!(title_from_folder(folder), expected, "{folder}");
        }
    }

    #[test]
    fn repack_labels_are_removed() {
        assert_eq!(title_from_folder("Elden.Ring.v1.10-FitGirl.Repack"), "Elden Ring");
        assert_eq!(title_from_folder("Hollow Knight [FitGirl Repack]"), "Hollow Knight");
        assert_eq!(title_from_folder("S.T.A.L.K.E.R. Anomaly"), "S.T.A.L.K.E.R. Anomaly");
        assert_eq!(title_from_folder("F.E.A.R.-FitGirl.Repack"), "F.E.A.R.");
        assert_eq!(title_from_folder("Cyberpunk 2077 (v2.1 + DLC) [DODI Repack]"), "Cyberpunk 2077");
        assert_eq!(title_from_folder("Sekiro.Shadows.Die.Twice.MULTi13-ElAmigos"), "Sekiro Shadows Die Twice");
        assert_eq!(title_from_folder("Hades_Build_12345"), "Hades");
    }

    #[test]
    fn real_title_numbers_are_kept() {
        assert_eq!(title_from_folder("Sniper Elite V2"), "Sniper Elite V2");
        assert_eq!(title_from_folder("Tron 2.0"), "Tron 2.0");
        assert_eq!(title_from_folder("left_4_dead_2"), "Left 4 Dead 2");
        assert_eq!(title_from_folder("back_4_blood"), "Back 4 Blood");
        assert_eq!(title_from_folder("metro_2033_redux"), "Metro 2033 Redux");
        assert_eq!(title_from_folder("Game.v1.2"), "Game");
        assert_eq!(title_from_folder("Tron.2.0"), "Tron 2.0");
        assert_eq!(title_from_folder("Game.Name.1.0.2-CODEX"), "Game Name");
        assert_eq!(title_from_folder("Sniper.Elite.V2-ElAmigos"), "Sniper Elite V2");
        assert_eq!(title_from_folder("halo_infinite"), "Halo Infinite");
        assert_eq!(title_from_folder("halo_combat_evolved"), "Halo: Combat Evolved");
    }

    #[test]
    fn deliberate_casing_is_kept() {
        assert_eq!(title_from_folder("A Way Out"), "A Way Out");
        assert_eq!(title_from_folder("BioShock Infinite"), "BioShock Infinite");
        assert_eq!(title_from_folder("Haze1"), "Haze1");
        assert_eq!(title_from_folder("Final Fantasy VII"), "Final Fantasy VII");
    }

    #[test]
    fn containers_and_generic_exes() {
        assert!(is_container_name("PLUG&PLAY"));
        assert!(is_container_name("Games"));
        assert!(is_container_name("Installed Games"));
        assert!(!is_container_name("Marvel vs Capcom"));
        assert!(is_generic_exe("game"));
        assert!(is_generic_exe("Start"));
        assert!(!is_generic_exe("AWayOut"));
    }
}
