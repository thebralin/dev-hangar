mod analytics;
mod checkpoints;
mod detect;
mod extra;
mod files;
mod git;
mod gitops;
mod graph;
mod php;
mod plugins;
mod process;
mod util;

use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use util::{command, shell};

type R<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

// ---------- project ----------

#[tauri::command]
async fn analyze_project(path: String) -> R<detect::ProjectInfo> {
    blocking(move || detect::analyze(&path)).await
}

#[tauri::command]
async fn project_stats(path: String) -> R<detect::ProjectStats> {
    blocking(move || detect::stats(&path)).await
}

// ---------- git ----------

#[tauri::command]
async fn git_summary(path: String) -> R<git::GitSummary> {
    blocking(move || git::summary(&path)).await
}

#[tauri::command]
async fn git_log(path: String, skip: u32, limit: u32, file: Option<String>) -> R<Vec<git::Commit>> {
    blocking(move || git::log(&path, skip, limit, file)).await
}

#[tauri::command]
async fn git_commit(path: String, hash: String) -> R<git::CommitDetail> {
    blocking(move || git::commit_detail(&path, &hash)).await
}

#[tauri::command]
async fn git_file_diff(path: String, hash: Option<String>, file: String) -> R<String> {
    blocking(move || git::file_diff(&path, hash, &file)).await
}

#[tauri::command]
async fn git_working_diff(path: String) -> R<String> {
    blocking(move || git::working_diff(&path)).await
}

#[tauri::command]
async fn git_file_at(path: String, hash: String, file: String) -> R<String> {
    blocking(move || git::file_at(&path, &hash, &file)).await
}

#[tauri::command]
async fn git_op(path: String, op: gitops::GitOp) -> R<String> {
    blocking(move || gitops::exec(&path, op)).await
}

// ---------- checkpoints ----------

#[tauri::command]
async fn checkpoint_create(path: String, message: String) -> R<checkpoints::Checkpoint> {
    blocking(move || checkpoints::create(&path, &message)).await
}

#[tauri::command]
async fn checkpoint_list(path: String) -> R<Vec<checkpoints::Checkpoint>> {
    blocking(move || checkpoints::list(&path)).await
}

#[tauri::command]
async fn checkpoint_diff(path: String, hash: String, file: Option<String>) -> R<checkpoints::CheckpointDiff> {
    blocking(move || checkpoints::diff(&path, &hash, file)).await
}

#[tauri::command]
async fn checkpoint_restore(path: String, hash: String) -> R<checkpoints::Checkpoint> {
    blocking(move || checkpoints::restore(&path, &hash)).await
}

#[tauri::command]
async fn checkpoint_delete(path: String, id: String) -> R<()> {
    blocking(move || checkpoints::delete(&path, &id)).await
}

#[tauri::command]
async fn git_init(path: String) -> R<()> {
    blocking(move || checkpoints::git_init(&path)).await
}

// ---------- extra ----------

#[tauri::command]
async fn scan_todos(paths: Vec<String>, limit: Option<usize>) -> R<Vec<extra::TodoHit>> {
    blocking(move || Ok(extra::scan_todos(paths, limit.unwrap_or(40)))).await
}

#[tauri::command]
async fn file_tree(root: String, max_depth: Option<usize>, max_entries: Option<usize>) -> R<String> {
    blocking(move || Ok(extra::file_tree(&root, max_depth.unwrap_or(4), max_entries.unwrap_or(250)))).await
}

#[tauri::command]
fn write_ai_file(root: String, name: String, content: String, overwrite: bool) -> R<String> {
    extra::write_ai_file(&root, &name, &content, overwrite)
}

#[tauri::command]
async fn scan_artifacts(root: String) -> R<Vec<extra::ArtifactDir>> {
    blocking(move || Ok(extra::scan_artifacts(&root))).await
}

#[tauri::command]
async fn delete_artifacts(roots: Vec<String>, paths: Vec<String>) -> R<Vec<extra::DeleteResult>> {
    blocking(move || Ok(extra::delete_artifacts(roots, paths))).await
}

// ---------- analytics ----------

#[tauri::command]
async fn git_analytics(path: String, max_commits: Option<u32>) -> R<analytics::GitAnalytics> {
    blocking(move || analytics::git_analytics(&path, max_commits.unwrap_or(20_000))).await
}

#[tauri::command]
async fn code_stats(root: String) -> R<analytics::CodeStats> {
    blocking(move || analytics::code_stats(&root)).await
}

#[tauri::command]
async fn code_graph(root: String) -> R<graph::CodeGraph> {
    blocking(move || graph::build(&root)).await
}

// ---------- files ----------

#[tauri::command]
async fn list_dir(path: String) -> R<Vec<files::DirEntry>> {
    blocking(move || files::list_dir(&path)).await
}

#[tauri::command]
async fn read_text_file(path: String) -> R<files::FileContent> {
    blocking(move || files::read_text(&path)).await
}

#[tauri::command]
async fn search_files(root: String, query: String, limit: Option<usize>) -> R<Vec<files::FileHit>> {
    blocking(move || files::search_files(&root, &query, limit.unwrap_or(200))).await
}

#[tauri::command]
async fn search_text(root: String, query: String, limit: Option<usize>) -> R<Vec<files::TextHit>> {
    blocking(move || files::search_text(&root, &query, limit.unwrap_or(500))).await
}

#[tauri::command]
fn path_exists(path: String) -> bool {
    PathBuf::from(path).exists()
}

#[tauri::command]
fn is_dir(path: String) -> bool {
    PathBuf::from(path).is_dir()
}

// ---------- processes ----------

#[tauri::command]
fn run_command(
    app: AppHandle,
    id: String,
    cwd: String,
    command: String,
    env: Option<HashMap<String, String>>,
) -> R<u32> {
    process::run(app, id, cwd, command, env.unwrap_or_default())
}

#[tauri::command]
fn kill_command(app: AppHandle, id: String) -> R<()> {
    process::kill(&app, &id)
}

#[tauri::command]
fn running_commands(app: AppHandle) -> Vec<String> {
    process::running(&app)
}

// ---------- external apps ----------

fn quote_exe(editor: &str) -> String {
    let e = editor.trim();
    if e.contains(' ') && !e.starts_with('"') && PathBuf::from(e).exists() {
        format!("\"{}\"", e)
    } else {
        e.to_string()
    }
}

#[tauri::command]
fn open_in_editor(editor: String, path: String, line: Option<u32>) -> R<()> {
    let exe = quote_exe(&editor);
    let lower = editor.to_lowercase();
    let vscode_like = ["code", "cursor", "windsurf", "codium", "antigravity", "trae", "kiro", "void"]
        .iter()
        .any(|n| lower.contains(n));
    let cmdline = match (line, vscode_like) {
        (Some(l), true) => format!("{} -g \"{}:{}\"", exe, path, l),
        _ => format!("{} \"{}\"", exe, path),
    };
    shell(&cmdline)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Не удалось открыть редактор: {}", e))
}

#[tauri::command]
fn open_terminal(path: String, terminal: Option<String>) -> R<()> {
    let term = terminal.unwrap_or_else(|| "wt".into());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let try_wt = || command("wt").arg("-d").arg(&path).spawn();
        let res = match term.as_str() {
            "wt" => try_wt().or_else(|_| {
                std::process::Command::new("cmd")
                    .creation_flags(CREATE_NEW_CONSOLE)
                    .current_dir(&path)
                    .spawn()
            }),
            "powershell" | "pwsh" => std::process::Command::new(if term == "pwsh" { "pwsh" } else { "powershell" })
                .arg("-NoExit")
                .creation_flags(CREATE_NEW_CONSOLE)
                .current_dir(&path)
                .spawn(),
            "git-bash" => {
                let candidates = [
                    r"C:\Program Files\Git\git-bash.exe",
                    r"C:\Program Files (x86)\Git\git-bash.exe",
                ];
                let exe = candidates.iter().find(|p| PathBuf::from(**p).exists()).copied().unwrap_or("git-bash.exe");
                std::process::Command::new(exe).arg(format!("--cd={}", path)).spawn()
            }
            _ => std::process::Command::new("cmd")
                .creation_flags(CREATE_NEW_CONSOLE)
                .current_dir(&path)
                .spawn(),
        };
        res.map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = term;
        for t in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
            if command(t).current_dir(&path).spawn().is_ok() {
                return Ok(());
            }
        }
        Err("Терминал не найден".into())
    }
}

#[tauri::command]
async fn detect_editors() -> R<Vec<String>> {
    blocking(|| {
        let names = [
            "code", "cursor", "windsurf", "antigravity", "zed", "trae", "kiro", "codium", "webstorm", "idea", "subl",
            "notepad++",
        ];
        let finder = if cfg!(windows) { "where" } else { "which" };
        Ok(names
            .iter()
            .filter(|n| {
                command(finder)
                    .arg(n)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            })
            .map(|s| s.to_string())
            .collect())
    })
    .await
}

// ---------- plugins ----------

#[tauri::command]
fn plugins_dir(app: AppHandle) -> R<String> {
    plugins::dir(&app).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn plugin_list(app: AppHandle) -> R<Vec<plugins::PluginEntry>> {
    plugins::list(&app)
}

#[tauri::command]
fn plugin_read_file(app: AppHandle, folder: String, rel: String) -> R<String> {
    plugins::read_file(&app, &folder, &rel)
}

#[tauri::command]
fn plugin_file_path(app: AppHandle, folder: String, rel: String) -> R<String> {
    plugins::file_path(&app, &folder, &rel)
}

#[tauri::command]
async fn plugin_install(app: AppHandle, src: String) -> R<String> {
    blocking(move || plugins::install(&app, &src)).await
}

#[tauri::command]
async fn plugin_uninstall(app: AppHandle, folder: String) -> R<()> {
    blocking(move || plugins::uninstall(&app, &folder)).await
}

#[tauri::command]
fn plugin_scaffold(app: AppHandle, id: String, name: String) -> R<String> {
    plugins::scaffold(&app, &id, &name)
}

#[tauri::command]
async fn exec_capture(cwd: String, command: String, timeout_ms: Option<u64>) -> R<plugins::ExecResult> {
    blocking(move || plugins::exec_capture(&cwd, &command, timeout_ms.unwrap_or(60_000))).await
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> R<()> {
    plugins::write_text(&path, &content)
}

// ---------- config ----------

fn config_path(app: &AppHandle) -> R<PathBuf> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join("config.json");
    // one-time migration from the old app id (Dev Projects Manager)
    if !p.exists() {
        if let Some(old) = dir.parent().map(|d| d.join("com.bolat.devprojectsmanager").join("config.json")) {
            if old.is_file() {
                let _ = std::fs::copy(&old, &p);
            }
        }
    }
    Ok(p)
}

#[tauri::command]
fn load_config(app: AppHandle) -> R<serde_json::Value> {
    let p = config_path(&app)?;
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
        Err(_) => Ok(serde_json::json!({})),
    }
}

#[tauri::command]
fn save_config(app: AppHandle, config: serde_json::Value) -> R<()> {
    let p = config_path(&app)?;
    let tmp = p.with_extension("json.tmp");
    let s = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

#[tauri::command]
fn config_location(app: AppHandle) -> R<String> {
    config_path(&app).map(|p| p.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(process::Procs::default())
        .invoke_handler(tauri::generate_handler![
            analyze_project,
            project_stats,
            git_summary,
            git_log,
            git_commit,
            git_file_diff,
            git_working_diff,
            git_file_at,
            git_op,
            list_dir,
            read_text_file,
            search_files,
            search_text,
            path_exists,
            is_dir,
            run_command,
            kill_command,
            running_commands,
            open_in_editor,
            open_terminal,
            detect_editors,
            load_config,
            save_config,
            config_location,
            checkpoint_create,
            checkpoint_list,
            checkpoint_diff,
            checkpoint_restore,
            checkpoint_delete,
            git_init,
            scan_todos,
            file_tree,
            write_ai_file,
            scan_artifacts,
            delete_artifacts,
            git_analytics,
            code_stats,
            code_graph,
            plugins_dir,
            plugin_list,
            plugin_read_file,
            plugin_file_path,
            plugin_install,
            plugin_uninstall,
            plugin_scaffold,
            exec_capture,
            write_text_file,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            process::kill_all(handle);
        }
    });
}

