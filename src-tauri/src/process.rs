use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::util::{command, shell};

#[derive(Default)]
pub struct Procs(pub Mutex<HashMap<String, u32>>);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OutputEvent {
    id: String,
    stream: String,
    data: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ExitEvent {
    id: String,
    code: Option<i32>,
}

fn pump<R: Read + Send + 'static>(app: AppHandle, id: String, stream: &'static str, r: R) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(r);
        let mut buf = Vec::with_capacity(4096);
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let data = String::from_utf8_lossy(&buf).to_string();
                    let _ = app.emit(
                        "proc://output",
                        OutputEvent {
                            id: id.clone(),
                            stream: stream.into(),
                            data,
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });
}

pub fn run(app: AppHandle, id: String, cwd: String, cmdline: String, env: HashMap<String, String>) -> Result<u32, String> {
    {
        let procs = app.state::<Procs>();
        if procs.0.lock().unwrap().contains_key(&id) {
            return Err("Процесс уже запущен".into());
        }
    }
    let dir = PathBuf::from(&cwd);
    if !dir.is_dir() {
        return Err(format!("Папка не найдена: {}", cwd));
    }
    let mut c = shell(&cmdline);
    c.current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("FORCE_COLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONIOENCODING", "utf-8");
    for (k, v) in env {
        c.env(k, v);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    let mut child = c.spawn().map_err(|e| format!("Не удалось запустить: {}", e))?;
    let pid = child.id();
    app.state::<Procs>().0.lock().unwrap().insert(id.clone(), pid);

    if let Some(out) = child.stdout.take() {
        pump(app.clone(), id.clone(), "stdout", out);
    }
    if let Some(err) = child.stderr.take() {
        pump(app.clone(), id.clone(), "stderr", err);
    }
    let app2 = app.clone();
    std::thread::spawn(move || {
        let code = child.wait().ok().and_then(|s| s.code());
        app2.state::<Procs>().0.lock().unwrap().remove(&id);
        // small delay so the last output lines arrive before exit
        std::thread::sleep(std::time::Duration::from_millis(150));
        let _ = app2.emit("proc://exit", ExitEvent { id, code });
    });
    Ok(pid)
}

pub fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        let _ = command("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
    #[cfg(unix)]
    {
        let _ = command("kill").args(["-TERM", "--", &format!("-{}", pid)]).output();
    }
}

pub fn kill(app: &AppHandle, id: &str) -> Result<(), String> {
    let pid = app.state::<Procs>().0.lock().unwrap().get(id).copied();
    match pid {
        Some(p) => {
            kill_pid(p);
            Ok(())
        }
        None => Err("Процесс не найден".into()),
    }
}

pub fn kill_all(app: &AppHandle) {
    let pids: Vec<u32> = app.state::<Procs>().0.lock().unwrap().values().copied().collect();
    for p in pids {
        kill_pid(p);
    }
}

pub fn running(app: &AppHandle) -> Vec<String> {
    app.state::<Procs>().0.lock().unwrap().keys().cloned().collect()
}
