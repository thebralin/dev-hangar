use regex::Regex;
use serde::Serialize;
use std::path::Path;

use crate::util::command;

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    let out = command("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=false", "--no-pager"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C.UTF-8")
        .output()
        .map_err(|e| format!("git не найден: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn git_ok(repo: &str, args: &[&str]) -> Option<String> {
    git(repo, args).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub subject: String,
    pub refs: String,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub web_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    pub path: String,
    pub index: String,
    pub worktree: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSummary {
    pub is_repo: bool,
    pub git_available: bool,
    pub root: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub remotes: Vec<Remote>,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
    pub status: Vec<StatusEntry>,
    pub last_commit: Option<Commit>,
    pub first_commit_date: Option<String>,
    pub commit_count: u32,
    pub stash_count: u32,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
    /// merge | rebase | cherry-pick | revert in progress
    pub operation: Option<String>,
    pub remote_branches: Vec<String>,
    pub stashes: Vec<Stash>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    pub index: u32,
    pub message: String,
    pub date: String,
}

fn web_url(url: &str) -> Option<String> {
    // git@github.com:user/repo.git → https://github.com/user/repo
    let u = url.trim();
    let u = if let Some(rest) = u.strip_prefix("git@") {
        let rest = rest.replacen(':', "/", 1);
        format!("https://{}", rest)
    } else if let Some(rest) = u.strip_prefix("ssh://git@") {
        format!("https://{}", rest)
    } else if u.starts_with("http") {
        // strip credentials
        let re = Regex::new(r"https?://[^@/]+@").ok()?;
        re.replace(u, "https://").to_string()
    } else {
        return None;
    };
    Some(u.trim_end_matches(".git").to_string())
}

const LOG_FMT: &str = "--pretty=format:%x1e%H%x1f%h%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%D";

fn parse_log(out: &str) -> Vec<Commit> {
    let re_files = Regex::new(r"(\d+) files? changed").unwrap();
    let re_ins = Regex::new(r"(\d+) insertions?\(\+\)").unwrap();
    let re_del = Regex::new(r"(\d+) deletions?\(-\)").unwrap();
    out.split('\u{1e}')
        .filter(|c| !c.trim().is_empty())
        .filter_map(|chunk| {
            let mut lines = chunk.splitn(2, '\n');
            let head = lines.next()?;
            let rest = lines.next().unwrap_or("");
            let f: Vec<&str> = head.split('\u{1f}').collect();
            if f.len() < 7 {
                return None;
            }
            let cap = |re: &Regex| {
                re.captures(rest)
                    .and_then(|c| c.get(1))
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0)
            };
            Some(Commit {
                hash: f[0].into(),
                short: f[1].into(),
                author: f[2].into(),
                email: f[3].into(),
                date: f[4].into(),
                subject: f[5].into(),
                refs: f[6].trim().into(),
                files_changed: cap(&re_files),
                insertions: cap(&re_ins),
                deletions: cap(&re_del),
            })
        })
        .collect()
}

pub fn summary(path: &str) -> Result<GitSummary, String> {
    let mut s = GitSummary {
        is_repo: false,
        git_available: command("git").arg("--version").output().is_ok(),
        root: None,
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        remotes: vec![],
        branches: vec![],
        tags: vec![],
        status: vec![],
        last_commit: None,
        first_commit_date: None,
        commit_count: 0,
        stash_count: 0,
        user_name: None,
        user_email: None,
        operation: None,
        remote_branches: vec![],
        stashes: vec![],
    };
    if !s.git_available || !Path::new(path).is_dir() {
        return Ok(s);
    }
    if git_ok(path, &["rev-parse", "--is-inside-work-tree"]).as_deref() != Some("true") {
        return Ok(s);
    }
    s.is_repo = true;
    s.root = git_ok(path, &["rev-parse", "--show-toplevel"]);
    s.branch = git_ok(path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .or_else(|| git_ok(path, &["symbolic-ref", "--short", "HEAD"]));
    s.upstream = git_ok(path, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
    if s.upstream.is_some() {
        if let Some(c) = git_ok(path, &["rev-list", "--left-right", "--count", "HEAD...@{u}"]) {
            let p: Vec<u32> = c.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if p.len() == 2 {
                s.ahead = p[0];
                s.behind = p[1];
            }
        }
    }
    if let Some(r) = git_ok(path, &["remote", "-v"]) {
        for line in r.lines() {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() >= 2 && !s.remotes.iter().any(|x: &Remote| x.name == p[0]) {
                s.remotes.push(Remote {
                    name: p[0].into(),
                    url: p[1].into(),
                    web_url: web_url(p[1]),
                });
            }
        }
    }
    if let Some(b) = git_ok(path, &["branch", "--format=%(refname:short)"]) {
        s.branches = b.lines().map(String::from).collect();
    }
    if let Some(t) = git_ok(path, &["tag", "--sort=-creatordate"]) {
        s.tags = t.lines().take(20).map(String::from).collect();
    }
    if let Ok(st) = git(path, &["status", "--porcelain=v1", "-uall"]) {
        for line in st.lines() {
            if line.len() < 4 {
                continue;
            }
            let (x, y) = (&line[0..1], &line[1..2]);
            let mut p = line[3..].to_string();
            if let Some(idx) = p.find(" -> ") {
                p = p[idx + 4..].to_string();
            }
            s.status.push(StatusEntry {
                path: p.trim_matches('"').to_string(),
                index: x.into(),
                worktree: y.into(),
            });
        }
    }
    if let Ok(l) = git(path, &["log", "-n", "1", "--shortstat", LOG_FMT]) {
        s.last_commit = parse_log(&l).into_iter().next();
    }
    s.commit_count = git_ok(path, &["rev-list", "--count", "HEAD"])
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    s.first_commit_date = git_ok(path, &["log", "--reverse", "--format=%aI", "--max-parents=0"])
        .and_then(|d| d.lines().next().map(String::from));
    if let Some(b) = git_ok(path, &["branch", "-r", "--format=%(refname:short)"]) {
        // "origin" alone is the symbolic origin/HEAD
        s.remote_branches = b.lines().filter(|l| l.contains('/') && !l.ends_with("/HEAD")).map(String::from).collect();
    }
    if let Some(st) = git_ok(path, &["stash", "list", "--format=%gd%x1f%s%x1f%cI"]) {
        for line in st.lines() {
            let f: Vec<&str> = line.split('\u{1f}').collect();
            if f.len() < 3 {
                continue;
            }
            let index = f[0].trim_start_matches("stash@{").trim_end_matches('}').parse().unwrap_or(0);
            s.stashes.push(Stash { index, message: f[1].into(), date: f[2].into() });
        }
    }
    s.stash_count = s.stashes.len() as u32;
    s.operation = crate::gitops::operation(path);
    s.user_name = git_ok(path, &["config", "user.name"]);
    s.user_email = git_ok(path, &["config", "user.email"]);
    Ok(s)
}

pub fn log(path: &str, skip: u32, limit: u32, file: Option<String>) -> Result<Vec<Commit>, String> {
    let skip_s = skip.to_string();
    let limit_s = limit.to_string();
    let mut args = vec!["log", "--shortstat", LOG_FMT, "--skip", &skip_s, "-n", &limit_s];
    let f;
    if let Some(ff) = file {
        f = ff;
        args.push("--follow");
        args.push("--");
        args.push(&f);
    }
    Ok(parse_log(&git(path, &args)?))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub status: String,
    pub path: String,
    pub old_path: Option<String>,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub message: String,
    pub parents: Vec<String>,
    pub files: Vec<ChangedFile>,
}

pub fn commit_detail(path: &str, hash: &str) -> Result<CommitDetail, String> {
    let head = git(path, &["show", "-s", "--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%B", hash])?;
    let f: Vec<&str> = head.splitn(6, '\u{1f}').collect();
    if f.len() < 6 {
        return Err("Не удалось прочитать коммит".into());
    }
    let parents: Vec<String> = f[4].split_whitespace().map(String::from).collect();

    let ns = git(path, &["diff-tree", "-r", "--root", "-M", "--no-commit-id", "-m", "--first-parent", "--name-status", hash])?;
    let num = git(path, &["diff-tree", "-r", "--root", "-M", "--no-commit-id", "-m", "--first-parent", "--numstat", hash])
        .unwrap_or_default();
    let mut numstat = std::collections::HashMap::new();
    for l in num.lines() {
        let p: Vec<&str> = l.split('\t').collect();
        if p.len() >= 3 {
            let key = p[p.len() - 1].to_string();
            numstat.insert(key, (p[0].parse().ok(), p[1].parse().ok()));
        }
    }
    let mut files = vec![];
    for l in ns.lines() {
        let p: Vec<&str> = l.split('\t').collect();
        if p.len() < 2 {
            continue;
        }
        let status = p[0].chars().next().unwrap_or('M').to_string();
        let (path_, old) = if p.len() >= 3 {
            (p[2].to_string(), Some(p[1].to_string()))
        } else {
            (p[1].to_string(), None)
        };
        let ns = numstat
            .iter()
            .find(|(k, _)| k.as_str() == path_ || k.ends_with(&format!("=> {}", path_)) || k.ends_with(&path_))
            .map(|(_, v)| *v)
            .unwrap_or((None, None));
        files.push(ChangedFile {
            status,
            path: path_,
            old_path: old,
            additions: ns.0,
            deletions: ns.1,
        });
    }
    Ok(CommitDetail {
        hash: f[0].into(),
        author: f[1].into(),
        email: f[2].into(),
        date: f[3].into(),
        message: f[5].trim().into(),
        parents,
        files,
    })
}

/// Diff of a single file in a commit (or working tree when hash is None).
pub fn file_diff(path: &str, hash: Option<String>, file: &str) -> Result<String, String> {
    match hash {
        Some(h) => git(path, &["show", "--format=", "-m", "--first-parent", "-M", &h, "--", file]),
        None => {
            let d = git(path, &["diff", "HEAD", "--", file]).unwrap_or_default();
            if d.trim().is_empty() {
                // untracked file or no HEAD yet: diff against empty file
                let out = command("git")
                    .arg("-C")
                    .arg(path)
                    .args(["-c", "core.quotepath=false", "--no-pager", "diff", "--no-index", "--"])
                    .arg(if cfg!(windows) { "NUL" } else { "/dev/null" })
                    .arg(file)
                    .output()
                    .map_err(|e| e.to_string())?;
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            } else {
                Ok(d)
            }
        }
    }
}

/// Full working-tree diff (for the "changes" view)
pub fn working_diff(path: &str) -> Result<String, String> {
    git(path, &["diff", "HEAD"]).or_else(|_| git(path, &["diff"]))
}

pub fn file_at(path: &str, hash: &str, file: &str) -> Result<String, String> {
    let spec = format!("{}:{}", hash, file.replace('\\', "/"));
    git(path, &["show", &spec])
}
