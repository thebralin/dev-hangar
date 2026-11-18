//! Project covers for the grid view: screenshots of the running dev server
//! (headless Edge / Chrome) or a picture chosen by the user.
//! Stored in `<app config>/covers/<hash>-<timestamp>.<ext>`; the timestamp busts the webview cache.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

use crate::util::{command, on_path};

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "svg", "bmp", "avif"];

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = app.path().app_config_dir().map_err(|e| e.to_string())?.join("covers");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

fn key(project: &str) -> String {
    let norm = project.trim_end_matches(['\\', '/']).replace('\\', "/").to_lowercase();
    let mut h = DefaultHasher::new();
    norm.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Remove every stored cover of a project.
fn clear(dir: &Path, key: &str) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(&format!("{key}-")) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

fn new_file(app: &AppHandle, project: &str, ext: &str) -> Result<(PathBuf, String, PathBuf), String> {
    let d = dir(app)?;
    let k = key(project);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file = d.join(format!("{k}-{ts}.{ext}"));
    Ok((d, k, file))
}

fn find_browser() -> Option<String> {
    #[cfg(windows)]
    {
        let mut c: Vec<PathBuf> = Vec::new();
        for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(var) {
                let b = PathBuf::from(base);
                c.push(b.join(r"Microsoft\Edge\Application\msedge.exe"));
                c.push(b.join(r"Google\Chrome\Application\chrome.exe"));
                c.push(b.join(r"BraveSoftware\Brave-Browser\Application\brave.exe"));
                c.push(b.join(r"Chromium\Application\chrome.exe"));
            }
        }
        if let Some(p) = c.into_iter().find(|p| p.is_file()) {
            return Some(p.to_string_lossy().to_string());
        }
    }
    #[cfg(target_os = "macos")]
    {
        for p in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        ] {
            if Path::new(p).is_file() {
                return Some(p.into());
            }
        }
    }
    ["msedge", "google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome", "microsoft-edge"]
        .iter()
        .find(|n| on_path(n))
        .map(|n| n.to_string())
}

/// Screenshot `url` with a headless Chromium browser into the project's cover file.
pub fn capture(app: &AppHandle, project: &str, url: &str, width: u32, height: u32) -> Result<String, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Нужен http(s) адрес".into());
    }
    let browser = find_browser().ok_or("Не найден Edge или Chrome для скриншота — добавьте картинку вручную")?;
    let (d, k, file) = new_file(app, project, "png")?;
    let tmp_profile = std::env::temp_dir().join(format!("devhangar-shot-{}", k));
    let mut c = command(&browser);
    c.args([
        "--headless=new",
        "--disable-gpu",
        "--hide-scrollbars",
        "--mute-audio",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-extensions",
        "--force-device-scale-factor=1",
        "--virtual-time-budget=6000",
    ])
    .arg(format!("--user-data-dir={}", tmp_profile.to_string_lossy()))
    .arg(format!("--window-size={},{}", width, height))
    .arg(format!("--screenshot={}", file.to_string_lossy()))
    .arg(url)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    let mut child = c.spawn().map_err(|e| format!("Не удалось запустить браузер: {}", e))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > Duration::from_secs(45) => {
                let _ = child.kill();
                let _ = std::fs::remove_dir_all(&tmp_profile);
                return Err("Браузер не успел сделать скриншот".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(150)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = std::fs::remove_dir_all(&tmp_profile);
    if !file.is_file() || std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err("Скриншот не получился".into());
    }
    // the new file exists → drop the old ones
    if let Ok(rd) = std::fs::read_dir(&d) {
        for e in rd.flatten() {
            let p = e.path();
            if p != file && e.file_name().to_string_lossy().starts_with(&format!("{k}-")) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    Ok(file.to_string_lossy().to_string())
}

/// Copy a user-chosen picture as the project's cover.
pub fn import(app: &AppHandle, project: &str, src: &str) -> Result<String, String> {
    let s = PathBuf::from(src);
    let ext = s
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| IMAGE_EXTS.contains(&e.as_str()))
        .ok_or("Это не картинка (png, jpg, webp, gif, svg…)")?;
    let meta = std::fs::metadata(&s).map_err(|e| e.to_string())?;
    if meta.len() > 25 * 1024 * 1024 {
        return Err("Картинка больше 25 МБ".into());
    }
    let (d, k, file) = new_file(app, project, &ext)?;
    clear(&d, &k);
    std::fs::copy(&s, &file).map_err(|e| format!("Не удалось скопировать: {}", e))?;
    Ok(file.to_string_lossy().to_string())
}

pub fn remove(app: &AppHandle, project: &str) -> Result<(), String> {
    let d = dir(app)?;
    clear(&d, &key(project));
    Ok(())
}
