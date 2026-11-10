//! Checkpoints: snapshots of the whole working copy (tracked + untracked, respecting
//! .gitignore) stored as hidden commits under refs/devhangar/checkpoints/*.
//! They never touch the branch history, the index or the stash.

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::util::command;

const REF_PREFIX: &str = "refs/devhangar/checkpoints/";
/// Prefix used before the rename to Dev Hangar; such refs are moved on first list.
const OLD_REF_PREFIX: &str = "refs/devpm/checkpoints/";

fn migrate_old_refs(root: &str) {
    let Ok(out) = git(root, &["for-each-ref", "--format=%(refname) %(objectname)", OLD_REF_PREFIX.trim_end_matches('/')]) else {
        return;
    };
    for line in out.lines() {
        let Some((name, hash)) = line.trim().split_once(' ') else { continue };
        let Some(id) = name.strip_prefix(OLD_REF_PREFIX) else { continue };
        if git(root, &["update-ref", &format!("{}{}", REF_PREFIX, id), hash]).is_ok() {
            let _ = git(root, &["update-ref", "-d", name]);
        }
    }
}

fn git_env(repo: &str, args: &[&str], envs: &[(&str, String)]) -> Result<String, String> {
    let mut c = command("git");
    c.arg("-C")
        .arg(repo)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=false", "--no-pager"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C.UTF-8");
    for (k, v) in envs {
        c.env(k, v);
    }
    let out = c.output().map_err(|e| format!("git не найден: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    git_env(repo, args, &[])
}

fn git_ok(repo: &str, args: &[&str]) -> Option<String> {
    git(repo, args).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn repo_root(path: &str) -> Result<String, String> {
    git_ok(path, &["rev-parse", "--show-toplevel"]).ok_or_else(|| "Это не git-репозиторий".to_string())
}

/// Write the current working tree (incl. untracked, not ignored files) to a tree object
/// using a temporary index, so the real index is left untouched.
fn snapshot_tree(root: &str) -> Result<String, String> {
    let git_dir = git_ok(root, &["rev-parse", "--absolute-git-dir"]).ok_or("git dir не найден")?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp: PathBuf = std::env::temp_dir().join(format!("devhangar-index-{}-{}", std::process::id(), nanos));
    // start from a copy of the real index: keeps the stat cache → `add -A` is fast
    let real_index = Path::new(&git_dir).join("index");
    if real_index.exists() {
        let _ = std::fs::copy(&real_index, &tmp);
    }
    let env = [("GIT_INDEX_FILE", tmp.to_string_lossy().to_string())];
    let res = (|| {
        git_env(root, &["add", "-A", "--", "."], &env)?;
        let tree = git_env(root, &["write-tree"], &env)?;
        Ok::<String, String>(tree.trim().to_string())
    })();
    let _ = std::fs::remove_file(&tmp);
    res
}

fn identity_env(root: &str) -> Vec<(&'static str, String)> {
    let mut env = vec![];
    if git_ok(root, &["config", "user.name"]).is_none() {
        env.push(("GIT_AUTHOR_NAME", "Dev Hangar".to_string()));
        env.push(("GIT_COMMITTER_NAME", "Dev Hangar".to_string()));
    }
    if git_ok(root, &["config", "user.email"]).is_none() {
        env.push(("GIT_AUTHOR_EMAIL", "devhangar@localhost".to_string()));
        env.push(("GIT_COMMITTER_EMAIL", "devhangar@localhost".to_string()));
    }
    env
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub id: String,
    pub hash: String,
    pub date: String,
    pub message: String,
    pub branch: String,
}

pub fn create(path: &str, message: &str) -> Result<Checkpoint, String> {
    let root = repo_root(path)?;
    let tree = snapshot_tree(&root)?;
    let head = git_ok(&root, &["rev-parse", "--verify", "-q", "HEAD"]);
    let branch = git_ok(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "—".into());
    let msg = if message.trim().is_empty() { "Чекпоинт".to_string() } else { message.trim().to_string() };
    let full_msg = format!("{}\n\nbranch: {}", msg, branch);
    let mut args = vec!["commit-tree", tree.as_str(), "-m", full_msg.as_str()];
    if let Some(h) = head.as_deref() {
        args.push("-p");
        args.push(h);
    }
    let env = identity_env(&root);
    let hash = git_env(&root, &args, &env)?.trim().to_string();
    let id = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    git(&root, &["update-ref", &format!("{}{}", REF_PREFIX, id), &hash])?;
    Ok(Checkpoint {
        id,
        hash,
        date: chrono::Local::now().to_rfc3339(),
        message: msg,
        branch,
    })
}

pub fn list(path: &str) -> Result<Vec<Checkpoint>, String> {
    let root = match repo_root(path) {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    migrate_old_refs(&root);
    let out = git(
        &root,
        &[
            "for-each-ref",
            "--sort=-refname",
            "--format=%(refname)%1f%(objectname)%1f%(creatordate:iso-strict)%1f%(contents)%1e",
            REF_PREFIX.trim_end_matches('/'),
        ],
    )?;
    let mut v: Vec<Checkpoint> = out
        .split('\u{1e}')
        .filter(|s| !s.trim().is_empty())
        .filter_map(|rec| {
            let f: Vec<&str> = rec.trim_start_matches('\n').splitn(4, '\u{1f}').collect();
            if f.len() < 4 {
                return None;
            }
            let contents = f[3];
            let message = contents.lines().next().unwrap_or("").to_string();
            let branch = contents
                .lines()
                .find_map(|l| l.strip_prefix("branch: "))
                .unwrap_or("")
                .to_string();
            Some(Checkpoint {
                id: f[0].trim_start_matches(REF_PREFIX).to_string(),
                hash: f[1].to_string(),
                date: f[2].to_string(),
                message,
                branch,
            })
        })
        .collect();
    // ids are unix ms → sort numerically, newest first
    v.sort_by(|a, b| b.id.parse::<u128>().unwrap_or(0).cmp(&a.id.parse::<u128>().unwrap_or(0)));
    Ok(v)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointDiff {
    pub files: Vec<(String, String)>,
    pub patch: String,
}

/// Changes from the checkpoint to the current working copy.
pub fn diff(path: &str, hash: &str, file: Option<String>) -> Result<CheckpointDiff, String> {
    let root = repo_root(path)?;
    let cur = snapshot_tree(&root)?;
    let base = format!("{}^{{tree}}", hash);
    let ns = git(&root, &["diff-tree", "-r", "-M", "--name-status", &base, &cur])?;
    let files = ns
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split('\t').collect();
            if p.len() < 2 {
                return None;
            }
            Some((p[0].chars().next().unwrap_or('M').to_string(), p[p.len() - 1].to_string()))
        })
        .collect();
    let mut args = vec!["diff-tree", "-p", "-M", base.as_str(), cur.as_str()];
    let f;
    if let Some(ff) = file {
        f = ff;
        args.push("--");
        args.push(&f);
    }
    let patch = git(&root, &args)?;
    Ok(CheckpointDiff { files, patch })
}

/// Make the working copy identical to the checkpoint. A safety checkpoint of the
/// current state is created first, so a restore can itself be undone.
pub fn restore(path: &str, hash: &str) -> Result<Checkpoint, String> {
    let root = repo_root(path)?;
    let short: String = hash.chars().take(7).collect();
    let safety = create(&root, &format!("Автоматически: перед откатом к {}", short))?;
    let cur = format!("{}^{{tree}}", safety.hash);
    let base = format!("{}^{{tree}}", hash);
    // files that appeared after the checkpoint
    let added = git(&root, &["diff-tree", "-r", "--no-renames", "--name-only", "--diff-filter=A", &base, &cur])?;
    git(&root, &["restore", &format!("--source={}", hash), "--worktree", "--", "."])?;
    for rel in added.lines().filter(|l| !l.trim().is_empty()) {
        let p = Path::new(&root).join(rel);
        if p.is_file() {
            let _ = std::fs::remove_file(&p);
        }
    }
    Ok(safety)
}

pub fn delete(path: &str, id: &str) -> Result<(), String> {
    let root = repo_root(path)?;
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Некорректный id".into());
    }
    git(&root, &["update-ref", "-d", &format!("{}{}", REF_PREFIX, id)]).map(|_| ())
}

pub fn git_init(path: &str) -> Result<(), String> {
    git(path, &["init"]).map(|_| ())
}
