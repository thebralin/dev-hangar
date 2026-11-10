use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::util::{is_heavy_dir, rel, to_millis};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
    pub heavy: bool,
    pub hidden: bool,
}

pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, String> {
    let rd = fs::read_dir(path).map_err(|e| e.to_string())?;
    let mut v: Vec<DirEntry> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name == ".git" {
                return None;
            }
            let md = e.metadata().ok()?;
            let is_dir = md.is_dir();
            Some(DirEntry {
                heavy: is_dir && is_heavy_dir(&name),
                hidden: name.starts_with('.'),
                name,
                path: e.path().to_string_lossy().to_string(),
                is_dir,
                size: if is_dir { 0 } else { md.len() },
                modified: md.modified().map(to_millis).unwrap_or(0),
            })
        })
        .collect();
    v.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(v)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub modified: i64,
    pub binary: bool,
    pub truncated: bool,
}

const MAX_READ: u64 = 3 * 1024 * 1024;

pub fn read_text(path: &str) -> Result<FileContent, String> {
    let p = Path::new(path);
    let md = fs::metadata(p).map_err(|e| e.to_string())?;
    let size = md.len();
    let mut f = fs::File::open(p).map_err(|e| e.to_string())?;
    let mut buf = Vec::with_capacity(size.min(MAX_READ) as usize);
    f.by_ref()
        .take(MAX_READ)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    let sniff = &buf[..buf.len().min(8000)];
    let binary = sniff.contains(&0u8);
    let content = if binary {
        String::new()
    } else {
        let s = String::from_utf8_lossy(&buf).to_string();
        s.trim_start_matches('\u{feff}').to_string()
    };
    Ok(FileContent {
        path: path.to_string(),
        content,
        size,
        modified: md.modified().map(to_millis).unwrap_or(0),
        binary,
        truncated: size > MAX_READ,
    })
}

const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "dist", ".next", ".nuxt", ".output", ".astro", ".svelte-kit",
    ".turbo", ".cache", "coverage", "__pycache__", ".venv", "venv", ".vs", ".idea",
];

fn walker(root: &Path, include_ignored: bool) -> ignore::Walk {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(!include_ignored)
        .git_exclude(!include_ignored)
        .ignore(!include_ignored)
        .parents(false)
        .require_git(false)
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.depth() > 0 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) && SKIP_DIRS.contains(&n.as_ref()))
        })
        .build()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    pub path: String,
    pub rel: String,
    pub score: i64,
    pub is_dir: bool,
}

/// Fuzzy file-name search that respects .gitignore.
pub fn search_files(root: &str, query: &str, limit: usize) -> Result<Vec<FileHit>, String> {
    let root_p = PathBuf::from(root);
    let q = query.to_lowercase().replace('\\', "/");
    let q = q.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let parts: Vec<&str> = q.split_whitespace().collect();
    let mut hits: Vec<FileHit> = vec![];
    let mut scanned = 0;
    for e in walker(&root_p, false).flatten() {
        if e.depth() == 0 {
            continue;
        }
        scanned += 1;
        if scanned > 150_000 {
            break;
        }
        let r = rel(&root_p, e.path());
        let rl = r.to_lowercase();
        let name = e.file_name().to_string_lossy().to_lowercase();
        let mut score: i64 = 0;
        let mut ok = true;
        for part in &parts {
            if name == *part {
                score += 1000;
            } else if name.starts_with(part) {
                score += 600;
            } else if name.contains(part) {
                score += 400;
            } else if rl.contains(part) {
                score += 150;
            } else if subsequence(&name, part) {
                score += 20;
            } else {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        score -= r.matches('/').count() as i64 * 5;
        score -= r.len() as i64 / 10;
        hits.push(FileHit {
            path: e.path().to_string_lossy().to_string(),
            rel: r,
            score,
            is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
        });
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score));
    hits.truncate(limit);
    Ok(hits)
}

fn subsequence(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextHit {
    pub path: String,
    pub rel: String,
    pub line: usize,
    pub text: String,
}

/// Simple full-text search (case-insensitive) that respects .gitignore.
pub fn search_text(root: &str, query: &str, limit: usize) -> Result<Vec<TextHit>, String> {
    let root_p = PathBuf::from(root);
    let q = query.to_lowercase();
    if q.trim().len() < 2 {
        return Ok(vec![]);
    }
    let mut hits = vec![];
    for e in walker(&root_p, false).flatten() {
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let md = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.len() > 1_500_000 {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.ends_with(".lock") || name.ends_with("lock.json") || name.ends_with(".min.js") || name.ends_with(".map") {
            continue;
        }
        let bytes = match fs::read(e.path()) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes[..bytes.len().min(8000)].contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&q) {
                let t: String = line.trim().chars().take(240).collect();
                hits.push(TextHit {
                    path: e.path().to_string_lossy().to_string(),
                    rel: rel(&root_p, e.path()),
                    line: i + 1,
                    text: t,
                });
                if hits.len() >= limit {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}
