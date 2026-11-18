use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

use crate::util::{command, shell};

pub struct ProcHandle {
    pub pid: u32,
    /// stdin pipe, so the user can type into a running process from the console
    pub stdin: Option<Arc<Mutex<ChildStdin>>>,
}

#[derive(Default)]
pub struct Procs(pub Mutex<HashMap<String, ProcHandle>>);

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
        .stdin(Stdio::piped())
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
    let stdin = child.stdin.take().map(|s| Arc::new(Mutex::new(s)));
    app.state::<Procs>().0.lock().unwrap().insert(id.clone(), ProcHandle { pid, stdin });

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
    let pid = app.state::<Procs>().0.lock().unwrap().get(id).map(|h| h.pid);
    match pid {
        Some(p) => {
            kill_pid(p);
            Ok(())
        }
        None => Err("Процесс не найден".into()),
    }
}

/// Write text to a running process' stdin.
pub fn write_stdin(app: &AppHandle, id: &str, data: &str) -> Result<(), String> {
    let pipe = {
        let procs = app.state::<Procs>();
        let map = procs.0.lock().unwrap();
        let h = map.get(id).ok_or("Процесс не запущен")?;
        h.stdin.clone().ok_or("У процесса нет ввода")?
    };
    let mut w = pipe.lock().unwrap();
    w.write_all(data.as_bytes())
        .and_then(|_| w.flush())
        .map_err(|e| format!("Процесс не принимает ввод: {}", e))
}

pub fn kill_all(app: &AppHandle) {
    let pids: Vec<u32> = app.state::<Procs>().0.lock().unwrap().values().map(|h| h.pid).collect();
    for p in pids {
        kill_pid(p);
    }
}

pub fn running(app: &AppHandle) -> Vec<String> {
    app.state::<Procs>().0.lock().unwrap().keys().cloned().collect()
}
