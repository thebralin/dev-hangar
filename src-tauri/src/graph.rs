//! Граф связей между файлами проекта: кто кого импортирует.
//! Парсинг регулярками (без AST): JS/TS/Vue/Svelte/Astro, CSS/SCSS/Less, HTML, Python, Rust, Go,
//! PHP (use + PSR-4 из composer.json, require/include, Blade, Twig, Laravel view()/Inertia),
//! плюс вызовы Tauri `invoke("cmd")` → файл с `#[tauri::command] fn cmd`.

use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::analytics::lang_of;
use crate::util::rel;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub path: String,
    pub lang: String,
    pub lines: u32,
    pub external: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    /// "import" | "ipc"
    pub kind: String,
    pub count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub truncated: bool,
}

const SKIP: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".next", ".nuxt", ".output", ".astro", ".svelte-kit",
    ".turbo", ".cache", "coverage", "__pycache__", ".venv", "venv", ".vs", ".idea", "vendor", "out", "release",
];
const MAX_FILES: usize = 4000;
const MAX_SIZE: u64 = 1_000_000;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Js,
    Css,
    Html,
    Py,
    Rs,
    Go,
    Php,
    /// Twig templates
    Tpl,
}

fn kind_of(name: &str) -> Option<Kind> {
    let lower = name.to_lowercase();
    if lower.ends_with(".min.js") || lower.ends_with(".d.ts") {
        return None;
    }
    let ext = lower.rsplit_once('.')?.1;
    Some(match ext {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" | "vue" | "svelte" | "astro" => Kind::Js,
        "css" | "scss" | "sass" | "less" => Kind::Css,
        "html" | "htm" => Kind::Html,
        "py" => Kind::Py,
        "rs" => Kind::Rs,
        "go" => Kind::Go,
        "php" | "phtml" => Kind::Php,
        "twig" => Kind::Tpl,
        _ => return None,
    })
}

struct File {
    rel: String,
    abs: PathBuf,
    kind: Kind,
}

enum Target {
    Local(usize),
    External(String),
    None,
}

struct Alias {
    key: String,
    wild: bool,
    targets: Vec<String>,
}

struct JsCfg {
    dir: String,
    aliases: Vec<Alias>,
    base_url: Option<String>,
}

struct Index {
    files: Vec<File>,
    by_rel: HashMap<String, usize>,
    by_dir: HashMap<String, Vec<usize>>,
    js_cfgs: Vec<JsCfg>,
    go_mods: Vec<(String, String)>,
    py_roots: Vec<String>,
    php: PhpIndex,
}

#[derive(Default)]
struct PhpIndex {
    /// lowercased FQN of every declared class/interface/trait/enum → file
    classes: HashMap<String, usize>,
    /// composer autoload psr-4: (lowercased namespace prefix "app\\", dirs), longest prefix first
    psr4: Vec<(String, Vec<String>)>,
    /// vendor/composer/autoload_psr4.php: (lowercased prefix, package name), longest first
    vendor: Vec<(String, String)>,
    /// lowercased top-level namespaces of the project itself ("app", "database")
    own: HashSet<String>,
    /// Laravel resources/views, Inertia resources/js/Pages, Twig templates/
    view_roots: Vec<String>,
    page_roots: Vec<String>,
    twig_roots: Vec<String>,
}

// ---------- path helpers (relative paths with '/') ----------

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map(|x| x.0).unwrap_or("")
}

fn file_name(p: &str) -> &str {
    p.rsplit_once('/').map(|x| x.1).unwrap_or(p)
}

/// Joins `spec` onto `base`, resolving `.` and `..`. None if it escapes the project root.
fn join(base: &str, spec: &str) -> Option<String> {
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in spec.split(['/', '\\']) {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

fn is_under(p: &str, dir: &str) -> bool {
    dir.is_empty() || p.len() > dir.len() && p.starts_with(dir) && p.as_bytes()[dir.len()] == b'/'
}

impl Index {
    fn lookup(&self, rel: &str) -> Option<usize> {
        self.by_rel.get(&rel.to_lowercase()).copied()
    }

    // ---------- JS / TS ----------

    fn js_cfg(&self, from: &str) -> Option<&JsCfg> {
        self.js_cfgs
            .iter()
            .filter(|c| is_under(from, &c.dir))
            .max_by_key(|c| c.dir.len())
    }

    fn try_js(&self, base: &str) -> Option<usize> {
        const EXTS: &[&str] = &[
            "", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".vue", ".svelte", ".astro",
        ];
        for e in EXTS {
            if let Some(i) = self.lookup(&format!("{base}{e}")) {
                if !e.is_empty() || !self.is_dir(base) {
                    return Some(i);
                }
            }
        }
        for e in &EXTS[1..] {
            if let Some(i) = self.lookup(&format!("{base}/index{e}")) {
                return Some(i);
            }
        }
        // TS ESM style: import "./x.js" → x.ts
        for (from, tos) in [(".js", &[".ts", ".tsx"][..]), (".jsx", &[".tsx"]), (".mjs", &[".mts"]), (".cjs", &[".cts"])] {
            if let Some(stem) = base.strip_suffix(from) {
                for to in tos {
                    if let Some(i) = self.lookup(&format!("{stem}{to}")) {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    fn is_dir(&self, p: &str) -> bool {
        self.by_dir.contains_key(&p.to_lowercase())
    }

    fn resolve_js(&self, from: &str, spec: &str) -> Target {
        if spec.starts_with('#') || spec.contains(':') || spec.starts_with("//") {
            return Target::None; // node:fs, virtual:x, https://, #subpath
        }
        let spec = spec.split(['?', '#']).next().unwrap_or(spec);
        if spec.is_empty() {
            return Target::None;
        }
        let cfg = self.js_cfg(from);
        if spec.starts_with('.') {
            return self.local(join(dir_of(from), spec).and_then(|b| self.try_js(&b)));
        }
        if spec.starts_with('/') {
            let root = cfg.map(|c| c.dir.as_str()).unwrap_or("");
            let hit = join(root, spec).and_then(|b| self.try_js(&b)).or_else(|| join("", spec).and_then(|b| self.try_js(&b)));
            return self.local(hit);
        }
        if let Some(cfg) = cfg {
            for a in &cfg.aliases {
                let rest = if a.wild {
                    spec.strip_prefix(a.key.as_str())
                } else if spec == a.key {
                    Some("")
                } else {
                    None
                };
                if let Some(rest) = rest {
                    for t in &a.targets {
                        if let Some(i) = join(t, rest).and_then(|b| self.try_js(&b)) {
                            return Target::Local(i);
                        }
                    }
                    if a.key.starts_with(['@', '~', '$']) && a.key.len() <= 3 {
                        return Target::None; // "@/x" that we couldn't find is not an npm package
                    }
                }
            }
            if let Some(b) = &cfg.base_url {
                if let Some(i) = join(b, spec).and_then(|b| self.try_js(&b)) {
                    return Target::Local(i);
                }
            }
        }
        let mut segs = spec.split('/');
        let first = segs.next().unwrap_or(spec);
        let name = if first.starts_with('@') {
            match segs.next() {
                Some(s) => format!("{first}/{s}"),
                None => return Target::None,
            }
        } else {
            first.to_string()
        };
        Target::External(name)
    }

    fn local(&self, i: Option<usize>) -> Target {
        i.map(Target::Local).unwrap_or(Target::None)
    }

    // ---------- CSS ----------

    fn resolve_css(&self, from: &str, spec: &str) -> Target {
        if spec.contains("://") || spec.starts_with("//") {
            return Target::None;
        }
        if let Some(pkg) = spec.strip_prefix('~') {
            if !pkg.starts_with('/') {
                return self.resolve_js(from, pkg);
            }
        }
        if !spec.starts_with('.') && !spec.starts_with('/') {
            // "@/styles/x" alias, or a bare relative name like "variables"
            if let Some(i) = join(dir_of(from), spec).and_then(|b| self.try_css(&b)) {
                return Target::Local(i);
            }
            return match self.resolve_js(from, spec) {
                Target::Local(i) => Target::Local(i),
                Target::External(n) if spec.starts_with('@') && spec.len() > 2 && !spec.starts_with("@/") => Target::External(n),
                _ => Target::None,
            };
        }
        let base = if spec.starts_with('/') { join("", spec) } else { join(dir_of(from), spec) };
        self.local(base.and_then(|b| self.try_css(&b)))
    }

    fn try_css(&self, base: &str) -> Option<usize> {
        let d = dir_of(base);
        let n = file_name(base);
        let partial = if d.is_empty() { format!("_{n}") } else { format!("{d}/_{n}") };
        for b in [base, partial.as_str()] {
            for e in ["", ".scss", ".sass", ".css", ".less"] {
                if let Some(i) = self.lookup(&format!("{b}{e}")) {
                    return Some(i);
                }
            }
        }
        for e in ["/index.scss", "/_index.scss", "/index.css"] {
            if let Some(i) = self.lookup(&format!("{base}{e}")) {
                return Some(i);
            }
        }
        self.try_js(base)
    }

    // ---------- Python ----------

    fn py_module(&self, root: &str, module: &str) -> Option<usize> {
        let p = if module.is_empty() { root.to_string() } else { join(root, &module.replace('.', "/"))? };
        self.lookup(&format!("{p}.py")).or_else(|| self.lookup(&format!("{p}/__init__.py")))
    }

    /// `from <dots><module> import <names>` / `import <module>` (dots = 0).
    fn resolve_py(&self, from: &str, dots: usize, module: &str, names: &[&str]) -> Vec<Target> {
        let mut roots: Vec<String> = vec![];
        if dots > 0 {
            let mut d = dir_of(from).to_string();
            for _ in 1..dots {
                d = dir_of(&d).to_string();
            }
            roots.push(d);
        } else {
            roots.push(dir_of(from).to_string());
            roots.extend(self.py_roots.iter().cloned());
        }
        let mut out = vec![];
        for r in &roots {
            // prefer submodules: from pkg import a, b → pkg/a.py, pkg/b.py
            let mut found = false;
            for n in names {
                let m = if module.is_empty() { n.to_string() } else { format!("{module}.{n}") };
                if let Some(i) = self.py_module(r, &m) {
                    out.push(Target::Local(i));
                    found = true;
                }
            }
            if !found && !module.is_empty() {
                if let Some(i) = self.py_module(r, module) {
                    out.push(Target::Local(i));
                    found = true;
                }
            }
            if found {
                return out;
            }
        }
        if dots == 0 && !module.is_empty() {
            let top = module.split('.').next().unwrap_or(module);
            if !PY_STDLIB.contains(&top) {
                out.push(Target::External(top.to_string()));
            }
        }
        out
    }

    // ---------- Rust ----------

    fn rust_crate_dir(&self, from: &str) -> Option<String> {
        let mut d = dir_of(from).to_string();
        loop {
            let pre = if d.is_empty() { String::new() } else { format!("{d}/") };
            if self.lookup(&format!("{pre}lib.rs")).is_some() || self.lookup(&format!("{pre}main.rs")).is_some() {
                return Some(d);
            }
            if d.is_empty() {
                return None;
            }
            d = dir_of(&d).to_string();
        }
    }

    fn rust_mod_file(&self, dir: &str, segs: &[&str]) -> Option<usize> {
        if segs.is_empty() {
            let pre = if dir.is_empty() { String::new() } else { format!("{dir}/") };
            return self.lookup(&format!("{pre}lib.rs")).or_else(|| self.lookup(&format!("{pre}main.rs")));
        }
        let p = join(dir, &segs.join("/"))?;
        self.lookup(&format!("{p}.rs")).or_else(|| self.lookup(&format!("{p}/mod.rs")))
    }

    /// Longest prefix of a module path that maps to a file.
    fn rust_path(&self, crate_dir: &str, segs: &[&str]) -> Option<usize> {
        (1..=segs.len()).rev().find_map(|n| self.rust_mod_file(crate_dir, &segs[..n]))
    }

    // ---------- PHP ----------

    fn php_class(&self, fqn: &str) -> Option<usize> {
        let lower = fqn.trim_start_matches('\\').to_lowercase();
        if let Some(&i) = self.php.classes.get(&lower) {
            return Some(i);
        }
        for (prefix, dirs) in &self.php.psr4 {
            let Some(rest) = lower.strip_prefix(prefix.as_str()) else { continue };
            for d in dirs {
                if let Some(i) = join(d, &rest.replace('\\', "/")).and_then(|b| self.lookup(&format!("{b}.php"))) {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Imported class that is not in the project: composer package or nothing (global PHP class).
    fn php_external(&self, fqn: &str) -> Target {
        let fqn = fqn.trim_start_matches('\\');
        let lower = fqn.to_lowercase();
        let Some((top, _)) = lower.split_once('\\') else { return Target::None };
        if self.php.own.contains(top) {
            return Target::None;
        }
        if let Some((_, pkg)) = self.php.vendor.iter().find(|(p, _)| lower.starts_with(p.as_str())) {
            return Target::External(pkg.clone());
        }
        Target::External(fqn.split('\\').next().unwrap_or(top).to_string())
    }

    /// Laravel view name ("layouts.app") → resources/views/layouts/app.blade.php
    fn php_view(&self, name: &str, prefix: &str) -> Option<usize> {
        if name.contains("::") {
            return None; // package views
        }
        let p = format!("{prefix}{}", name.replace('.', "/"));
        for r in &self.php.view_roots {
            let Some(base) = join(r, &p) else { continue };
            for suf in [".blade.php", ".php", "/index.blade.php"] {
                if let Some(i) = self.lookup(&format!("{base}{suf}")) {
                    return Some(i);
                }
            }
        }
        None
    }

    fn blade_component(&self, name: &str) -> Option<usize> {
        if name.contains("::") || name == "dynamic-component" || name == "slot" || name.starts_with("slot:") {
            return None;
        }
        self.php_view(name, "components/")
            .or_else(|| self.php_class(&format!("App\\View\\Components\\{}", studly_path(name))))
    }

    fn livewire(&self, name: &str) -> Option<usize> {
        let s = studly_path(name);
        self.php_class(&format!("App\\Livewire\\{s}"))
            .or_else(|| self.php_class(&format!("App\\Http\\Livewire\\{s}")))
            .or_else(|| self.php_view(name, "livewire/"))
    }

    fn php_targets(&self, from: &str, text: &str, kind: Kind) -> Vec<Target> {
        let mut out = vec![];
        if kind == Kind::Php {
            let ns = re_php_ns().captures(text).map(|c| c[1].to_string()).unwrap_or_default();
            let mut aliases: HashMap<String, String> = HashMap::new();
            let mut imported: Vec<String> = vec![];
            let mut refs: Vec<String> = vec![];
            for c in re_php_use().captures_iter(text) {
                let stmt = c[1].trim();
                if stmt.starts_with("function ") || stmt.starts_with("const ") {
                    continue;
                }
                let (prefix, items) = match stmt.split_once('{') {
                    Some((p, rest)) => (p.trim().trim_matches('\\').to_string(), rest.trim_end_matches('}')),
                    None => (String::new(), stmt),
                };
                for item in items.split(',') {
                    let mut parts = item.split_whitespace();
                    let Some(name) = parts.next() else { continue };
                    if !is_php_name(name) {
                        continue;
                    }
                    let alias = match (parts.next(), parts.next()) {
                        (Some(a), Some(al)) if a.eq_ignore_ascii_case("as") => Some(al),
                        _ => None,
                    };
                    if prefix.is_empty() && !name.contains('\\') {
                        // `use HasFactory;` inside a class body (trait), or `use Exception;` (global class)
                        refs.push(name.to_string());
                        continue;
                    }
                    let full = if prefix.is_empty() {
                        name.trim_start_matches('\\').to_string()
                    } else {
                        format!("{prefix}\\{}", name.trim_start_matches('\\'))
                    };
                    let short = alias.unwrap_or_else(|| full.rsplit('\\').next().unwrap_or(&full)).to_lowercase();
                    aliases.insert(short, full.clone());
                    imported.push(full);
                }
            }
            for full in &imported {
                out.push(match self.php_class(full) {
                    Some(i) => Target::Local(i),
                    None => self.php_external(full),
                });
            }
            // same-namespace or aliased references: new X, extends X, implements A, B, X::method()
            for c in re_php_ref().captures_iter(text) {
                refs.extend(c[1].split(',').map(|s| s.trim().to_string()));
            }
            refs.extend(re_php_static().captures_iter(text).map(|c| c[1].to_string()));
            for r in refs {
                if r.is_empty() || matches!(r.to_lowercase().as_str(), "self" | "static" | "parent" | "class") {
                    continue;
                }
                let full = if let Some(abs) = r.strip_prefix('\\') {
                    abs.to_string()
                } else {
                    let (head, tail) = r.split_once('\\').map(|(h, t)| (h, Some(t))).unwrap_or((r.as_str(), None));
                    match aliases.get(&head.to_lowercase()) {
                        Some(f) => tail.map(|t| format!("{f}\\{t}")).unwrap_or_else(|| f.clone()),
                        None if ns.is_empty() => r.clone(),
                        None => format!("{ns}\\{r}"),
                    }
                };
                if let Some(i) = self.php_class(&full) {
                    out.push(Target::Local(i));
                }
            }
            // require / include
            for c in re_php_inc().captures_iter(text) {
                let spec = &c[2];
                let mut cands = vec![join(dir_of(from), spec)];
                if c.get(1).is_none() {
                    cands.push(join("", spec)); // relative to the include path / project root
                }
                if let Some(i) = cands.into_iter().flatten().find_map(|p| self.lookup(&p)) {
                    out.push(Target::Local(i));
                }
            }
            // Laravel: view('x.y'), Inertia::render('Page')
            for c in re_php_view().captures_iter(text) {
                if let Some(i) = self.php_view(&c[1], "") {
                    out.push(Target::Local(i));
                }
            }
            for c in re_inertia().captures_iter(text) {
                if let Some(i) = self.php.page_roots.iter().find_map(|r| join(r, &c[1]).and_then(|b| self.try_js(&b))) {
                    out.push(Target::Local(i));
                }
            }
            if from.to_lowercase().ends_with(".blade.php") {
                for c in re_blade_dir().captures_iter(text) {
                    let hit = if &c[1] == "livewire" { self.livewire(&c[2]) } else { self.php_view(&c[2], "") };
                    if let Some(i) = hit {
                        out.push(Target::Local(i));
                    }
                }
                for c in re_blade_x().captures_iter(text) {
                    if let Some(i) = self.blade_component(&c[1]) {
                        out.push(Target::Local(i));
                    }
                }
                for c in re_livewire_tag().captures_iter(text) {
                    if let Some(i) = self.livewire(&c[1]) {
                        out.push(Target::Local(i));
                    }
                }
            }
        }
        // Twig: {% extends 'base.html.twig' %}, $this->render('x/y.html.twig')
        for c in re_twig_ref().captures_iter(text) {
            let spec = &c[1];
            if spec.starts_with('@') {
                continue; // bundle templates
            }
            let hit = self
                .php
                .twig_roots
                .iter()
                .find_map(|r| join(r, spec).and_then(|p| self.lookup(&p)))
                .or_else(|| join(dir_of(from), spec).and_then(|p| self.lookup(&p)));
            if let Some(i) = hit {
                out.push(Target::Local(i));
            }
        }
        out
    }
}

fn is_php_name(s: &str) -> bool {
    let s = s.trim_start_matches('\\');
    s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\\')
}

/// "forms.text-input" → "Forms\TextInput"
fn studly_path(name: &str) -> String {
    name.split('.')
        .map(|seg| {
            seg.split(['-', '_'])
                .map(|w| {
                    let mut ch = w.chars();
                    ch.next().map(|f| f.to_uppercase().chain(ch).collect::<String>()).unwrap_or_default()
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\\")
}

/// composer.json autoload psr-4 + vendor packages next to it.
fn load_composer(path: &Path, dir: &str, php: &mut PhpIndex) {
    if let Some(v) = crate::detect::read_json(path) {
        for sect in ["autoload", "autoload-dev"] {
            let Some(map) = v[sect]["psr-4"].as_object() else { continue };
            for (prefix, dirs) in map {
                let list: Vec<&str> = match dirs {
                    serde_json::Value::String(s) => vec![s.as_str()],
                    serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect(),
                    _ => vec![],
                };
                let dirs: Vec<String> = list.into_iter().filter_map(|x| join(dir, x)).collect();
                let prefix = prefix.to_lowercase();
                if let Some(top) = prefix.split('\\').next().filter(|t| !t.is_empty()) {
                    php.own.insert(top.to_string());
                }
                php.psr4.push((prefix, dirs));
            }
        }
    }
    let autoload = path.with_file_name("vendor").join("composer").join("autoload_psr4.php");
    if let Ok(s) = fs::read_to_string(autoload) {
        for c in re_php_vendor().captures_iter(&s) {
            php.vendor.push((c[1].replace("\\\\", "\\").to_lowercase(), c[2].to_string()));
        }
    }
}

fn rust_children_dir(rel: &str) -> String {
    let name = file_name(rel);
    if matches!(name, "mod.rs" | "lib.rs" | "main.rs") {
        dir_of(rel).to_string()
    } else {
        rel.trim_end_matches(".rs").to_string()
    }
}

fn rust_module_segs<'a>(rel: &'a str, crate_dir: &str) -> Vec<&'a str> {
    let r = if crate_dir.is_empty() { rel } else { rel.strip_prefix(crate_dir).map(|s| s.trim_start_matches('/')).unwrap_or(rel) };
    let r = r.trim_end_matches(".rs");
    if matches!(r, "lib" | "main") {
        return vec![];
    }
    let r = r.strip_suffix("/mod").unwrap_or(r);
    r.split('/').collect()
}

const PY_STDLIB: &[&str] = &[
    "__future__", "abc", "argparse", "array", "ast", "asyncio", "base64", "bisect", "builtins", "calendar", "collections",
    "concurrent", "contextlib", "copy", "csv", "ctypes", "dataclasses", "datetime", "decimal", "difflib", "email", "enum",
    "errno", "fnmatch", "fractions", "functools", "gc", "getpass", "glob", "gzip", "hashlib", "heapq", "hmac", "html",
    "http", "importlib", "inspect", "io", "ipaddress", "itertools", "json", "logging", "math", "mimetypes",
    "multiprocessing", "operator", "os", "pathlib", "pickle", "platform", "pprint", "queue", "random", "re", "secrets",
    "select", "shlex", "shutil", "signal", "socket", "sqlite3", "ssl", "stat", "statistics", "string", "struct",
    "subprocess", "sys", "tempfile", "textwrap", "threading", "time", "timeit", "tkinter", "traceback", "types",
    "typing", "unittest", "urllib", "uuid", "warnings", "weakref", "xml", "zipfile", "zlib", "zoneinfo",
];

// ---------- config files ----------

/// tsconfig/jsconfig allow comments and trailing commas.
fn parse_jsonc(s: &str) -> Option<serde_json::Value> {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 1;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == ',' {
            let next = b[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    serde_json::from_str(&out).ok()
}

fn load_js_cfg(root: &Path, dir: &str) -> JsCfg {
    let mut cfg = JsCfg { dir: dir.to_string(), aliases: vec![], base_url: None };
    for name in ["tsconfig.json", "tsconfig.app.json", "jsconfig.json"] {
        let p = if dir.is_empty() { root.join(name) } else { root.join(dir).join(name) };
        let Some(v) = fs::read_to_string(&p).ok().and_then(|s| parse_jsonc(&s)) else { continue };
        let co = &v["compilerOptions"];
        let base = co["baseUrl"].as_str().and_then(|b| join(dir, b)).unwrap_or_else(|| dir.to_string());
        if co["baseUrl"].is_string() && cfg.base_url.is_none() {
            cfg.base_url = Some(base.clone());
        }
        if let Some(paths) = co["paths"].as_object() {
            for (k, targets) in paths {
                if cfg.aliases.iter().any(|a| a.key == k.trim_end_matches('*')) {
                    continue;
                }
                let wild = k.ends_with('*');
                let targets = targets
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str())
                            .filter_map(|t| join(&base, t.trim_end_matches('*')))
                            .collect()
                    })
                    .unwrap_or_default();
                cfg.aliases.push(Alias { key: k.trim_end_matches('*').to_string(), wild, targets });
            }
        }
    }
    let src = join(dir, "src").unwrap_or_default();
    let defaults = [
        ("@/", src.clone()),
        ("~/", src.clone()),
        ("@@/", dir.to_string()),
        ("~~/", dir.to_string()),
        ("$lib/", join(&src, "lib").unwrap_or_default()),
    ];
    for (k, t) in defaults {
        if !cfg.aliases.iter().any(|a| a.key == k) {
            cfg.aliases.push(Alias { key: k.to_string(), wild: true, targets: vec![t] });
        }
    }
    // longer keys first: "@/components/" before "@/"
    cfg.aliases.sort_by(|a, b| b.key.len().cmp(&a.key.len()));
    cfg
}

// ---------- regexes ----------

macro_rules! re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($pat).unwrap())
        }
    };
}

re!(re_js_from, r#"(?:^|[^.\w$])(?:import|export)\s+(?:type\s+)?[^'";]*?\bfrom\s*['"]([^'"\n]+)['"]"#);
re!(re_js_side, r#"(?m)^\s*import\s*['"]([^'"\n]+)['"]"#);
re!(re_js_call, r#"(?:^|[^.\w$])(?:import|require)\s*\(\s*['"]([^'"\n]+)['"]\s*\)"#);
re!(re_invoke, r#"\binvoke\s*(?:<[^>()]*>)?\s*\(\s*['"]([\w-]+)['"]"#);
re!(re_css, r#"@(?:import|use|forward)\s+(?:url\(\s*)?['"]([^'"\n]+)['"]"#);
re!(re_html_script, r#"(?i)<script\b[^>]*\bsrc\s*=\s*["']([^"']+)["']"#);
re!(re_html_link, r#"(?i)<link\b[^>]*\bhref\s*=\s*["']([^"']+\.(?:css|scss|less))["']"#);
re!(re_py_from, r#"(?m)^[ \t]*from[ \t]+(\.*)([\w.]*)[ \t]+import[ \t]+\(?([^)\n#]*)"#);
re!(re_py_import, r#"(?m)^[ \t]*import[ \t]+([\w., \t]+)"#);
re!(re_rs_mod, r#"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+(\w+)\s*;"#);
re!(re_rs_path, r#"\b(crate|super|self)((?:::\w+)+)"#);
re!(re_rs_group, r#"\b(crate|super|self)((?:::\w+)*)::\{([^}]*)\}"#);
re!(re_rs_use, r#"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?use\s+(?:::)?(\w+)"#);
re!(re_rs_cmd, r#"#\[(?:tauri::)?command[^\]]*\]\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)"#);
re!(re_go_block, r#"(?s)\bimport\s*\(([^)]*)\)"#);
re!(re_go_single, r#"\bimport\s+(?:[\w.]+\s+)?"([^"]+)""#);
re!(re_quoted, r#""([^"]+)""#);
re!(re_go_mod, r#"(?m)^module\s+(\S+)"#);
re!(re_php_ns, r#"(?m)^\s*namespace\s+([\w\\]+)\s*[;{]"#);
re!(re_php_decl, r#"(?m)^\s*(?:(?:abstract|final|readonly)\s+)*(?:class|interface|trait|enum)\s+(\w+)"#);
re!(re_php_use, r#"(?m)(?:^|[{;])[ \t]*use[ \t]+([^;(]+);"#);
re!(re_php_ref, r#"\b(?:new|extends|instanceof|implements)\s+(\\?[A-Za-z_][\w\\]*(?:\s*,\s*\\?[A-Za-z_][\w\\]*)*)"#);
re!(re_php_static, r#"(?:^|[^\w\\$>])(\\?[A-Z]\w*(?:\\\w+)*)::"#);
re!(re_php_inc, r#"\b(?:require|include)(?:_once)?\s*\(?\s*((?:__DIR__|dirname\s*\(\s*__FILE__\s*\))\s*\.\s*)?['"]([^'"$]+)['"]"#);
re!(re_php_view, r#"(?:\bview|View::make|View::first|->view)\s*\(\s*\[?\s*['"]([\w.\-/]+)['"]"#);
re!(re_inertia, r#"(?:Inertia::render|\binertia)\s*\(\s*['"]([\w./\-]+)['"]"#);
re!(re_blade_dir, r#"@(extends|include|includeIf|includeUnless|includeFirst|component|each|livewire)\s*\(\s*\[?\s*['"]([\w.\-/:]+)['"]"#);
re!(re_blade_x, r#"<x-([\w.\-:]+)"#);
re!(re_livewire_tag, r#"<livewire:([\w.\-]+)"#);
re!(re_twig_ref, r#"['"]([\w./\-@]+\.twig)['"]"#);
re!(re_php_vendor, r#"'((?:[^'\\]|\\.)+)'\s*=>\s*array\(\s*\$vendorDir\s*\.\s*'/([\w.\-]+/[\w.\-]+)"#);

// ---------- main ----------

pub fn build(root: &str) -> Result<CodeGraph, String> {
    let root_p = PathBuf::from(root);
    let walker = ignore::WalkBuilder::new(&root_p)
        .hidden(false)
        .require_git(false)
        .parents(false)
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.depth() > 0 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) && SKIP.contains(&n.as_ref()))
        })
        .build();

    let mut files: Vec<File> = vec![];
    let mut cfg_dirs: HashSet<String> = HashSet::new();
    let mut go_mods = vec![];
    let mut py_roots: Vec<String> = vec![String::new()];
    let mut php = PhpIndex::default();
    let mut truncated = false;

    for e in walker.flatten() {
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let r = rel(&root_p, e.path());
        match name.as_str() {
            "package.json" | "tsconfig.json" | "jsconfig.json" => {
                cfg_dirs.insert(dir_of(&r).to_string());
            }
            "go.mod" => {
                if let Some(m) = fs::read_to_string(e.path()).ok().and_then(|s| re_go_mod().captures(&s).map(|c| c[1].to_string())) {
                    go_mods.push((m, dir_of(&r).to_string()));
                }
            }
            "pyproject.toml" | "setup.py" => {
                let d = dir_of(&r).to_string();
                py_roots.push(join(&d, "src").unwrap_or_default());
                py_roots.push(d);
            }
            "composer.json" => load_composer(e.path(), dir_of(&r), &mut php),
            _ => {}
        }
        let Some(kind) = kind_of(&name) else { continue };
        if e.metadata().map(|m| m.len() > MAX_SIZE).unwrap_or(true) {
            continue;
        }
        if files.len() >= MAX_FILES {
            truncated = true;
            break;
        }
        files.push(File { rel: r, abs: e.path().to_path_buf(), kind });
    }
    if files.iter().any(|f| f.rel.starts_with("src/") && f.kind == Kind::Py) {
        py_roots.push("src".into());
    }
    py_roots.dedup();

    let mut by_rel = HashMap::new();
    let mut by_dir: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        by_rel.insert(f.rel.to_lowercase(), i);
        // register every ancestor dir so try_js can tell "./x" (dir) from "./x" (file)
        let mut d = dir_of(&f.rel);
        by_dir.entry(d.to_lowercase()).or_default().push(i);
        while !d.is_empty() {
            d = dir_of(d);
            by_dir.entry(d.to_lowercase()).or_default();
        }
    }
    let js_cfgs = cfg_dirs.iter().map(|d| load_js_cfg(&root_p, d)).collect();

    // read everything once
    let texts: Vec<String> = files
        .iter()
        .map(|f| fs::read(&f.abs).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default())
        .collect();

    // PHP: declared classes, template roots
    if files.iter().any(|f| matches!(f.kind, Kind::Php | Kind::Tpl)) {
        for (i, f) in files.iter().enumerate() {
            if f.kind != Kind::Php || f.rel.to_lowercase().ends_with(".blade.php") {
                continue;
            }
            let ns = re_php_ns().captures(&texts[i]).map(|c| c[1].to_string()).unwrap_or_default();
            if let Some(top) = ns.split('\\').next().filter(|t| !t.is_empty()) {
                php.own.insert(top.to_lowercase());
            }
            for c in re_php_decl().captures_iter(&texts[i]) {
                let fqn = if ns.is_empty() { c[1].to_string() } else { format!("{ns}\\{}", &c[1]) };
                php.classes.entry(fqn.to_lowercase()).or_insert(i);
            }
        }
        for d in by_dir.keys() {
            if d == "resources/views" || d.ends_with("/resources/views") {
                php.view_roots.push(d.clone());
            } else if d == "resources/js/pages" || d.ends_with("/resources/js/pages") {
                php.page_roots.push(d.clone());
            } else if file_name(d) == "templates" {
                php.twig_roots.push(d.clone());
            }
        }
        php.psr4.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        php.vendor.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    }
    let idx = Index { files, by_rel, by_dir, js_cfgs, go_mods, py_roots, php };

    // tauri commands: fn name → file
    let mut commands: HashMap<String, usize> = HashMap::new();
    for (i, f) in idx.files.iter().enumerate() {
        if f.kind == Kind::Rs && texts[i].contains("command") {
            for c in re_rs_cmd().captures_iter(&texts[i]) {
                commands.entry(c[1].to_string()).or_insert(i);
            }
        }
    }

    let mut edges: HashMap<(usize, Option<String>, usize, &'static str), u32> = HashMap::new();
    let mut ext_used: HashSet<String> = HashSet::new();
    let mut add = |from: usize, t: Target, kind: &'static str, edges: &mut HashMap<_, u32>| match t {
        Target::Local(to) if to != from => *edges.entry((from, None, to, kind)).or_insert(0) += 1,
        Target::External(name) => {
            ext_used.insert(name.clone());
            *edges.entry((from, Some(name), usize::MAX, kind)).or_insert(0) += 1;
        }
        _ => {}
    };

    for (i, f) in idx.files.iter().enumerate() {
        let text = &texts[i];
        let from = f.rel.as_str();
        match f.kind {
            Kind::Js => {
                let mut seen = HashSet::new();
                for re in [re_js_from(), re_js_side(), re_js_call()] {
                    for c in re.captures_iter(text) {
                        let spec = c[1].trim();
                        if seen.insert(spec.to_string()) {
                            add(i, idx.resolve_js(from, spec), "import", &mut edges);
                        }
                    }
                }
                if !commands.is_empty() {
                    for c in re_invoke().captures_iter(text) {
                        if let Some(&to) = commands.get(&c[1]) {
                            add(i, Target::Local(to), "ipc", &mut edges);
                        }
                    }
                }
            }
            Kind::Css => {
                for c in re_css().captures_iter(text) {
                    add(i, idx.resolve_css(from, c[1].trim()), "import", &mut edges);
                }
            }
            Kind::Html => {
                for re in [re_html_script(), re_html_link()] {
                    for c in re.captures_iter(text) {
                        let spec = c[1].trim();
                        let t = match idx.resolve_js(from, spec) {
                            Target::Local(x) => Target::Local(x),
                            _ if !spec.starts_with('.') && !spec.starts_with('/') && !spec.contains(':') => {
                                idx.local(join(dir_of(from), spec).and_then(|b| idx.try_js(&b)))
                            }
                            _ => Target::None,
                        };
                        add(i, t, "import", &mut edges);
                    }
                }
            }
            Kind::Py => {
                for c in re_py_from().captures_iter(text) {
                    let dots = c[1].len();
                    let names: Vec<&str> = c[3]
                        .split(',')
                        .filter_map(|n| n.split_whitespace().next())
                        .filter(|n| *n != "*" && n.chars().all(|ch| ch.is_alphanumeric() || ch == '_'))
                        .collect();
                    for t in idx.resolve_py(from, dots, &c[2], &names) {
                        add(i, t, "import", &mut edges);
                    }
                }
                for c in re_py_import().captures_iter(text) {
                    for m in c[1].split(',') {
                        if let Some(m) = m.split_whitespace().next() {
                            for t in idx.resolve_py(from, 0, m, &[]) {
                                add(i, t, "import", &mut edges);
                            }
                        }
                    }
                }
            }
            Kind::Rs => {
                let children = rust_children_dir(from);
                let mut seen = HashSet::new();
                for c in re_rs_mod().captures_iter(text) {
                    if let Some(to) = idx.rust_mod_file(&children, &[&c[1]]) {
                        if seen.insert(to) {
                            add(i, Target::Local(to), "import", &mut edges);
                        }
                    }
                }
                // main.rs calls its own library via the crate name (`my_lib::run()`)
                if file_name(from) == "main.rs" {
                    if let Some(lib) = idx.lookup(&format!("{children}/lib.rs").trim_start_matches('/')) {
                        add(i, Target::Local(lib), "import", &mut edges);
                    }
                }
                let Some(crate_dir) = idx.rust_crate_dir(from) else { continue };
                let self_segs = rust_module_segs(from, &crate_dir);
                let resolve = |head: &str, rest: &str| -> Option<usize> {
                    let mut segs: Vec<&str> = match head {
                        "crate" => vec![],
                        _ => self_segs.clone(),
                    };
                    let mut rest_segs = rest.split("::").filter(|s| !s.is_empty()).peekable();
                    if head == "super" {
                        segs.pop()?;
                    }
                    while rest_segs.peek() == Some(&"super") {
                        rest_segs.next();
                        segs.pop()?;
                    }
                    segs.extend(rest_segs);
                    idx.rust_path(&crate_dir, &segs)
                };
                for c in re_rs_path().captures_iter(text) {
                    if let Some(to) = resolve(&c[1], &c[2]) {
                        if seen.insert(to) {
                            add(i, Target::Local(to), "import", &mut edges);
                        }
                    }
                }
                for c in re_rs_group().captures_iter(text) {
                    for item in c[3].split(',') {
                        let item = item.trim().split(" as ").next().unwrap_or("").trim();
                        if item.is_empty() || item == "self" {
                            continue;
                        }
                        if let Some(to) = resolve(&c[1], &format!("{}::{}", &c[2], item)) {
                            if seen.insert(to) {
                                add(i, Target::Local(to), "import", &mut edges);
                            }
                        }
                    }
                }
                for c in re_rs_use().captures_iter(text) {
                    let name = &c[1];
                    if matches!(name, "crate" | "self" | "super" | "std" | "core" | "alloc") {
                        continue;
                    }
                    // 2015-style `use util::x` for a sibling module of the crate root
                    let t = match idx.rust_mod_file(&children, &[name]).or_else(|| idx.rust_mod_file(&crate_dir, &[name])) {
                        Some(to) => Target::Local(to),
                        None => Target::External(name.to_string()),
                    };
                    if let Target::Local(to) = t {
                        if !seen.insert(to) {
                            continue;
                        }
                    }
                    add(i, t, "import", &mut edges);
                }
            }
            Kind::Go => {
                let mut specs: Vec<String> = vec![];
                for c in re_go_block().captures_iter(text) {
                    specs.extend(re_quoted().captures_iter(&c[1]).map(|q| q[1].to_string()));
                }
                specs.extend(re_go_single().captures_iter(text).map(|c| c[1].to_string()));
                for spec in specs {
                    let local = idx.go_mods.iter().find_map(|(m, d)| {
                        let rest = if spec == *m { Some("") } else { spec.strip_prefix(&format!("{m}/")) };
                        rest.and_then(|r| join(d, r))
                    });
                    match local {
                        Some(dir) => {
                            for &to in idx.by_dir.get(&dir.to_lowercase()).map(|v| v.as_slice()).unwrap_or(&[]).iter().take(12) {
                                if idx.files[to].kind == Kind::Go && !idx.files[to].rel.ends_with("_test.go") {
                                    add(i, Target::Local(to), "import", &mut edges);
                                }
                            }
                        }
                        None if spec.split('/').next().is_some_and(|s| s.contains('.')) => {
                            let name = spec.split('/').take(3).collect::<Vec<_>>().join("/");
                            add(i, Target::External(name), "import", &mut edges);
                        }
                        None => {} // stdlib
                    }
                }
            }
            Kind::Php | Kind::Tpl => {
                let mut seen_local = HashSet::new();
                let mut seen_ext = HashSet::new();
                for t in idx.php_targets(from, text, f.kind) {
                    let fresh = match &t {
                        Target::Local(x) => seen_local.insert(*x),
                        Target::External(n) => seen_ext.insert(n.clone()),
                        Target::None => false,
                    };
                    if fresh {
                        add(i, t, "import", &mut edges);
                    }
                }
            }
        }
    }

    let mut nodes: Vec<GraphNode> = idx
        .files
        .iter()
        .zip(texts.iter())
        .map(|(f, t)| GraphNode {
            id: f.rel.clone(),
            path: f.abs.to_string_lossy().to_string(),
            lang: lang_of(file_name(&f.rel)).unwrap_or("Other").to_string(),
            lines: t.lines().count() as u32,
            external: false,
        })
        .collect();
    let mut ext: Vec<String> = ext_used.into_iter().collect();
    ext.sort();
    nodes.extend(ext.into_iter().map(|name| GraphNode {
        id: format!("pkg:{name}"),
        path: String::new(),
        lang: "package".into(),
        lines: 0,
        external: true,
    }));

    let mut out_edges: Vec<GraphEdge> = edges
        .into_iter()
        .map(|((from, ext, to, kind), count)| GraphEdge {
            source: idx.files[from].rel.clone(),
            target: match ext {
                Some(n) => format!("pkg:{n}"),
                None => idx.files[to].rel.clone(),
            },
            kind: kind.to_string(),
            count,
        })
        .collect();
    out_edges.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.target.cmp(&b.target)));

    Ok(CodeGraph { nodes, edges: out_edges, truncated })
}
