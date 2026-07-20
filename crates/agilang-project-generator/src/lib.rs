use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

pub fn generate_project(name: &str, template: &str) -> Result<()> {
    let path = PathBuf::from(name);
    if path.exists() {
        bail!("directory `{}` already exists", name);
    }

    // Folders to create
    let dirs = [
        "app/Controllers",
        "app/Models",
        "app/Services",
        "app/Middleware",
        "app/Providers",
        "app/Console/Commands",
        "routes",
        "resources/views/layouts",
        "resources/css",
        "resources/js",
        "resources/assets",
        "public",
        "config",
        "database/migrations",
        "database/seeders",
        "database/factories",
        "storage/logs",
        "storage/cache",
        "storage/sessions",
        "storage/uploads",
        "tests/Feature",
        "tests/Unit",
        "bootstrap",
        "build",
    ];

    for d in &dirs {
        fs::create_dir_all(path.join(d))?;
    }

    // Write agilang.toml
    let toml = format!(
        "[project]\n\
         name = \"{}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2026\"\n\
         type = \"{}\"\n\n\
         [application]\n\
         entry = \"bootstrap/app.agi\"\n\
         environment = \"local\"\n\n\
         [server]\n\
         host = \"127.0.0.1\"\n\
         port = 8080\n\n\
         [routes]\n\
         web = \"routes/web.agi\"\n\
         api = \"routes/api.agi\"\n\n\
         [views]\n\
         path = \"resources/views\"\n\
         cache = \"storage/cache/views\"\n\n\
         [public]\n\
         path = \"public\"\n\n\
         [build]\n\
         target = \"native\"\n\
         output = \"build/{}.exe\"\n",
        name, template, name
    );
    fs::write(path.join("agilang.toml"), toml)?;

    // Write .gitignore
    fs::write(
        path.join(".gitignore"),
        "/target\n/build\nstorage/cache/*\n",
    )?;

    // Write README.md
    fs::write(
        path.join("README.md"),
        format!(
            "# {}\n\nCreated with AGILANG CLI utilizing the `{}` template.\n",
            name, template
        ),
    )?;

    // Write bootstrap/app.agi
    let bootstrap_agi = "\
fn main() -> i32:\n\
    print(\"Bootstrapping AGILANG Framework App\")\n\
    return 0\n";
    fs::write(path.join("bootstrap/app.agi"), bootstrap_agi)?;

    // Write app/Controllers/HomeController.agi
    let home_controller = "\
fn index() -> i32:\n\
    print(\"HomeController.index called\")\n\
    return 0\n";
    fs::write(
        path.join("app/Controllers/HomeController.agi"),
        home_controller,
    )?;

    // Write routes/web.agi
    fs::write(
        path.join("routes/web.agi"),
        "fn register_web() -> i32:\n    print(\"web routes registered\")\n    return 0\n",
    )?;

    // Write routes/api.agi
    fs::write(
        path.join("routes/api.agi"),
        "fn register_api() -> i32:\n    print(\"api routes registered\")\n    return 0\n",
    )?;

    // Write resources/views/home.ags
    let home_ags = "\
@extends(\"layouts/app\")\n\n\
@section(\"content\")\n\
    <main class=\"hero\">\n\
        <h1>Hello from AGS</h1>\n\
    </main>\n\
@endsection\n";
    fs::write(path.join("resources/views/home.ags"), home_ags)?;

    // Write resources/views/layouts/app.ags
    let app_ags = "\
<!DOCTYPE html>\n\
<html>\n\
<head>\n\
    <title>AGILANG App</title>\n\
</head>\n\
<body>\n\
    @yield(\"content\")\n\
</body>\n\
</html>\n";
    fs::write(path.join("resources/views/layouts/app.ags"), app_ags)?;

    // Write public/index.html
    fs::write(
        path.join("public/index.html"),
        "<h1>Hello from AGILANG Public Folder</h1>\n",
    )?;

    Ok(())
}

pub fn make_component(component: &str, name: &str) -> Result<()> {
    // Determine project root by looking for agilang.toml in current or parents
    let mut dir = std::env::current_dir()?;
    let mut root = None;
    loop {
        if dir.join("agilang.toml").exists() {
            root = Some(dir.clone());
            break;
        }
        if !dir.pop() {
            break;
        }
    }

    let project_root = match root {
        Some(r) => r,
        None => bail!("not in an AGILANG project (agilang.toml not found)"),
    };

    match component {
        "controller" => {
            let file_name = if name.ends_with("Controller") {
                format!("{}.agi", name)
            } else {
                format!("{}Controller.agi", name)
            };
            let path = project_root.join("app/Controllers").join(&file_name);
            if path.exists() {
                bail!("controller `{}` already exists", file_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                &path,
                format!("# {}\nfn index() -> i32:\n    return 0\n", name),
            )?;
            println!("Created controller app/Controllers/{}", file_name);
        }
        "model" => {
            let path = project_root
                .join("app/Models")
                .join(format!("{}.agi", name));
            if path.exists() {
                bail!("model `{}` already exists", name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, format!("# {}\n", name))?;
            println!("Created model app/Models/{}.agi", name);
        }
        "service" => {
            let file_name = if name.ends_with("Service") {
                format!("{}.agi", name)
            } else {
                format!("{}Service.agi", name)
            };
            let path = project_root.join("app/Services").join(&file_name);
            if path.exists() {
                bail!("service `{}` already exists", file_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, format!("# {}\n", name))?;
            println!("Created service app/Services/{}", file_name);
        }
        "middleware" => {
            let path = project_root
                .join("app/Middleware")
                .join(format!("{}.agi", name));
            if path.exists() {
                bail!("middleware `{}` already exists", name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, format!("# {}\n", name))?;
            println!("Created middleware app/Middleware/{}.agi", name);
        }
        "provider" => {
            let file_name = if name.ends_with("ServiceProvider") {
                format!("{}.agi", name)
            } else {
                format!("{}ServiceProvider.agi", name)
            };
            let path = project_root.join("app/Providers").join(&file_name);
            if path.exists() {
                bail!("provider `{}` already exists", file_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, format!("# {}\n", name))?;
            println!("Created provider app/Providers/{}", file_name);
        }
        "view" => {
            let path = project_root
                .join("resources/views")
                .join(format!("{}.ags", name));
            if path.exists() {
                bail!("view `{}` already exists", name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, "<!-- View template -->\n")?;
            println!("Created view resources/views/{}.ags", name);
        }
        "route" => {
            let path = project_root.join("routes").join(format!("{}.agi", name));
            if path.exists() {
                bail!("route file `{}` already exists", name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, "# Routes\n")?;
            println!("Created routes/{}.agi", name);
        }
        "migration" => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let file_name = format!("{}_{}.agi", timestamp, name);
            let path = project_root.join("database/migrations").join(&file_name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, "# Migration\n")?;
            println!("Created migration database/migrations/{}", file_name);
        }
        "test" => {
            let path = project_root
                .join("tests/Feature")
                .join(format!("{}Test.agi", name));
            if path.exists() {
                bail!("test `{}` already exists", name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                &path,
                format!("fn test_{}() -> i32:\n    return 0\n", name.to_lowercase()),
            )?;
            println!("Created test tests/Feature/{}Test.agi", name);
        }
        _ => bail!("unknown framework component `{}`", component),
    }

    Ok(())
}
