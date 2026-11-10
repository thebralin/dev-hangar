//! PHP-проекты: composer, фреймворки (Laravel, Symfony, Yii, CodeIgniter, CakePHP, Slim, Laminas,
//! Drupal, WordPress) и обычные сайты с index.php. Команды запуска, тесты, линтеры, зависимости.

use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::detect::{action, push_stack, read_json, Action, Dependency, Module};
use crate::util::on_path;

/// Folders that are part of a PHP site, not separate projects.
const PART_DIRS: &[&str] = &[
    "public", "public_html", "www", "web", "htdocs", "src", "app", "includes", "inc", "lib", "templates", "views",
    "assets", "static", "tests", "test", "docs", "examples", "config", "routes", "resources", "storage", "bootstrap",
    "database", "wp-content", "wp-admin", "wp-includes",
];

/// Composer event hooks: run automatically, not useful as buttons.
fn is_composer_event(name: &str) -> bool {
    name.starts_with("pre-") || name.starts_with("post-")
}

fn bin(name: &str) -> String {
    if cfg!(windows) {
        format!("vendor\\bin\\{name}")
    } else {
        format!("vendor/bin/{name}")
    }
}

fn doc_root(dir: &Path) -> Option<&'static str> {
    ["public", "web", "public_html", "www", "htdocs"]
        .into_iter()
        .find(|d| dir.join(d).join("index.php").exists())
}

fn php_serve(dir: &Path, docroot: Option<&str>, port: u16) -> String {
    let mut cmd = format!("php -S localhost:{port}");
    if let Some(d) = docroot {
        cmd.push_str(&format!(" -t {d}"));
    }
    // router script: Drupal's .ht.router.php, or a router.php/server.php in the root
    let router = docroot
        .map(|d| format!("{d}/.ht.router.php"))
        .filter(|r| dir.join(r).exists())
        .or_else(|| ["router.php", "server.php"].iter().find(|f| dir.join(f).exists()).map(|s| s.to_string()));
    if let Some(r) = router {
        cmd.push(' ');
        cmd.push_str(&r);
    }
    cmd
}

pub fn detect(root: &Path, dir: &Path, m: &mut Module, deps_out: &mut Vec<Dependency>) -> bool {
    let composer = read_json(&dir.join("composer.json"));
    let is_wp = ["wp-config.php", "wp-load.php", "wp-config-sample.php"].iter().any(|f| dir.join(f).exists());
    let has_index = dir.join("index.php").exists() || doc_root(dir).is_some();
    if composer.is_none() && !is_wp && !has_index {
        return false;
    }
    if composer.is_none() && dir != root {
        // public/index.php of a PHP project in the root is not a separate module
        let name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if PART_DIRS.contains(&name.as_str()) || root.join("composer.json").exists() || root.join("index.php").exists() {
            return false;
        }
    }

    let cwd = m.rel_path.clone();
    let cwd = cwd.as_str();
    let c = composer.unwrap_or(Value::Null);
    let has_composer = !c.is_null();
    let has_pkg = |n: &str| ["require", "require-dev"].iter().any(|k| c[*k].get(n).is_some());
    let has_file = |f: &str| dir.join(f).exists();

    push_stack(&mut m.stacks, "php", "PHP");

    // ---------- meta ----------
    if has_composer {
        if !has_file("package.json") && !has_file("Cargo.toml") {
            if let Some(n) = c["name"].as_str() {
                m.name = n.to_string();
            }
        }
        if m.version.is_none() {
            m.version = c["version"].as_str().map(String::from);
        }
        if m.description.is_none() {
            m.description = c["description"].as_str().filter(|s| !s.is_empty()).map(String::from);
        }
        m.vendor_missing = !dir.join("vendor").is_dir();
    }
    m.php_version = fs::read_to_string(dir.join(".php-version"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| c["require"]["php"].as_str().map(String::from))
        .or_else(|| c["config"]["platform"]["php"].as_str().map(String::from));
    if !on_path("php") {
        m.missing_tools.push("php".into());
    }
    if has_composer && !on_path("composer") {
        m.missing_tools.push("composer".into());
    }

    if let Some(obj) = c["scripts"].as_object() {
        for (k, v) in obj {
            if is_composer_event(k) {
                continue;
            }
            let body = match v {
                Value::String(s) => s.clone(),
                Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" && "),
                _ => String::new(),
            };
            m.composer_scripts.push((k.clone(), body));
        }
    }
    let cs = |n: &str| m.composer_scripts.iter().any(|(k, _)| k == n);

    // ---------- frameworks ----------
    let laravel = has_file("artisan") || has_pkg("laravel/framework");
    let symfony = !laravel && (has_pkg("symfony/framework-bundle") || has_file("symfony.lock"));
    let yii = has_pkg("yiisoft/yii2");
    let ci = has_pkg("codeigniter4/framework") || has_file("spark");
    let cake = has_pkg("cakephp/cakephp");
    let slim = has_pkg("slim/slim");
    let laminas = has_pkg("laminas/laminas-mvc") || has_pkg("mezzio/mezzio");
    let drupal = has_pkg("drupal/core") || has_pkg("drupal/core-recommended");
    let wp = is_wp || has_pkg("roots/wordpress") || has_pkg("johnpbloch/wordpress") || dir.join("wp-content").is_dir();

    let st = &mut m.stacks;
    for (on, id, label) in [
        (laravel, "laravel", "Laravel"),
        (symfony, "symfony", "Symfony"),
        (yii, "yii", "Yii"),
        (ci, "codeigniter", "CodeIgniter"),
        (cake, "cakephp", "CakePHP"),
        (slim, "slim", "Slim"),
        (laminas, "laminas", "Laminas"),
        (drupal, "drupal", "Drupal"),
        (wp, "wordpress", "WordPress"),
        (has_pkg("livewire/livewire"), "livewire", "Livewire"),
        (has_pkg("inertiajs/inertia-laravel"), "inertia", "Inertia"),
        (has_pkg("filament/filament"), "filament", "Filament"),
        (has_pkg("doctrine/orm"), "doctrine", "Doctrine"),
    ] {
        if on {
            push_stack(st, id, label);
        }
    }

    // ---------- run ----------
    let mut dev: Vec<Action> = vec![];
    let mut more: Vec<Action> = vec![];
    let artisan = |id: &str, label: &str, args: &str, kind: &str| action(id, label, format!("php artisan {args}"), kind, cwd);

    if laravel {
        if cs("dev") {
            dev.push(action("composer-dev", "Dev (composer dev)", "composer run dev".into(), "dev", cwd));
        }
        dev.push(artisan("artisan-serve", "artisan serve", "serve", "dev"));
        if !has_file(".env") && has_file(".env.example") {
            let cp = if cfg!(windows) { "copy .env.example .env" } else { "cp .env.example .env" };
            more.push(action("laravel-env", "Создать .env", format!("{cp} && php artisan key:generate"), "install", cwd));
        }
        more.push(artisan("artisan-migrate", "migrate", "migrate", "script"));
        more.push(artisan("artisan-queue", "queue:listen", "queue:listen --tries=1", "serve"));
        more.push(artisan("artisan-schedule", "schedule:work", "schedule:work", "serve"));
        more.push(artisan("artisan-routes", "route:list", "route:list", "script"));
        more.push(artisan("artisan-clear", "optimize:clear", "optimize:clear", "script"));
        more.push(artisan("artisan-test", "artisan test", "test", "test"));
    } else {
        // project-defined serve script wins (e.g. Slim skeleton: "start": "php -S … -t public")
        if let Some(s) = ["dev", "serve", "start"].into_iter().find(|s| cs(s)) {
            dev.push(action(&format!("composer-{s}"), &format!("Dev (composer {s})"), format!("composer run {s}"), "dev", cwd));
        }
        let docroot = doc_root(dir);
        if symfony {
            if on_path("symfony") {
                dev.push(action("symfony-serve", "symfony serve", "symfony server:start".into(), "dev", cwd));
            }
            dev.push(action("php-serve", "php -S", php_serve(dir, docroot.or(Some("public")), 8000), "dev", cwd));
            if has_file("bin/console") {
                more.push(action("sf-cache", "cache:clear", "php bin/console cache:clear".into(), "script", cwd));
                more.push(action("sf-routes", "debug:router", "php bin/console debug:router".into(), "script", cwd));
                if has_pkg("doctrine/doctrine-migrations-bundle") {
                    more.push(action(
                        "sf-migrate",
                        "migrations:migrate",
                        "php bin/console doctrine:migrations:migrate --no-interaction".into(),
                        "script",
                        cwd,
                    ));
                }
            }
        } else if yii && has_file("yii") {
            dev.push(action("yii-serve", "yii serve", "php yii serve".into(), "dev", cwd));
            more.push(action("yii-migrate", "yii migrate", "php yii migrate --interactive=0".into(), "script", cwd));
        } else if ci && has_file("spark") {
            dev.push(action("spark-serve", "spark serve", "php spark serve".into(), "dev", cwd));
            more.push(action("spark-migrate", "spark migrate", "php spark migrate".into(), "script", cwd));
            more.push(action("spark-routes", "spark routes", "php spark routes".into(), "script", cwd));
        } else if cake && has_file("bin/cake.php") {
            dev.push(action("cake-serve", "cake server", "php bin/cake.php server".into(), "dev", cwd));
            if has_pkg("cakephp/migrations") {
                more.push(action("cake-migrate", "migrations migrate", "php bin/cake.php migrations migrate".into(), "script", cwd));
            }
        } else if wp && on_path("wp") {
            dev.push(action("wp-serve", "wp server", "wp server".into(), "dev", cwd));
        }
        if !dev.iter().any(|a| a.id.ends_with("php-serve")) {
            let docroot = if drupal { docroot.or(Some("web")) } else { docroot };
            dev.push(action("php-serve", "php -S", php_serve(dir, docroot, 8000), "dev", cwd));
        }
        if drupal && has_pkg("drush/drush") {
            more.push(action("drush-cr", "drush cr", format!("{} cr", bin("drush")), "script", cwd));
        }

        // tests
        if cs("test") {
            more.push(action("composer-test", "composer test", "composer test".into(), "test", cwd));
        } else if has_pkg("pestphp/pest") {
            more.push(action("pest", "pest", bin("pest"), "test", cwd));
        } else if has_file("bin/phpunit") {
            more.push(action("phpunit", "phpunit", "php bin/phpunit".into(), "test", cwd));
        } else if has_pkg("phpunit/phpunit") || has_file("phpunit.xml") || has_file("phpunit.xml.dist") {
            more.push(action("phpunit", "phpunit", bin("phpunit"), "test", cwd));
        }
    }

    // ---------- quality tools ----------
    if has_pkg("phpstan/phpstan") || has_pkg("larastan/larastan") || has_pkg("nunomaduro/larastan") {
        more.push(action("phpstan", "phpstan", format!("{} analyse", bin("phpstan")), "script", cwd));
    }
    if has_pkg("laravel/pint") {
        more.push(action("pint", "pint", bin("pint"), "script", cwd));
    }
    if has_pkg("friendsofphp/php-cs-fixer") {
        more.push(action("cs-fixer", "php-cs-fixer", format!("{} fix --dry-run --diff", bin("php-cs-fixer")), "script", cwd));
    }
    if has_pkg("vimeo/psalm") {
        more.push(action("psalm", "psalm", bin("psalm"), "script", cwd));
    }
    if has_pkg("rector/rector") {
        more.push(action("rector", "rector (dry-run)", format!("{} process --dry-run", bin("rector")), "script", cwd));
    }

    // ---------- install ----------
    if has_composer {
        more.push(action("composer-install", "composer install", "composer install".into(), "install", cwd));
        more.push(action("composer-update", "composer update", "composer update".into(), "script", cwd));
        more.push(action("composer-dump", "dump-autoload", "composer dump-autoload".into(), "script", cwd));
    }

    // PHP is the app server: its dev commands go first, a JS dev server (Vite) becomes "Frontend …"
    if !dev.is_empty() {
        for a in m.actions.iter_mut().filter(|a| a.kind == "dev") {
            if !a.label.starts_with("Frontend") {
                a.label = format!("Frontend {}", a.label);
            }
        }
        m.actions.splice(0..0, dev);
    }
    // skip commands that are already there
    for a in more {
        if !m.actions.iter().any(|x| x.command == a.command) {
            m.actions.push(a);
        }
    }

    // ---------- dependencies ----------
    for (key, dev) in [("require", false), ("require-dev", true)] {
        if let Some(obj) = c[key].as_object() {
            for (n, v) in obj {
                // platform requirements (php, ext-mbstring, composer-plugin-api) are not packages
                if !n.contains('/') {
                    continue;
                }
                deps_out.push(Dependency {
                    name: n.clone(),
                    version: v.as_str().unwrap_or_default().into(),
                    dev,
                    source: "composer".into(),
                    scope: m.rel_path.clone(),
                });
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("devhangar-php-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (p, body) in files {
            let f = root.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, body).unwrap();
        }
        root
    }

    #[test]
    fn laravel_project() {
        let root = fixture(
            "laravel",
            &[
                (
                    "composer.json",
                    r#"{"name":"acme/shop","require":{"php":"^8.2","ext-mbstring":"*","laravel/framework":"^11.0","livewire/livewire":"^3"},
                       "require-dev":{"pestphp/pest":"^2","laravel/pint":"^1"},
                       "autoload":{"psr-4":{"App\\":"app/"}},
                       "scripts":{"dev":"npx concurrently \"php artisan serve\" \"npm run dev\"","post-autoload-dump":"x"}}"#,
                ),
                ("package.json", r#"{"scripts":{"dev":"vite","build":"vite build"},"devDependencies":{"vite":"^5"}}"#),
                ("artisan", "<?php"),
                (".env.example", "APP_KEY="),
                ("public/index.php", "<?php"),
                (
                    "app/Http/Controllers/ShopController.php",
                    "<?php\nnamespace App\\Http\\Controllers;\n\nuse App\\Models\\{Product, Order as O};\nuse Illuminate\\Http\\Request;\n\nclass ShopController extends Controller\n{\n    public function index() { Product::all(); return view('shop.index'); }\n}\n",
                ),
                ("app/Http/Controllers/Controller.php", "<?php\nnamespace App\\Http\\Controllers;\n\nabstract class Controller {}\n"),
                ("app/Models/Product.php", "<?php\nnamespace App\\Models;\n\nclass Product { use HasPrice; }\n"),
                ("app/Models/HasPrice.php", "<?php\nnamespace App\\Models;\n\ntrait HasPrice {}\n"),
                ("app/Models/Order.php", "<?php\nnamespace App\\Models;\nclass Order {}\n"),
                ("app/Livewire/CartCounter.php", "<?php\nnamespace App\\Livewire;\nclass CartCounter {}\n"),
                ("resources/views/layouts/app.blade.php", "<html>@yield('content')</html>"),
                ("resources/views/components/product-card.blade.php", "<div></div>"),
                (
                    "resources/views/shop/index.blade.php",
                    "@extends('layouts.app')\n@section('content')<x-product-card /> <livewire:cart-counter />@endsection",
                ),
                ("routes/web.php", "<?php\nuse App\\Http\\Controllers\\ShopController;\nRoute::get('/', [ShopController::class, 'index']);\nrequire __DIR__.'/auth.php';\n"),
                ("routes/auth.php", "<?php\n"),
            ],
        );
        let info = crate::detect::analyze(root.to_str().unwrap()).unwrap();
        let ids: Vec<&str> = info.stacks.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"php") && ids.contains(&"laravel") && ids.contains(&"livewire"), "{ids:?}");
        assert!(info.subprojects.is_empty(), "public/ must not be a subproject: {:?}", info.subprojects.iter().map(|m| &m.rel_path).collect::<Vec<_>>());
        let m = &info.root;
        let cmds: Vec<&str> = m.actions.iter().map(|a| a.command.as_str()).collect();
        assert_eq!(m.actions[0].command, "composer run dev", "{cmds:?}");
        assert_eq!(m.actions[1].command, "php artisan serve");
        assert!(m.actions.iter().any(|a| a.kind == "dev" && a.command == "npm run dev" && a.label.starts_with("Frontend")), "{cmds:?}");
        assert!(cmds.iter().any(|c| c.contains(".env.example") && c.contains("key:generate")), "{cmds:?}");
        assert!(cmds.contains(&"php artisan test") && cmds.contains(&"composer install"), "{cmds:?}");
        assert_eq!(m.php_version.as_deref(), Some("^8.2"));
        assert!(m.vendor_missing);
        assert_eq!(m.composer_scripts.len(), 1, "post-* hooks must be skipped");
        let deps: Vec<&str> = info.dependencies.iter().filter(|d| d.source == "composer").map(|d| d.name.as_str()).collect();
        assert!(deps.contains(&"laravel/framework") && !deps.contains(&"php") && !deps.contains(&"ext-mbstring"), "{deps:?}");

        let g = crate::graph::build(root.to_str().unwrap()).unwrap();
        let has = |a: &str, b: &str| g.edges.iter().any(|e| e.source == a && e.target == b);
        let ctrl = "app/Http/Controllers/ShopController.php";
        for (a, b) in [
            (ctrl, "app/Models/Product.php"),
            (ctrl, "app/Models/Order.php"),
            (ctrl, "app/Http/Controllers/Controller.php"),
            (ctrl, "resources/views/shop/index.blade.php"),
            (ctrl, "pkg:Illuminate"),
            ("app/Models/Product.php", "app/Models/HasPrice.php"),
            ("routes/web.php", ctrl),
            ("routes/web.php", "routes/auth.php"),
            ("resources/views/shop/index.blade.php", "resources/views/layouts/app.blade.php"),
            ("resources/views/shop/index.blade.php", "resources/views/components/product-card.blade.php"),
            ("resources/views/shop/index.blade.php", "app/Livewire/CartCounter.php"),
        ] {
            assert!(has(a, b), "missing edge {a} -> {b}\n{:#?}", g.edges.iter().map(|e| format!("{} -> {}", e.source, e.target)).collect::<Vec<_>>());
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn plain_php_site() {
        let root = fixture(
            "plain",
            &[
                ("index.php", "<?php require_once 'inc/db.php'; include __DIR__ . '/inc/header.php';"),
                ("inc/db.php", "<?php"),
                ("inc/header.php", "<?php"),
            ],
        );
        let info = crate::detect::analyze(root.to_str().unwrap()).unwrap();
        assert!(info.stacks.iter().any(|s| s.id == "php"));
        assert!(info.subprojects.is_empty());
        assert_eq!(info.root.actions[0].command, "php -S localhost:8000");
        assert!(!info.root.vendor_missing);
        let g = crate::graph::build(root.to_str().unwrap()).unwrap();
        assert!(g.edges.iter().any(|e| e.source == "index.php" && e.target == "inc/db.php"));
        assert!(g.edges.iter().any(|e| e.source == "index.php" && e.target == "inc/header.php"));
        let _ = fs::remove_dir_all(&root);
    }
}
