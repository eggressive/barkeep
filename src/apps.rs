use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub description: Option<String>,
}

/// Strip freedesktop field codes (`%f`, `%u`, `%F`, `%U`, `%%`, ...) and trim.
/// .desktop Exec lines commonly look like `code %F` or `firefox %u`; those
/// codes must not reach the spawned command line.
pub fn clean_exec(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|tok| !(tok.starts_with('%') && tok.len() <= 2))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Enumerate installed applications from freedesktop .desktop entries.
pub async fn enumerate() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    use freedesktop_desktop_entry::{default_paths, get_languages_from_env, Iter};

    let locales = get_languages_from_env();
    for entry in Iter::new(default_paths()).entries(Some(&locales)) {
        if let (Some(name), Some(exec)) = (entry.name(&locales), entry.exec()) {
            apps.push(AppEntry {
                name: name.to_string(),
                exec: exec.to_string(),
                description: entry.comment(&locales).map(|c| c.to_string()),
            });
        }
    }
    apps
}

/// Fuzzy-rank apps against a query (best match first).
pub fn fuzzy_search(apps: &[AppEntry], query: &str) -> Vec<AppEntry> {
    if query.is_empty() {
        return apps.to_vec();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let atom = Atom::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );

    let mut results: Vec<(i32, &AppEntry)> = apps
        .iter()
        .filter_map(|app| {
            let mut buf = Vec::new();
            let haystack = Utf32Str::new(&app.name, &mut buf);
            atom.score(haystack, &mut matcher)
                .map(|score| (score as i32, app))
        })
        .collect();

    results.sort_by(|a, b| b.0.cmp(&a.0));
    results.into_iter().map(|(_, app)| app.clone()).collect()
}

/// Launch an app by its (cleaned) Exec line.
pub fn launch(exec: &str) -> std::io::Result<std::process::Child> {
    let cmd = clean_exec(exec);
    let mut parts = cmd.split_whitespace();
    let program = parts.next().unwrap_or_default();
    if program.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty exec",
        ));
    }
    std::process::Command::new(program).args(parts).spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_exec_strips_field_codes() {
        assert_eq!(clean_exec("code %F"), "code");
        assert_eq!(clean_exec("firefox %u"), "firefox");
        assert_eq!(clean_exec("code --flags %F"), "code --flags");
        assert_eq!(clean_exec("  gnome-terminal  "), "gnome-terminal");
    }

    #[test]
    fn clean_exec_keeps_percent_in_words() {
        // three-or-more-letter tokens starting with % are real arguments
        assert_eq!(clean_exec("foo %abc"), "foo %abc");
    }

    #[test]
    fn launch_rejects_empty() {
        assert!(launch("").is_err());
    }
}
