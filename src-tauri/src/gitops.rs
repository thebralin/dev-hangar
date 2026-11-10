//! Изменяющие git-операции: stage/commit, ветки, merge/rebase, pull/push, stash, теги, reset.
//! Перед необратимыми действиями (discard, reset --hard) создаётся чекпоинт.

use serde::Deserialize;
use std::process::Stdio;

use crate::util::command;

#[derive(Deserialize, Debug)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GitOp {
    /// empty paths = everything
    Stage { paths: Vec<String> },
    Unstage { paths: Vec<String> },
    Discard { paths: Vec<String> },
    Commit { message: String, amend: bool, all: bool },
    Fetch,
    Pull { rebase: bool },
    Push { force: bool },
    Checkout { branch: String },
    CreateBranch { name: String, from: Option<String>, checkout: bool },
    DeleteBranch { name: String, force: bool },
    RenameBranch { from: String, to: String },
    Merge { branch: String, no_ff: bool },
    Rebase { onto: String },
    /// abort / continue the merge, rebase, cherry-pick or revert in progress
    Abort,
    Continue,
    /// resolve a conflicted file by taking one side ("ours" | "theirs")
    Resolve { path: String, side: String },
    StashPush { message: Option<String>, include_untracked: bool },
    StashApply { index: u32, pop: bool },
    StashDrop { index: u32 },
    CreateTag { name: String, at: Option<String>, message: Option<String> },
    DeleteTag { name: String },
    PushTags,
    CherryPick { hash: String },
    Revert { hash: String },
    /// mode: soft | mixed | hard
    Reset { hash: String, mode: String },
    /// undo the last commit, keeping its changes staged
    UndoCommit,
    SetUser { name: String, email: String },
}

struct Out {
    ok: bool,
    text: String,
}

fn run(repo: &str, args: &[&str]) -> Out {
    let mut c = command("git");
    c.arg("-C")
        .arg(repo)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=false", "--no-pager"])
        .args(args)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        // never open an editor (merge --continue, rebase --continue, revert)
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .env("LC_ALL", "C.UTF-8");
    // ssh must not wait for a passphrase/host-key prompt nobody can see
    if std::env::var_os("GIT_SSH_COMMAND").is_none() && !has_ssh_config(repo) {
        c.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    match c.output() {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).trim_end().to_string();
            let err = String::from_utf8_lossy(&o.stderr).trim_end().to_string();
            if !err.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&err);
            }
            Out { ok: o.status.success(), text }
        }
        Err(e) => Out { ok: false, text: format!("git не найден: {e}") },
    }
}

fn has_ssh_config(repo: &str) -> bool {
    command("git")
        .args(["-C", repo, "config", "core.sshCommand"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    let o = run(repo, args);
    if o.ok {
        Ok(o.text)
    } else if o.text.is_empty() {
        Err(format!("git {} завершился с ошибкой", args.first().unwrap_or(&"")))
    } else {
        Err(o.text)
    }
}

fn with_paths<'a>(base: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    let mut v = base.to_vec();
    v.push("--");
    if paths.is_empty() {
        v.push(".");
    } else {
        v.extend(paths.iter().map(|s| s.as_str()));
    }
    v
}

fn has_head(repo: &str) -> bool {
    run(repo, &["rev-parse", "--verify", "-q", "HEAD"]).ok
}

fn current_branch(repo: &str) -> Option<String> {
    let o = run(repo, &["symbolic-ref", "--short", "-q", "HEAD"]);
    Some(o.text.trim().to_string()).filter(|s| o.ok && !s.is_empty())
}

fn checkpoint(repo: &str, what: &str) -> Result<(), String> {
    crate::checkpoints::create(repo, &format!("Авто: перед {what}"))
        .map(|_| ())
        .map_err(|e| format!("Не удалось создать страховочный чекпоинт, операция отменена: {e}"))
}

/// What is in progress: merge | rebase | cherry-pick | revert
pub fn operation(repo: &str) -> Option<String> {
    let dir = run(repo, &["rev-parse", "--absolute-git-dir"]);
    if !dir.ok {
        return None;
    }
    let d = std::path::PathBuf::from(dir.text.trim());
    if d.join("rebase-merge").is_dir() || d.join("rebase-apply").is_dir() {
        Some("rebase".into())
    } else if d.join("MERGE_HEAD").exists() {
        Some("merge".into())
    } else if d.join("CHERRY_PICK_HEAD").exists() {
        Some("cherry-pick".into())
    } else if d.join("REVERT_HEAD").exists() {
        Some("revert".into())
    } else {
        None
    }
}

fn validate_ref(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.starts_with('-') {
        return Err("Некорректное имя".into());
    }
    Ok(())
}

pub fn exec(repo: &str, op: GitOp) -> Result<String, String> {
    use GitOp::*;
    match op {
        Stage { paths } => git(repo, &with_paths(&["add", "-A"], &paths)),
        Unstage { paths } => {
            if has_head(repo) {
                git(repo, &with_paths(&["reset", "-q", "HEAD"], &paths))
            } else {
                git(repo, &with_paths(&["rm", "-r", "-q", "--cached"], &paths))
            }
        }
        Discard { paths } => {
            checkpoint(repo, "отменой изменений")?;
            // per-file: untracked → clean, added in index → rm, tracked → restore from HEAD
            let st = git(repo, &with_paths(&["status", "--porcelain=v1", "-z", "-uall"], &paths))?;
            let (mut untracked, mut added, mut tracked) = (vec![], vec![], vec![]);
            let mut it = st.split('\0').filter(|s| s.len() > 3);
            while let Some(e) = it.next() {
                let (xy, p) = e.split_at(3);
                let p = p.to_string();
                match &xy[..2] {
                    "??" => untracked.push(p),
                    x if x.starts_with('A') => added.push(p),
                    x => {
                        if x.starts_with('R') || x.starts_with('C') {
                            // rename: the source path follows; restore it too
                            if let Some(src) = it.next() {
                                tracked.push(src.to_string());
                            }
                            added.push(p);
                        } else {
                            tracked.push(p);
                        }
                    }
                }
            }
            let mut log = vec![];
            if !untracked.is_empty() {
                log.push(git(repo, &with_paths(&["clean", "-f", "-q"], &untracked))?);
            }
            if !added.is_empty() {
                log.push(git(repo, &with_paths(&["rm", "-f", "-q", "-r"], &added))?);
            }
            if !tracked.is_empty() {
                log.push(git(repo, &with_paths(&["checkout", "HEAD"], &tracked))?);
            }
            Ok(log.join("\n"))
        }
        Commit { message, amend, all } => {
            if message.trim().is_empty() && !amend {
                return Err("Пустое сообщение коммита".into());
            }
            if all {
                git(repo, &["add", "-A"])?;
            }
            let mut args = vec!["commit"];
            if amend {
                args.push("--amend");
            }
            if message.trim().is_empty() {
                args.push("--no-edit");
            } else {
                args.extend(["-m", message.as_str()]);
            }
            git(repo, &args)
        }
        Fetch => git(repo, &["fetch", "--all", "--prune"]),
        Pull { rebase } => git(repo, &["pull", if rebase { "--rebase" } else { "--no-rebase" }]),
        Push { force } => {
            let mut args: Vec<String> = vec!["push".into()];
            if force {
                args.push("--force-with-lease".into());
            }
            // first push of a branch: set upstream on origin (or the only remote)
            if !run(repo, &["rev-parse", "--abbrev-ref", "@{u}"]).ok {
                let branch = current_branch(repo).ok_or("Нет текущей ветки (detached HEAD)")?;
                let remotes = git(repo, &["remote"])?;
                let remote = remotes
                    .lines()
                    .find(|r| *r == "origin")
                    .or_else(|| remotes.lines().next())
                    .ok_or("Нет remote — добавьте его: git remote add origin <url>")?
                    .to_string();
                args.extend(["-u".into(), remote, branch]);
            }
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            git(repo, &a)
        }
        Checkout { branch } => {
            validate_ref(&branch)?;
            // "origin/feature" without a local branch → create a tracking branch
            let local = run(repo, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")]).ok;
            let remote = !local && run(repo, &["rev-parse", "--verify", "-q", &format!("refs/remotes/{branch}")]).ok;
            if remote {
                git(repo, &["checkout", "--track", &branch])
            } else {
                git(repo, &["checkout", &branch])
            }
        }
        CreateBranch { name, from, checkout } => {
            validate_ref(&name)?;
            let mut args: Vec<&str> = if checkout { vec!["checkout", "-b", &name] } else { vec!["branch", &name] };
            if let Some(f) = from.as_deref().filter(|f| !f.is_empty()) {
                args.push(f);
            }
            git(repo, &args)
        }
        DeleteBranch { name, force } => {
            validate_ref(&name)?;
            git(repo, &["branch", if force { "-D" } else { "-d" }, &name])
        }
        RenameBranch { from, to } => {
            validate_ref(&from)?;
            validate_ref(&to)?;
            git(repo, &["branch", "-m", &from, &to])
        }
        Merge { branch, no_ff } => {
            validate_ref(&branch)?;
            let mut args = vec!["merge", "--no-edit"];
            if no_ff {
                args.push("--no-ff");
            }
            args.push(&branch);
            git(repo, &args)
        }
        Rebase { onto } => {
            validate_ref(&onto)?;
            git(repo, &["rebase", &onto])
        }
        Abort => match operation(repo).as_deref() {
            Some(op) => git(repo, &[op, "--abort"]),
            None => Err("Нет незавершённой операции".into()),
        },
        Continue => match operation(repo).as_deref() {
            Some("merge") => git(repo, &["commit", "--no-edit"]),
            Some(op) => git(repo, &[op, "--continue"]),
            None => Err("Нет незавершённой операции".into()),
        },
        Resolve { path, side } => {
            let flag = match side.as_str() {
                "ours" => "--ours",
                "theirs" => "--theirs",
                _ => return Err("side: ours | theirs".into()),
            };
            git(repo, &["checkout", flag, "--", &path])?;
            git(repo, &["add", "--", &path])
        }
        StashPush { message, include_untracked } => {
            let mut args = vec!["stash", "push"];
            if include_untracked {
                args.push("-u");
            }
            if let Some(m) = message.as_deref().filter(|m| !m.trim().is_empty()) {
                args.extend(["-m", m]);
            }
            git(repo, &args)
        }
        StashApply { index, pop } => git(repo, &["stash", if pop { "pop" } else { "apply" }, &format!("stash@{{{index}}}")]),
        StashDrop { index } => git(repo, &["stash", "drop", &format!("stash@{{{index}}}")]),
        CreateTag { name, at, message } => {
            validate_ref(&name)?;
            let mut args = vec!["tag"];
            if let Some(m) = message.as_deref().filter(|m| !m.trim().is_empty()) {
                args.extend(["-a", "-m", m]);
            }
            args.push(&name);
            if let Some(a) = at.as_deref().filter(|a| !a.is_empty()) {
                args.push(a);
            }
            git(repo, &args)
        }
        DeleteTag { name } => {
            validate_ref(&name)?;
            git(repo, &["tag", "-d", &name])
        }
        PushTags => git(repo, &["push", "--tags"]),
        CherryPick { hash } => {
            validate_ref(&hash)?;
            git(repo, &["cherry-pick", &hash])
        }
        Revert { hash } => {
            validate_ref(&hash)?;
            git(repo, &["revert", "--no-edit", &hash])
        }
        Reset { hash, mode } => {
            validate_ref(&hash)?;
            let flag = match mode.as_str() {
                "soft" => "--soft",
                "mixed" => "--mixed",
                "hard" => {
                    checkpoint(repo, "reset --hard")?;
                    "--hard"
                }
                _ => return Err("mode: soft | mixed | hard".into()),
            };
            git(repo, &["reset", flag, &hash])
        }
        UndoCommit => {
            let parent = run(repo, &["rev-parse", "--verify", "-q", "HEAD~1"]);
            if !parent.ok {
                return Err("Это первый коммит — отменять некуда".into());
            }
            git(repo, &["reset", "--soft", "HEAD~1"])
        }
        SetUser { name, email } => {
            git(repo, &["config", "user.name", name.trim()])?;
            git(repo, &["config", "user.email", email.trim()])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo(name: &str) -> String {
        let root = std::env::temp_dir().join(format!("devhangar-gitops-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let r = root.to_string_lossy().to_string();
        git(&r, &["init", "-q", "-b", "main"]).unwrap();
        git(&r, &["config", "user.name", "T"]).unwrap();
        git(&r, &["config", "user.email", "t@t"]).unwrap();
        r
    }

    fn op(r: &str, json: &str) -> Result<String, String> {
        exec(r, serde_json::from_str(json).unwrap())
    }

    #[test]
    fn commit_branch_merge_discard() {
        let r = repo("flow");
        let p = std::path::Path::new(&r);
        fs::write(p.join("a.txt"), "1\n").unwrap();
        op(&r, r#"{"op":"commit","message":"init","amend":false,"all":true}"#).unwrap();

        op(&r, r#"{"op":"createBranch","name":"feat","from":null,"checkout":true}"#).unwrap();
        fs::write(p.join("b.txt"), "b\n").unwrap();
        op(&r, r#"{"op":"stage","paths":["b.txt"]}"#).unwrap();
        op(&r, r#"{"op":"commit","message":"feat b","amend":false,"all":false}"#).unwrap();
        op(&r, r#"{"op":"checkout","branch":"main"}"#).unwrap();
        assert!(!p.join("b.txt").exists());
        op(&r, r#"{"op":"merge","branch":"feat","noFf":true}"#).unwrap();
        assert!(p.join("b.txt").exists());

        // conflict → abort
        op(&r, r#"{"op":"checkout","branch":"feat"}"#).unwrap();
        fs::write(p.join("a.txt"), "feat\n").unwrap();
        op(&r, r#"{"op":"commit","message":"feat a","amend":false,"all":true}"#).unwrap();
        op(&r, r#"{"op":"checkout","branch":"main"}"#).unwrap();
        fs::write(p.join("a.txt"), "main\n").unwrap();
        op(&r, r#"{"op":"commit","message":"main a","amend":false,"all":true}"#).unwrap();
        assert!(op(&r, r#"{"op":"merge","branch":"feat","noFf":false}"#).is_err());
        assert_eq!(operation(&r).as_deref(), Some("merge"));
        op(&r, r#"{"op":"resolve","path":"a.txt","side":"theirs"}"#).unwrap();
        op(&r, r#"{"op":"continue"}"#).unwrap();
        assert_eq!(operation(&r), None);
        assert_eq!(fs::read_to_string(p.join("a.txt")).unwrap().trim(), "feat");

        // discard: modified + untracked + staged-new, with a safety checkpoint
        fs::write(p.join("a.txt"), "dirty\n").unwrap();
        fs::write(p.join("new.txt"), "n\n").unwrap();
        fs::write(p.join("staged.txt"), "s\n").unwrap();
        op(&r, r#"{"op":"stage","paths":["staged.txt"]}"#).unwrap();
        op(&r, r#"{"op":"discard","paths":[]}"#).unwrap();
        assert_eq!(fs::read_to_string(p.join("a.txt")).unwrap().trim(), "feat");
        assert!(!p.join("new.txt").exists() && !p.join("staged.txt").exists());
        assert!(!crate::checkpoints::list(&r).unwrap().is_empty());

        // stash round-trip, undo commit
        fs::write(p.join("a.txt"), "stashed\n").unwrap();
        op(&r, r#"{"op":"stashPush","message":"wip","includeUntracked":true}"#).unwrap();
        assert_eq!(fs::read_to_string(p.join("a.txt")).unwrap().trim(), "feat");
        op(&r, r#"{"op":"stashApply","index":0,"pop":true}"#).unwrap();
        assert_eq!(fs::read_to_string(p.join("a.txt")).unwrap().trim(), "stashed");
        op(&r, r#"{"op":"commit","message":"oops","amend":false,"all":true}"#).unwrap();
        op(&r, r#"{"op":"undoCommit"}"#).unwrap();
        assert!(git(&r, &["diff", "--cached", "--name-only"]).unwrap().contains("a.txt"));

        assert!(op(&r, r#"{"op":"push","force":false}"#).unwrap_err().contains("remote"));
        let _ = fs::remove_dir_all(&r);
    }
}
