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

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
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
    let file = d.join(format!("{k}-{}.{ext}", now_ms()));
    Ok((d, k, file))
}

/// Every installed Chromium browser, preferred first (Edge, Chrome, Brave, Chromium).
fn find_browsers() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    #[cfg(windows)]
    {
        let mut c: Vec<PathBuf> = Vec::new();
        for rel in [
            r"Microsoft\Edge\Application\msedge.exe",
            r"Google\Chrome\Application\chrome.exe",
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Chromium\Application\chrome.exe",
        ] {
            for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
                if let Ok(base) = std::env::var(var) {
                    c.push(PathBuf::from(base).join(rel));
                }
            }
        }
        found.extend(c.into_iter().filter(|p| p.is_file()).map(|p| p.to_string_lossy().to_string()));
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
                found.push(p.into());
            }
        }
    }
    for n in ["msedge", "google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome", "microsoft-edge"] {
        if on_path(n) {
            found.push(n.to_string());
        }
    }
    found.dedup();
    found
}

/// Screenshot `url` with a headless Chromium browser into the project's cover file.
/// Tries every installed browser, each in the new and then the old headless mode,
/// and reports what the browsers said if all attempts fail.
pub fn capture(app: &AppHandle, project: &str, url: &str, width: u32, height: u32) -> Result<String, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Нужен http(s) адрес".into());
    }
    let browsers = find_browsers();
    if browsers.is_empty() {
        return Err("Не найден Edge или Chrome для скриншота — добавьте картинку вручную".into());
    }
    let (d, k, file) = new_file(app, project, "png")?;
    let mut errors: Vec<String> = Vec::new();
    'outer: for browser in &browsers {
        for mode in ["--headless=new", "--headless"] {
            match shoot(browser, mode, url, &file, width, height) {
                Ok(()) => break 'outer,
                Err(e) => {
                    let name = Path::new(browser).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    errors.push(format!("{name} {mode}: {e}"));
                }
            }
        }
    }
    if !has_image(&file) {
        let _ = std::fs::remove_file(&file);
        return Err(format!("Скриншот не получился.\n{}", errors.join("\n")));
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

fn has_image(file: &Path) -> bool {
    std::fs::metadata(file).map(|m| m.is_file() && m.len() > 0).unwrap_or(false)
}

/// Last meaningful lines of the browser log, for the error message.
fn log_tail(log: &Path) -> String {
    let text = std::fs::read(log).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains("task_manager") && !l.contains("DevTools listening"))
        .collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" · ");
    if tail.chars().count() > 400 {
        tail.chars().take(400).collect::<String>() + "…"
    } else {
        tail
    }
}

/// One attempt: a fresh profile every time, so a browser left over from an earlier
/// attempt can never take the command line over (and exit without a screenshot).
fn shoot(browser: &str, mode: &str, url: &str, file: &Path, width: u32, height: u32) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!("devhangar-shot-{}-{}", std::process::id(), now_ms()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let profile = tmp.join("profile");
    let log = tmp.join("browser.log");
    let res = (|| {
        let log_file = std::fs::File::create(&log).map_err(|e| e.to_string())?;
        let log_err = log_file.try_clone().map_err(|e| e.to_string())?;
        let mut c = command(browser);
        c.arg(mode)
            .args([
                "--disable-gpu",
                "--hide-scrollbars",
                "--mute-audio",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--disable-background-networking",
                "--disable-component-update",
                "--disable-sync",
                "--force-device-scale-factor=1",
                "--virtual-time-budget=6000",
                "--enable-logging=stderr",
                "--v=0",
            ])
            .arg(format!("--user-data-dir={}", profile.to_string_lossy()))
            .arg(format!("--window-size={},{}", width, height))
            .arg(format!("--screenshot={}", file.to_string_lossy()))
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(log_file)
            .stderr(log_err);
        let mut child = c.spawn().map_err(|e| format!("не запускается: {}", e))?;
        let start = Instant::now();
        let code = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s.code(),
                Ok(None) if start.elapsed() > Duration::from_secs(45) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("не успел за 45 секунд".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(150)),
                Err(e) => return Err(e.to_string()),
            }
        };
        // the launcher may hand the work to another browser process and exit first
        let until = Instant::now() + Duration::from_secs(8);
        while !has_image(file) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(200));
        }
        if has_image(file) {
            return Ok(());
        }
        let tail = log_tail(&log);
        Err(format!(
            "браузер завершился (код {}) без скриншота{}",
            code.map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
            if tail.is_empty() { String::new() } else { format!(": {tail}") }
        ))
    })();
    // helper processes may still hold the profile for a moment
    for _ in 0..10 {
        if std::fs::remove_dir_all(&tmp).is_ok() || !tmp.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    res
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

#[cfg(test)]
mod tests {
    /// Needs a running dev server:
    /// `$env:SHOT_URL="http://localhost:5173/"; cargo test --lib shoot_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn shoot_live() {
        let url = std::env::var("SHOT_URL").unwrap_or_else(|_| "http://localhost:1420/".into());
        let browsers = super::find_browsers();
        assert!(!browsers.is_empty(), "no browser");
        let file = std::env::temp_dir().join("devhangar-shot-test.png");
        let _ = std::fs::remove_file(&file);
        for b in &browsers {
            let r = super::shoot(b, "--headless=new", &url, &file, 1280, 800);
            println!("{b}: {r:?}");
            let _ = std::fs::remove_file(&file);
        }
        // an unreachable port still gives a picture (the browser's error page) — must not fail
        let r = super::shoot(&browsers[0], "--headless=new", "http://127.0.0.1:9/", &file, 800, 500);
        println!("unreachable: {r:?}");
    }
}
