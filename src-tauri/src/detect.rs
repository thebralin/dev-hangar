use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::util::{is_heavy_dir, modified_ms, rel};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Stack {
    pub id: String,
    pub label: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub label: String,
    pub command: String,
    /// dev | build | start | install | test | script | open | serve
    pub kind: String,
    /// working directory relative to project root ("" = root)
    pub cwd: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub dev: bool,
    /// npm | cargo | pip | go | composer
    pub source: String,
    /// subproject relative path ("" = root)
    pub scope: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    /// relative path ("" = root)
    pub rel_path: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub stacks: Vec<Stack>,
    pub package_manager: Option<String>,
    pub scripts: Vec<(String, String)>,
    pub actions: Vec<Action>,
    pub needs_install: bool,
    pub node_version: Option<String>,
    /// PHP: required version (.php-version / composer require.php)
    pub php_version: Option<String>,
    /// PHP: composer.json present but vendor/ is not
    pub vendor_missing: bool,
    /// composer.json "scripts" (run with `composer run <name>`)
    pub composer_scripts: Vec<(String, String)>,
    /// tools the module needs but which are not in PATH (php, composer)
    pub missing_tools: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub root: Module,
    pub subprojects: Vec<Module>,
    pub stacks: Vec<Stack>,
    pub dependencies: Vec<Dependency>,
    pub env_files: Vec<String>,
    pub readme: Option<String>,
    pub notable_files: Vec<String>,
    pub is_git: bool,
    pub modified: Option<i64>,
}

pub(crate) fn read_json(p: &Path) -> Option<Value> {
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()
}

fn read_toml(p: &Path) -> Option<toml::Value> {
    let s = fs::read_to_string(p).ok()?;
    toml::from_str(&s).ok()
}

fn has(dir: &Path, name: &str) -> bool {
    dir.join(name).exists()
}

fn has_ext(dir: &Path, ext: &str) -> bool {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                e.path()
                    .extension()
                    .map(|x| x.eq_ignore_ascii_case(ext))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

pub(crate) fn push_stack(v: &mut Vec<Stack>, id: &str, label: &str) {
    if !v.iter().any(|s| s.id == id) {
        v.push(Stack {
            id: id.into(),
            label: label.into(),
        });
    }
}

pub(crate) fn action(id: &str, label: &str, command: String, kind: &str, cwd: &str) -> Action {
    Action {
        id: if cwd.is_empty() {
            id.to_string()
        } else {
            format!("{}:{}", cwd, id)
        },
        label: label.to_string(),
        command,
        kind: kind.to_string(),
        cwd: cwd.to_string(),
    }
}

fn detect_pm(dir: &Path, pkg: Option<&Value>, root: &Path) -> String {
    // lockfiles in the module dir, then in the root (monorepo)
    for d in [dir, root] {
        if has(d, "bun.lockb") || has(d, "bun.lock") {
            return "bun".into();
        }
        if has(d, "pnpm-lock.yaml") {
            return "pnpm".into();
        }
        if has(d, "yarn.lock") {
            return "yarn".into();
        }
        if has(d, "package-lock.json") {
            return "npm".into();
        }
    }
    if let Some(pm) = pkg
        .and_then(|p| p.get("packageManager"))
        .and_then(|v| v.as_str())
    {
        for n in ["bun", "pnpm", "yarn", "npm"] {
            if pm.starts_with(n) {
                return n.into();
            }
        }
    }
    "npm".into()
}

fn pm_run(pm: &str, script: &str) -> String {
    match pm {
        "yarn" => format!("yarn {}", script),
        _ => format!("{} run {}", pm, script),
    }
}

fn pm_x(pm: &str) -> &'static str {
    match pm {
        "bun" => "bunx",
        "pnpm" => "pnpm exec",
        "yarn" => "yarn",
        _ => "npx",
    }
}

fn dep_version(pkg: &Value, name: &str) -> Option<String> {
    for key in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(v) = pkg.get(key).and_then(|d| d.get(name)).and_then(|v| v.as_str()) {
            return Some(v.to_string());
        }
    }
    None
}

/// Analyze a single directory as a module.
fn analyze_module(root: &Path, dir: &Path, deps_out: &mut Vec<Dependency>) -> Option<Module> {
    let rel_path = rel(root, dir);
    let cwd = rel_path.as_str();
    let mut m = Module {
        rel_path: rel_path.clone(),
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        ..Default::default()
    };
    let mut recognized = false;

    // ---------- Node / JS ----------
    if let Some(pkg) = read_json(&dir.join("package.json")) {
        recognized = true;
        if let Some(n) = pkg.get("name").and_then(|v| v.as_str()) {
            m.name = n.to_string();
        }
        m.version = pkg.get("version").and_then(|v| v.as_str()).map(String::from);
        m.description = pkg
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);
        let pm = detect_pm(dir, Some(&pkg), root);
        m.package_manager = Some(pm.clone());
        m.needs_install = !dir.join("node_modules").exists() && !root.join("node_modules").exists();

        if let Some(e) = pkg
            .get("engines")
            .and_then(|e| e.get("node"))
            .and_then(|v| v.as_str())
        {
            m.node_version = Some(e.to_string());
        }
        for f in [".nvmrc", ".node-version"] {
            if let Ok(s) = fs::read_to_string(dir.join(f)) {
                m.node_version = Some(s.trim().to_string());
            }
        }

        let has_dep = |n: &str| dep_version(&pkg, n).is_some();
        let has_dep_prefix = |prefix: &str| {
            ["dependencies", "devDependencies"].iter().any(|k| {
                pkg.get(*k)
                    .and_then(|d| d.as_object())
                    .map(|o| o.keys().any(|key| key.starts_with(prefix)))
                    .unwrap_or(false)
            })
        };

        let st = &mut m.stacks;
        let checks: &[(&str, &str, &str)] = &[
            ("nuxt", "nuxt", "Nuxt"),
            ("vue", "vue", "Vue"),
            ("astro", "astro", "Astro"),
            ("next", "next", "Next.js"),
            ("react", "react", "React"),
            ("svelte", "svelte", "Svelte"),
            ("@sveltejs/kit", "sveltekit", "SvelteKit"),
            ("solid-js", "solid", "Solid"),
            ("@angular/core", "angular", "Angular"),
            ("electron", "electron", "Electron"),
            ("@tauri-apps/cli", "tauri", "Tauri"),
            ("@tauri-apps/api", "tauri", "Tauri"),
            ("express", "express", "Express"),
            ("hono", "hono", "Hono"),
            ("fastify", "fastify", "Fastify"),
            ("koa", "koa", "Koa"),
            ("elysia", "elysia", "Elysia"),
            ("@nestjs/core", "nest", "NestJS"),
            ("socket.io", "socketio", "Socket.IO"),
            ("vite", "vite", "Vite"),
            ("webpack", "webpack", "Webpack"),
            ("tailwindcss", "tailwind", "Tailwind"),
            ("prisma", "prisma", "Prisma"),
            ("drizzle-orm", "drizzle", "Drizzle"),
            ("typescript", "ts", "TypeScript"),
        ];
        for (dep, id, label) in checks {
            if has_dep(dep) {
                push_stack(st, id, label);
            }
        }
        if has_dep_prefix("@electron-forge/") || has_dep("electron-builder") {
            push_stack(st, "electron", "Electron");
        }
        if dir.join("src-tauri").is_dir() {
            push_stack(st, "tauri", "Tauri");
        }
        if pm == "bun" || has_dep("@types/bun") || has_dep("bun-types") {
            push_stack(st, "bun", "Bun");
        } else {
            push_stack(st, "node", "Node.js");
        }

        // scripts
        if let Some(obj) = pkg.get("scripts").and_then(|s| s.as_object()) {
            for (k, v) in obj {
                m.scripts
                    .push((k.clone(), v.as_str().unwrap_or_default().to_string()));
            }
        }
        let script_names: Vec<&str> = m.scripts.iter().map(|(k, _)| k.as_str()).collect();
        let find = |cands: &[&str]| -> Option<String> {
            cands
                .iter()
                .find(|c| script_names.contains(c))
                .map(|s| s.to_string())
        };
        let is_tauri = m.stacks.iter().any(|s| s.id == "tauri");

        // dev
        if is_tauri {
            if script_names.contains(&"tauri") {
                m.actions.push(action("dev", "Tauri dev", format!("{} dev", pm_run(&pm, "tauri")), "dev", cwd));
                m.actions.push(action("build", "Tauri build", format!("{} build", pm_run(&pm, "tauri")), "build", cwd));
            } else if let Some(s) = find(&["tauri:dev", "tauri-dev", "app:dev"]) {
                m.actions.push(action("dev", "Tauri dev", pm_run(&pm, &s), "dev", cwd));
            } else {
                m.actions.push(action("dev", "Tauri dev", format!("{} tauri dev", pm_x(&pm)), "dev", cwd));
                m.actions.push(action("build", "Tauri build", format!("{} tauri build", pm_x(&pm)), "build", cwd));
            }
            if let Some(s) = find(&["dev"]) {
                m.actions.push(action("web-dev", "Frontend dev", pm_run(&pm, &s), "dev", cwd));
            }
        } else if let Some(s) = find(&["dev", "start:dev", "develop", "serve", "watch", "start"]) {
            m.actions.push(action("dev", &format!("Dev ({})", s), pm_run(&pm, &s), "dev", cwd));
        }
        if !m.actions.iter().any(|a| a.kind == "build") {
            if let Some(s) = find(&["build", "make", "package", "dist", "compile"]) {
                m.actions.push(action("build", &format!("Build ({})", s), pm_run(&pm, &s), "build", cwd));
            }
        }
        if let Some(s) = find(&["start"]) {
            if !m.actions.iter().any(|a| a.command == pm_run(&pm, &s)) {
                m.actions.push(action("start", "Start", pm_run(&pm, &s), "start", cwd));
            }
        }
        if let Some(s) = find(&["preview"]) {
            m.actions.push(action("preview", "Preview", pm_run(&pm, &s), "start", cwd));
        }
        if let Some(s) = find(&["test"]) {
            m.actions.push(action("test", "Test", pm_run(&pm, &s), "test", cwd));
        }
        m.actions.push(action("install", "Install", format!("{} install", pm), "install", cwd));
        // no scripts and a plain entry → node/bun main
        if m.scripts.is_empty() {
            let main = pkg
                .get("main")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    ["index.js", "server.js", "app.js", "main.js", "index.ts", "src/index.ts", "src/index.js"]
                        .iter()
                        .find(|f| dir.join(f).exists())
                        .map(|s| s.to_string())
                });
            if let Some(main) = main {
                let runner = if pm == "bun" { "bun" } else { "node" };
                let runner = if main.ends_with(".ts") && runner == "node" { "npx tsx" } else { runner };
                m.actions.insert(0, action("dev", &format!("Run {}", main), format!("{} {}", runner, main), "dev", cwd));
            }
        }

        for (key, dev) in [("dependencies", false), ("devDependencies", true)] {
            if let Some(obj) = pkg.get(key).and_then(|d| d.as_object()) {
                for (n, v) in obj {
                    deps_out.push(Dependency {
                        name: n.clone(),
                        version: v.as_str().unwrap_or_default().to_string(),
                        dev,
                        source: "npm".into(),
                        scope: rel_path.clone(),
                    });
                }
            }
        }
    }

    // ---------- Deno ----------
    for f in ["deno.json", "deno.jsonc"] {
        if let Some(cfg) = read_json(&dir.join(f)) {
            recognized = true;
            push_stack(&mut m.stacks, "deno", "Deno");
            if let Some(tasks) = cfg.get("tasks").and_then(|t| t.as_object()) {
                for (k, v) in tasks {
                    m.scripts.push((k.clone(), v.as_str().unwrap_or_default().to_string()));
                    let kind = match k.as_str() {
                        "dev" | "start" => "dev",
                        "build" => "build",
                        _ => "script",
                    };
                    if kind != "script" && !m.actions.iter().any(|a| a.kind == kind) {
                        m.actions.push(action(k, &format!("deno {}", k), format!("deno task {}", k), kind, cwd));
                    }
                }
            }
        }
    }

    // ---------- Rust ----------
    if let Some(cargo) = read_toml(&dir.join("Cargo.toml")) {
        recognized = true;
        push_stack(&mut m.stacks, "rust", "Rust");
        let is_tauri_backend = dir.file_name().map(|n| n == "src-tauri").unwrap_or(false)
            || dir.join("tauri.conf.json").exists();
        if is_tauri_backend {
            push_stack(&mut m.stacks, "tauri", "Tauri");
        }
        if let Some(pkg) = cargo.get("package") {
            if m.version.is_none() {
                m.version = pkg.get("version").and_then(|v| v.as_str()).map(String::from);
            }
            if m.description.is_none() {
                m.description = pkg.get("description").and_then(|v| v.as_str()).map(String::from);
            }
            if let Some(n) = pkg.get("name").and_then(|v| v.as_str()) {
                if !dir.join("package.json").exists() {
                    m.name = n.to_string();
                }
            }
        }
        if !is_tauri_backend {
            if !m.actions.iter().any(|a| a.kind == "dev") {
                m.actions.push(action("cargo-run", "cargo run", "cargo run".into(), "dev", cwd));
            }
            m.actions.push(action("cargo-build", "cargo build --release", "cargo build --release".into(), "build", cwd));
            m.actions.push(action("cargo-test", "cargo test", "cargo test".into(), "test", cwd));
        }
        for (key, dev) in [("dependencies", false), ("dev-dependencies", true), ("build-dependencies", true)] {
            if let Some(tbl) = cargo.get(key).and_then(|d| d.as_table()) {
                for (n, v) in tbl {
                    let ver = match v {
                        toml::Value::String(s) => s.clone(),
                        toml::Value::Table(t) => t
                            .get("version")
                            .and_then(|x| x.as_str())
                            .map(String::from)
                            .or_else(|| t.get("path").and_then(|x| x.as_str()).map(|p| format!("path:{}", p)))
                            .or_else(|| t.get("git").and_then(|x| x.as_str()).map(|p| format!("git:{}", p)))
                            .unwrap_or_else(|| "*".into()),
                        _ => "*".into(),
                    };
                    deps_out.push(Dependency {
                        name: n.clone(),
                        version: ver,
                        dev,
                        source: "cargo".into(),
                        scope: rel_path.clone(),
                    });
                }
            }
        }
    }

    // ---------- Go ----------
    if let Ok(gomod) = fs::read_to_string(dir.join("go.mod")) {
        recognized = true;
        push_stack(&mut m.stacks, "go", "Go");
        m.actions.push(action("go-run", "go run .", "go run .".into(), "dev", cwd));
        m.actions.push(action("go-build", "go build", "go build ./...".into(), "build", cwd));
        let mut in_block = false;
        for line in gomod.lines() {
            let l = line.trim();
            if l.starts_with("require (") {
                in_block = true;
                continue;
            }
            if in_block && l == ")" {
                in_block = false;
                continue;
            }
            let body = if in_block {
                Some(l)
            } else {
                l.strip_prefix("require ")
            };
            if let Some(b) = body {
                let parts: Vec<&str> = b.split_whitespace().collect();
                if parts.len() >= 2 {
                    deps_out.push(Dependency {
                        name: parts[0].into(),
                        version: parts[1].into(),
                        dev: b.contains("// indirect"),
                        source: "go".into(),
                        scope: rel_path.clone(),
                    });
                }
            }
        }
    }

    // ---------- Python ----------
    let py = has(dir, "requirements.txt") || has(dir, "pyproject.toml") || has(dir, "manage.py") || has(dir, "Pipfile");
    if py {
        recognized = true;
        push_stack(&mut m.stacks, "python", "Python");
        let venv_py = if dir.join(".venv").is_dir() {
            if cfg!(windows) { ".venv\\Scripts\\python" } else { ".venv/bin/python" }
        } else if dir.join("venv").is_dir() {
            if cfg!(windows) { "venv\\Scripts\\python" } else { "venv/bin/python" }
        } else if cfg!(windows) {
            "python"
        } else {
            "python3"
        };
        if has(dir, "manage.py") {
            push_stack(&mut m.stacks, "django", "Django");
            m.actions.push(action("django", "runserver", format!("{} manage.py runserver", venv_py), "dev", cwd));
        } else if let Some(entry) = ["main.py", "app.py", "run.py", "server.py", "bot.py"]
            .iter()
            .find(|f| dir.join(f).exists())
        {
            m.actions.push(action("py-run", &format!("python {}", entry), format!("{} {}", venv_py, entry), "dev", cwd));
        }
        if let Ok(req) = fs::read_to_string(dir.join("requirements.txt")) {
            m.actions.push(action("pip", "pip install -r", format!("{} -m pip install -r requirements.txt", venv_py), "install", cwd));
            for l in req.lines() {
                let l = l.trim();
                if l.is_empty() || l.starts_with('#') || l.starts_with('-') {
                    continue;
                }
                let idx = l.find(|c: char| "=<>!~[; ".contains(c)).unwrap_or(l.len());
                let (n, v) = l.split_at(idx);
                deps_out.push(Dependency {
                    name: n.trim().into(),
                    version: v.trim().trim_start_matches("==").into(),
                    dev: false,
                    source: "pip".into(),
                    scope: rel_path.clone(),
                });
            }
        }
        if let Some(pp) = read_toml(&dir.join("pyproject.toml")) {
            if let Some(arr) = pp.get("project").and_then(|p| p.get("dependencies")).and_then(|d| d.as_array()) {
                for d in arr.iter().filter_map(|x| x.as_str()) {
                    let idx = d.find(|c: char| "=<>!~[; ".contains(c)).unwrap_or(d.len());
                    let (n, v) = d.split_at(idx);
                    deps_out.push(Dependency {
                        name: n.trim().into(),
                        version: v.trim().into(),
                        dev: false,
                        source: "pip".into(),
                        scope: rel_path.clone(),
                    });
                }
            }
            if has(dir, "uv.lock") {
                m.actions.push(action("uv-sync", "uv sync", "uv sync".into(), "install", cwd));
            }
        }
    }

    // ---------- PHP ----------
    if crate::php::detect(root, dir, &mut m, deps_out) {
        recognized = true;
    }

    // ---------- .NET ----------
    if has_ext(dir, "csproj") || has_ext(dir, "sln") || has_ext(dir, "fsproj") {
        recognized = true;
        push_stack(&mut m.stacks, "dotnet", ".NET");
        m.actions.push(action("dotnet-run", "dotnet run", "dotnet run".into(), "dev", cwd));
        m.actions.push(action("dotnet-build", "dotnet build", "dotnet build -c Release".into(), "build", cwd));
    }

    // ---------- Java / Kotlin ----------
    if has(dir, "pom.xml") {
        recognized = true;
        push_stack(&mut m.stacks, "java", "Java / Maven");
        let mvn = if has(dir, "mvnw.cmd") && cfg!(windows) { "mvnw.cmd" } else if has(dir, "mvnw") { "./mvnw" } else { "mvn" };
        m.actions.push(action("mvn-build", "mvn package", format!("{} package", mvn), "build", cwd));
    }
    if has(dir, "build.gradle") || has(dir, "build.gradle.kts") {
        recognized = true;
        push_stack(&mut m.stacks, "gradle", "Gradle");
        let g = if cfg!(windows) && has(dir, "gradlew.bat") { "gradlew.bat" } else if has(dir, "gradlew") { "./gradlew" } else { "gradle" };
        m.actions.push(action("gradle-run", "gradle run", format!("{} run", g), "dev", cwd));
        m.actions.push(action("gradle-build", "gradle build", format!("{} build", g), "build", cwd));
    }

    // ---------- Flutter / Dart ----------
    if has(dir, "pubspec.yaml") {
        recognized = true;
        push_stack(&mut m.stacks, "flutter", "Flutter / Dart");
        m.actions.push(action("flutter-run", "flutter run", "flutter run".into(), "dev", cwd));
        m.actions.push(action("flutter-build", "flutter build", "flutter build windows".into(), "build", cwd));
    }

    // ---------- C / C++ ----------
    if has(dir, "CMakeLists.txt") {
        recognized = true;
        push_stack(&mut m.stacks, "cmake", "CMake");
        m.actions.push(action("cmake-build", "cmake build", "cmake -B build && cmake --build build".into(), "build", cwd));
    }
    if has(dir, "Makefile") {
        recognized = true;
        push_stack(&mut m.stacks, "make", "Make");
        m.actions.push(action("make", "make", "make".into(), "build", cwd));
    }

    // ---------- Docker ----------
    let compose = ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
        .iter()
        .find(|f| dir.join(f).exists());
    if compose.is_some() || has(dir, "Dockerfile") {
        push_stack(&mut m.stacks, "docker", "Docker");
        if compose.is_some() {
            recognized = true;
            m.actions.push(action("compose-up", "docker compose up", "docker compose up".into(), "serve", cwd));
        }
    }

    // ---------- Vanilla HTML ----------
    let has_pkg = dir.join("package.json").exists();
    let dir_name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let asset_dir = dir != root
        && ["src", "public", "www", "assets", "static", "docs", "dist", "examples", "test", "tests"].contains(&dir_name.as_str());
    if !has_pkg && !asset_dir {
        let index = ["index.html", "src/index.html", "public/index.html", "www/index.html"]
            .iter()
            .find(|f| dir.join(f).exists());
        if let Some(index) = index {
            recognized = true;
            push_stack(&mut m.stacks, "html", "HTML / JS");
            let serve_dir = Path::new(index).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            let serve_dir = if serve_dir.is_empty() { ".".to_string() } else { serve_dir };
            m.actions.push(action("open-index", &format!("Открыть {}", index), index.to_string(), "open", cwd));
            m.actions.push(action(
                "live-server",
                "Live server",
                format!("npx --yes live-server {} --port=5500", serve_dir),
                "dev",
                cwd,
            ));
        }
    }

    if recognized {
        Some(m)
    } else {
        None
    }
}

fn candidate_subdirs(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let scan = |dir: &Path, out: &mut Vec<PathBuf>| {
        if let Ok(rd) = fs::read_dir(dir) {
            let mut v: Vec<PathBuf> = rd
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    !n.starts_with('.') && !is_heavy_dir(&n)
                })
                .collect();
            v.sort();
            out.extend(v);
        }
    };
    scan(root, &mut out);
    for group in ["apps", "packages", "services", "libs"] {
        let g = root.join(group);
        if g.is_dir() {
            scan(&g, &mut out);
        }
    }
    out
}

pub fn analyze(path: &str) -> Result<ProjectInfo, String> {
    let root = PathBuf::from(path);
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    if !root.is_dir() {
        return Ok(ProjectInfo {
            name,
            path: path.into(),
            exists: false,
            root: Module::default(),
            subprojects: vec![],
            stacks: vec![],
            dependencies: vec![],
            env_files: vec![],
            readme: None,
            notable_files: vec![],
            is_git: false,
            modified: None,
        });
    }

    let mut deps = vec![];
    let mut root_mod = analyze_module(&root, &root, &mut deps).unwrap_or_else(|| Module {
        rel_path: String::new(),
        name: name.clone(),
        ..Default::default()
    });
    if root_mod.name.is_empty() {
        root_mod.name = name.clone();
    }

    let mut subprojects = vec![];
    for d in candidate_subdirs(&root) {
        if let Some(m) = analyze_module(&root, &d, &mut deps) {
            subprojects.push(m);
        }
    }

    let mut stacks = root_mod.stacks.clone();
    for s in subprojects.iter().flat_map(|m| m.stacks.iter()) {
        push_stack(&mut stacks, &s.id, &s.label);
    }
    if stacks.is_empty() {
        push_stack(&mut stacks, "unknown", "Не определён");
    }

    let mut env_files = vec![];
    let mut readme = None;
    let mut notable = vec![];
    if let Ok(rd) = fs::read_dir(&root) {
        let mut entries: Vec<String> = rd
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        entries.sort();
        for n in entries {
            let lower = n.to_lowercase();
            if lower.starts_with(".env") {
                env_files.push(n.clone());
            }
            if readme.is_none() && lower.starts_with("readme") {
                readme = Some(n.clone());
            }
            let notable_names = [
                "vite.config.ts", "vite.config.js", "vite.config.mjs", "astro.config.mjs", "astro.config.ts",
                "nuxt.config.ts", "next.config.js", "next.config.mjs", "tsconfig.json", "electron-builder.yml",
                "electron-builder.json", "forge.config.js", "forge.config.ts", "dockerfile", "docker-compose.yml",
                "compose.yml", ".gitignore", "wrangler.toml", "vercel.json", "netlify.toml", "tailwind.config.js",
                "tailwind.config.ts", "drizzle.config.ts", "ecosystem.config.js", "biome.json", ".prettierrc",
                "eslint.config.js", ".eslintrc.json", "claude.md", "agents.md", "composer.json", "artisan",
                "phpunit.xml", "phpunit.xml.dist", "phpstan.neon", "phpstan.neon.dist", "pint.json",
                ".php-cs-fixer.php", ".php-cs-fixer.dist.php", "rector.php", ".php-version", "symfony.lock",
                "wp-config.php", ".htaccess",
            ];
            if notable_names.contains(&lower.as_str()) {
                notable.push(n);
            }
        }
    }
    if root.join("src-tauri/tauri.conf.json").exists() {
        notable.push("src-tauri/tauri.conf.json".into());
    }

    // de-duplicate deps by (scope, source, name)
    let mut seen = BTreeMap::new();
    for d in deps {
        seen.entry((d.scope.clone(), d.source.clone(), d.name.clone())).or_insert(d);
    }

    Ok(ProjectInfo {
        name: if root_mod.name.is_empty() { name } else { root_mod.name.clone() },
        path: path.into(),
        exists: true,
        root: root_mod,
        subprojects,
        stacks,
        dependencies: seen.into_values().collect(),
        env_files,
        readme,
        notable_files: notable,
        is_git: root.join(".git").exists(),
        modified: modified_ms(&root),
    })
}

// ---------------- project statistics (walk) ----------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    pub modified: i64,
    pub size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStats {
    pub files: u64,
    pub dirs: u64,
    pub total_size: u64,
    pub last_modified: Option<i64>,
    pub recent: Vec<RecentFile>,
    pub languages: Vec<(String, u64)>,
    pub truncated: bool,
}

pub fn stats(path: &str) -> Result<ProjectStats, String> {
    let root = PathBuf::from(path);
    let walker = ignore::WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(false)
        .require_git(false)
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.depth() > 0
                && e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && [".git", "node_modules", "target", "dist", ".next", ".nuxt", ".output", ".svelte-kit", ".turbo", ".cache", "__pycache__", ".venv", "venv", "vendor"]
                    .contains(&n.as_ref()))
        })
        .build();

    let mut files = 0u64;
    let mut dirs = 0u64;
    let mut total = 0u64;
    let mut recent: Vec<RecentFile> = vec![];
    let mut langs: BTreeMap<String, u64> = BTreeMap::new();
    let mut truncated = false;

    for e in walker.flatten() {
        if e.depth() == 0 {
            continue;
        }
        let ft = match e.file_type() {
            Some(t) => t,
            None => continue,
        };
        if ft.is_dir() {
            dirs += 1;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        files += 1;
        if files > 60_000 {
            truncated = true;
            break;
        }
        let md = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        total += md.len();
        if let Some(ext) = e.path().extension() {
            *langs.entry(ext.to_string_lossy().to_lowercase()).or_default() += 1;
        }
        let mt = md.modified().map(crate::util::to_millis).unwrap_or(0);
        if recent.len() < 25 || mt > recent.last().map(|r| r.modified).unwrap_or(0) {
            recent.push(RecentFile {
                path: rel(&root, e.path()),
                modified: mt,
                size: md.len(),
            });
            recent.sort_by(|a, b| b.modified.cmp(&a.modified));
            recent.truncate(25);
        }
    }

    let mut languages: Vec<(String, u64)> = langs.into_iter().collect();
    languages.sort_by(|a, b| b.1.cmp(&a.1));
    languages.truncate(12);

    Ok(ProjectStats {
        files,
        dirs,
        total_size: total,
        last_modified: recent.first().map(|r| r.modified),
        recent,
        languages,
        truncated,
    })
}
