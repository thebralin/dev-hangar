use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

/// Command without a flashing console window on Windows.
pub fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Shell command (`cmd /C` on Windows, `sh -c` elsewhere).
pub fn shell(cmdline: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = command("cmd");
        // chcp 65001 → UTF-8 output from console tools
        c.arg("/D").arg("/S").arg("/C");
        c.raw_arg(format!("\"chcp 65001>nul & {}\"", cmdline));
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = command("sh");
        c.arg("-c").arg(cmdline);
        c
    }
}

/// Is an executable available in PATH? Cached for a minute (analyze runs for every project).
pub fn on_path(name: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    static CACHE: OnceLock<Mutex<HashMap<String, (bool, Instant)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(&(hit, at)) = cache.lock().unwrap().get(name) {
        if at.elapsed() < Duration::from_secs(60) {
            return hit;
        }
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(String::from)
            .collect()
    } else {
        vec![String::new()]
    };
    let hit = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| exts.iter().any(|e| d.join(format!("{name}{e}")).is_file())))
        .unwrap_or(false);
    cache.lock().unwrap().insert(name.to_string(), (hit, Instant::now()));
    hit
}

pub fn to_millis(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn modified_ms(p: &Path) -> Option<i64> {
    std::fs::metadata(p).ok()?.modified().ok().map(to_millis)
}

/// Relative path with forward slashes.
pub fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

pub const HEAVY_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".output",
    ".astro",
    ".svelte-kit",
    ".turbo",
    ".cache",
    "out",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    "vendor",
    "bin",
    "obj",
    ".idea",
    ".vs",
];

pub fn is_heavy_dir(name: &str) -> bool {
    HEAVY_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name))
}
