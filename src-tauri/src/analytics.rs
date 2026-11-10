use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use crate::util::{command, rel};

// ---------------- git activity ----------------

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub date: String,
    pub commits: u32,
    pub adds: u64,
    pub dels: u64,
}

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub name: String,
    pub email: String,
    pub commits: u32,
    pub adds: u64,
    pub dels: u64,
    pub first: String,
    pub last: String,
}

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HotFile {
    pub path: String,
    pub commits: u32,
    pub adds: u64,
    pub dels: u64,
    pub last: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitAnalytics {
    pub is_repo: bool,
    pub total_commits: u32,
    pub total_adds: u64,
    pub total_dels: u64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub days: Vec<Day>,
    /// [weekday 0=Mon..6=Sun][hour 0..23] in the author's local time
    pub punch: Vec<Vec<u32>>,
    pub authors: Vec<Author>,
    pub hot_files: Vec<HotFile>,
    pub truncated: bool,
}

fn weekday_index(date: &str) -> Option<usize> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    use chrono::Datelike;
    Some(d.weekday().num_days_from_monday() as usize)
}

/// "src/{old => new}/a.ts" or "old => new" → the new path
fn rename_target(p: &str, re: &Regex) -> String {
    if let Some(c) = re.captures(p) {
        let whole = c.get(0).unwrap();
        let new = c.get(2).map(|m| m.as_str()).unwrap_or("");
        let s = format!("{}{}{}", &p[..whole.start()], new, &p[whole.end()..]);
        return s.replace("//", "/");
    }
    if let Some(idx) = p.find(" => ") {
        return p[idx + 4..].to_string();
    }
    p.to_string()
}

pub fn git_analytics(path: &str, max_commits: u32) -> Result<GitAnalytics, String> {
    let mut a = GitAnalytics {
        punch: vec![vec![0; 24]; 7],
        ..Default::default()
    };
    let top = command("git")
        .args(["-C", path, "rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| e.to_string())?;
    if !top.status.success() {
        return Ok(a);
    }
    let root = String::from_utf8_lossy(&top.stdout).trim().to_string();
    // limit to the project folder when it's a subfolder of a bigger repo
    let prefix = rel(&PathBuf::from(&root), &PathBuf::from(path));
    a.is_repo = true;

    let n = max_commits.to_string();
    let mut c = command("git");
    c.args(["-C", &root, "-c", "core.quotepath=false", "--no-pager", "log", "--no-merges", "--numstat", "-n", &n])
        .arg("--format=%x1e%aI%x1f%an%x1f%ae");
    if !prefix.is_empty() && !prefix.starts_with('/') && !prefix.contains(':') {
        c.args(["--", &prefix]);
    }
    let out = c.env("LC_ALL", "C.UTF-8").output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        // empty repo (no commits yet)
        return Ok(a);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let re_brace = Regex::new(r"\{([^{}]*) => ([^{}]*)\}").unwrap();

    let mut days: BTreeMap<String, Day> = BTreeMap::new();
    let mut authors: HashMap<String, Author> = HashMap::new();
    let mut files: HashMap<String, HotFile> = HashMap::new();

    for chunk in text.split('\u{1e}') {
        let chunk = chunk.trim_matches('\n');
        if chunk.is_empty() {
            continue;
        }
        let mut lines = chunk.lines();
        let head = lines.next().unwrap_or("");
        let f: Vec<&str> = head.split('\u{1f}').collect();
        if f.len() < 3 || f[0].len() < 13 {
            continue;
        }
        let iso = f[0];
        let date = &iso[..10];
        let hour: usize = iso[11..13].parse().unwrap_or(0);
        let (mut adds, mut dels) = (0u64, 0u64);
        let mut touched: Vec<(String, u64, u64)> = vec![];
        for l in lines {
            let p: Vec<&str> = l.splitn(3, '\t').collect();
            if p.len() < 3 {
                continue;
            }
            let ad: u64 = p[0].parse().unwrap_or(0);
            let de: u64 = p[1].parse().unwrap_or(0);
            adds += ad;
            dels += de;
            touched.push((rename_target(p[2], &re_brace), ad, de));
        }

        a.total_commits += 1;
        a.total_adds += adds;
        a.total_dels += dels;
        if a.last_date.is_none() {
            a.last_date = Some(iso.to_string());
        }
        a.first_date = Some(iso.to_string());

        let d = days.entry(date.to_string()).or_insert_with(|| Day { date: date.to_string(), ..Default::default() });
        d.commits += 1;
        d.adds += adds;
        d.dels += dels;

        if let Some(w) = weekday_index(date) {
            if hour < 24 {
                a.punch[w][hour] += 1;
            }
        }

        // same person often commits under several emails → merge by name
        let key = f[1].trim().to_lowercase();
        let au = authors.entry(key).or_insert_with(|| Author {
            name: f[1].to_string(),
            email: f[2].to_string(),
            last: iso.to_string(),
            ..Default::default()
        });
        au.commits += 1;
        au.adds += adds;
        au.dels += dels;
        au.first = iso.to_string();

        for (p, ad, de) in touched {
            let hf = files.entry(p.clone()).or_insert_with(|| HotFile { path: p, last: iso.to_string(), ..Default::default() });
            hf.commits += 1;
            hf.adds += ad;
            hf.dels += de;
        }
    }
    a.truncated = a.total_commits >= max_commits;
    a.days = days.into_values().collect();

    let mut au: Vec<Author> = authors.into_values().collect();
    au.sort_by(|x, y| y.commits.cmp(&x.commits));
    a.authors = au;

    let lock_re = Regex::new(r"(?i)(\.lock|lock\.json|lock\.yaml|\.lockb|\.min\.js|\.map|\.snap)$").unwrap();
    let mut hf: Vec<HotFile> = files
        .into_values()
        .filter(|f| !lock_re.is_match(&f.path))
        .collect();
    hf.sort_by(|x, y| y.commits.cmp(&x.commits).then(y.adds.cmp(&x.adds)));
    hf.truncate(25);
    a.hot_files = hf;
    Ok(a)
}

// ---------------- code statistics ----------------

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LangStat {
    pub lang: String,
    pub files: u32,
    pub lines: u64,
    pub code: u64,
    pub bytes: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BigFile {
    pub path: String,
    pub rel: String,
    pub lines: u64,
    pub lang: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodeStats {
    pub languages: Vec<LangStat>,
    pub total_files: u32,
    pub total_lines: u64,
    pub total_code: u64,
    pub biggest: Vec<BigFile>,
    pub truncated: bool,
}

pub(crate) fn lang_of(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if lower.ends_with(".min.js") || lower.ends_with(".d.ts") && lower.contains("generated") {
        return None;
    }
    if lower == "dockerfile" {
        return Some("Dockerfile");
    }
    if lower.ends_with(".blade.php") {
        return Some("Blade");
    }
    let ext = lower.rsplit('.').next()?;
    if ext == lower {
        return None;
    }
    Some(match ext {
        "ts" | "mts" | "cts" => "TypeScript",
        "tsx" => "TSX",
        "js" | "mjs" | "cjs" => "JavaScript",
        "jsx" => "JSX",
        "vue" => "Vue",
        "astro" => "Astro",
        "svelte" => "Svelte",
        "rs" => "Rust",
        "go" => "Go",
        "py" => "Python",
        "php" | "phtml" => "PHP",
        "twig" => "Twig",
        "cs" => "C#",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "dart" => "Dart",
        "swift" => "Swift",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" | "cxx" => "C++",
        "rb" => "Ruby",
        "lua" => "Lua",
        "css" => "CSS",
        "scss" | "sass" => "SCSS",
        "less" => "Less",
        "html" | "htm" => "HTML",
        "md" | "mdx" => "Markdown",
        "json" | "jsonc" => "JSON",
        "yml" | "yaml" => "YAML",
        "toml" => "TOML",
        "sql" => "SQL",
        "sh" | "bash" => "Shell",
        "ps1" => "PowerShell",
        "bat" | "cmd" => "Batch",
        "graphql" | "gql" => "GraphQL",
        "prisma" => "Prisma",
        "xml" => "XML",
        _ => return None,
    })
}

const SKIP: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".next", ".nuxt", ".output", ".astro", ".svelte-kit",
    ".turbo", ".cache", "coverage", "__pycache__", ".venv", "venv", ".vs", ".idea", "vendor", "out",
];

pub fn code_stats(root: &str) -> Result<CodeStats, String> {
    let root_p = PathBuf::from(root);
    let walker = ignore::WalkBuilder::new(&root_p)
        .hidden(false)
        .require_git(false)
        .parents(false)
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.depth() > 0 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) && SKIP.contains(&n.as_ref()))
        })
        .build();
    let mut langs: HashMap<&'static str, LangStat> = HashMap::new();
    let mut biggest: Vec<BigFile> = vec![];
    let mut s = CodeStats::default();
    for e in walker.flatten() {
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if name == "package-lock.json" || name.ends_with(".lock") {
            continue;
        }
        let Some(lang) = lang_of(&name) else { continue };
        let Ok(md) = e.metadata() else { continue };
        if md.len() > 2_000_000 {
            continue;
        }
        s.total_files += 1;
        if s.total_files > 50_000 {
            s.truncated = true;
            break;
        }
        let Ok(bytes) = fs::read(e.path()) else { continue };
        if bytes[..bytes.len().min(8000)].contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let mut lines = 0u64;
        let mut code = 0u64;
        for l in text.lines() {
            lines += 1;
            if !l.trim().is_empty() {
                code += 1;
            }
        }
        let st = langs.entry(lang).or_insert_with(|| LangStat { lang: lang.to_string(), ..Default::default() });
        st.files += 1;
        st.lines += lines;
        st.code += code;
        st.bytes += md.len();
        s.total_lines += lines;
        s.total_code += code;
        if !matches!(lang, "JSON" | "Markdown" | "YAML" | "XML") {
            biggest.push(BigFile {
                path: e.path().to_string_lossy().to_string(),
                rel: rel(&root_p, e.path()),
                lines,
                lang: lang.to_string(),
            });
        }
    }
    biggest.sort_by(|a, b| b.lines.cmp(&a.lines));
    biggest.truncate(15);
    let mut l: Vec<LangStat> = langs.into_values().collect();
    l.sort_by(|a, b| b.code.cmp(&a.code));
    s.languages = l;
    s.biggest = biggest;
    Ok(s)
}
