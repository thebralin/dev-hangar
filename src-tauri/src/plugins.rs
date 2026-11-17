//! Plugins: user extensions that live in `<app config dir>/plugins/<id>/`.
//!
//! Rust only manages files (list / read / install / remove / scaffold);
//! loading and the plugin API live on the frontend (`src/plugins/`).

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::process::kill_pid;
use crate::util::shell;

pub const MANIFEST: &str = "plugin.json";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    /// folder name (normally equals manifest.id)
    pub folder: String,
    pub dir: String,
    pub manifest: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

pub fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = app.path().app_config_dir().map_err(|e| e.to_string())?.join("plugins");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

/// Plugin ids: lowercase latin letters, digits, `-`, `_`, `.`; 2–64 chars.
pub fn valid_id(id: &str) -> bool {
    let len_ok = (2..=64).contains(&id.len());
    let first_ok = id.chars().next().map(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).unwrap_or(false);
    len_ok
        && first_ok
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '.')
        && !id.contains("..")
}

fn read_manifest(dir: &Path) -> Result<serde_json::Value, String> {
    let s = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|_| format!("Нет файла {}", MANIFEST))?;
    let v: serde_json::Value = serde_json::from_str(s.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{}: ошибка JSON: {}", MANIFEST, e))?;
    if !v.is_object() {
        return Err(format!("{}: ожидается объект", MANIFEST));
    }
    Ok(v)
}

pub fn list(app: &AppHandle) -> Result<Vec<PluginEntry>, String> {
    let root = dir(app)?;
    let mut out = vec![];
    for e in std::fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let folder = e.file_name().to_string_lossy().to_string();
        if folder.starts_with('.') {
            continue;
        }
        let (manifest, error) = match read_manifest(&p) {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(e)),
        };
        out.push(PluginEntry { folder, dir: p.to_string_lossy().to_string(), manifest, error });
    }
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(out)
}

/// Resolve `rel` inside the plugin folder, refusing anything that escapes it.
fn resolve_inside(app: &AppHandle, folder: &str, rel: &str) -> Result<PathBuf, String> {
    if !valid_id(folder) {
        return Err(format!("Некорректный id плагина: {}", folder));
    }
    let base = dir(app)?.join(folder).canonicalize().map_err(|_| format!("Плагин не найден: {}", folder))?;
    let p = base.join(rel).canonicalize().map_err(|_| format!("Файл не найден: {}", rel))?;
    if !p.starts_with(&base) {
        return Err("Путь вне папки плагина".into());
    }
    Ok(p)
}

pub fn read_file(app: &AppHandle, folder: &str, rel: &str) -> Result<String, String> {
    let p = resolve_inside(app, folder, rel)?;
    std::fs::read_to_string(&p).map_err(|e| format!("{}: {}", rel, e))
}

pub fn file_path(app: &AppHandle, folder: &str, rel: &str) -> Result<String, String> {
    resolve_inside(app, folder, rel).map(|p| p.to_string_lossy().to_string())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name();
        let n = name.to_string_lossy();
        if n == "node_modules" || n == ".git" {
            continue;
        }
        let from = e.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| format!("{}: {}", from.display(), e))?;
        }
    }
    Ok(())
}

/// Copy a plugin folder (the one containing plugin.json) into the plugins dir.
/// An existing plugin with the same id is replaced (update).
pub fn install(app: &AppHandle, src: &str) -> Result<String, String> {
    let src = PathBuf::from(src);
    let m = read_manifest(&src)?;
    let id = m.get("id").and_then(|v| v.as_str()).ok_or("В plugin.json нет поля \"id\"")?.to_string();
    if !valid_id(&id) {
        return Err(format!("Некорректный id \"{}\": только a-z, 0-9, -, _, . (2–64 символа)", id));
    }
    let root = dir(app)?;
    let dst = root.join(&id);
    if let (Ok(a), Ok(b)) = (src.canonicalize(), dst.canonicalize()) {
        if a == b {
            return Ok(id); // already in place
        }
    }
    // copy to a temp folder first, then swap — a failed copy never destroys the installed version
    let tmp = root.join(format!(".installing-{}", id));
    let _ = std::fs::remove_dir_all(&tmp);
    copy_dir(&src, &tmp)?;
    if dst.exists() {
        std::fs::remove_dir_all(&dst).map_err(|e| format!("Не удалось заменить старую версию: {}", e))?;
    }
    std::fs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn uninstall(app: &AppHandle, folder: &str) -> Result<(), String> {
    if !valid_id(folder) {
        return Err(format!("Некорректный id плагина: {}", folder));
    }
    let p = dir(app)?.join(folder);
    if !p.is_dir() {
        return Err(format!("Плагин не найден: {}", folder));
    }
    std::fs::remove_dir_all(&p).map_err(|e| e.to_string())
}

/// Create a starter plugin from the built-in template.
pub fn scaffold(app: &AppHandle, id: &str, name: &str) -> Result<String, String> {
    if !valid_id(id) {
        return Err(format!("Некорректный id \"{}\": только a-z, 0-9, -, _, . (2–64 символа)", id));
    }
    let p = dir(app)?.join(id);
    if p.exists() {
        return Err(format!("Плагин \"{}\" уже существует", id));
    }
    std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({
        "id": id,
        "name": name,
        "version": "0.1.0",
        "description": "Мой первый плагин для Dev Hangar",
        "author": "",
        "apiVersion": 1,
        "main": "main.js",
        "permissions": ["projects:read", "fs:read"]
    });
    let files: [(&str, String); 4] = [
        (MANIFEST, serde_json::to_string_pretty(&manifest).unwrap()),
        ("main.js", TEMPLATE_MAIN.replace("{{NAME}}", name).replace("{{ID}}", id)),
        // typings for editor autocompletion (the same file the app is type-checked against)
        ("hangar.d.ts", include_str!("../../src/plugins/hangar.d.ts").to_string()),
        ("README.md", format!("# {}\n\nПлагин для Dev Hangar.\n\nДокументация по плагинам: https://github.com/thebralin/dev-hangar/tree/main/docs/plugins\n", name)),
    ];
    for (f, content) in files {
        std::fs::write(p.join(f), content).map_err(|e| e.to_string())?;
    }
    Ok(p.to_string_lossy().to_string())
}

const TEMPLATE_MAIN: &str = r##"// {{NAME}} — плагин Dev Hangar.
// Документация: docs/plugins/README.md в репозитории Dev Hangar.
// После правок нажмите «Перезагрузить» на экране «Плагины».

/** @param {import("./hangar").Hangar} hangar */
export function activate(hangar) {
  // Своя вкладка в каждом проекте
  hangar.tabs.register({
    id: "main",
    label: "{{NAME}}",
    icon: "sparkles",
    mount(el, { project }) {
      el.innerHTML = `
        <div style="padding: 20px">
          <h2 style="margin-top: 0">Привет из плагина {{NAME}}!</h2>
          <p class="muted">Проект: <b>${hangar.util.escapeHtml(project.name)}</b></p>
          <p class="muted mono">${hangar.util.escapeHtml(project.path)}</p>
          <button class="btn primary" data-act="count">Посчитать файлы в корне</button>
          <p data-out></p>
        </div>`;
      el.querySelector("[data-act=count]").addEventListener("click", async () => {
        const entries = await hangar.fs.listDir(project.path);
        el.querySelector("[data-out]").textContent =
          `Файлов: ${entries.filter((e) => !e.isDir).length}, папок: ${entries.filter((e) => e.isDir).length}`;
      });
      // вернуть функцию очистки (необязательно)
      return () => {};
    },
  });

  // Команда в меню «Все команды» проекта (toolbar: true — кнопка в шапке)
  hangar.actions.register({
    id: "hello",
    label: "Поздороваться",
    run(project) {
      hangar.ui.toast(`Привет, ${project.name}!`);
    },
  });

  // ─── Ещё возможности (раскомментируйте, чтобы попробовать) ───

  // Свой экран в боковой панели:
  // hangar.pages.register({
  //   id: "page",
  //   label: "{{NAME}}",
  //   mount(el) { el.innerHTML = `<div style="padding: 20px"><h1>Мой экран</h1></div>`; },
  // });

  // Поддержка нового стека (бейдж + кнопки Dev/Build):
  // hangar.stacks.register({
  //   id: "my-stack",
  //   label: "My Stack",
  //   color: "#a371f7",
  //   detect: (ctx) => ctx.has("my-stack.config"),
  //   actions: () => [
  //     { id: "dev", label: "dev", command: "my-tool serve", kind: "dev" },
  //     { id: "build", label: "build", command: "my-tool build", kind: "build" },
  //   ],
  // });
}

export function deactivate() {
  // освободите ресурсы (таймеры, подписки), если они не зарегистрированы через hangar.*
}
"##;

/// Run a shell command and capture its output (for plugins with the `shell` permission).
pub fn exec_capture(cwd: &str, cmdline: &str, timeout_ms: u64) -> Result<ExecResult, String> {
    let mut child = shell(cmdline)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Не удалось запустить: {}", e))?;
    let pid = child.id();
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let t_out = std::thread::spawn(move || {
        let mut b = vec![];
        let _ = so.read_to_end(&mut b);
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = vec![];
        let _ = se.read_to_end(&mut b);
        b
    });
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    let (code, timed_out) = match rx.recv_timeout(Duration::from_millis(timeout_ms.max(100))) {
        Ok(Ok(st)) => (st.code(), false),
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => {
            kill_pid(pid);
            (None, true)
        }
    };
    let stdout = String::from_utf8_lossy(&t_out.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&t_err.join().unwrap_or_default()).to_string();
    Ok(ExecResult { code, stdout, stderr, timed_out })
}

pub fn write_text(path: &str, content: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, content).map_err(|e| format!("{}: {}", path, e))
}
