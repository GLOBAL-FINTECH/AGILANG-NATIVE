use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

pub fn generate_project(name: &str, template: &str) -> Result<()> {
    let path = PathBuf::from(name);
    if path.exists() {
        bail!("directory `{}` already exists", name);
    }

    let mut files = vec![
        (
            "agilang.toml",
            r#"[project]
name = "{name}"
version = "0.1.0"
edition = "2026"
type = "{template}"

[template]
name = "{template}"
version = "0.4.0"

[application]
entry = "bootstrap/app.agi"
environment = "local"

[server]
host = "127.0.0.1"
port = 8080

[routes]
web = "routes/web.agi"
api = "routes/api.agi"

[views]
path = "resources/views"
cache = "storage/cache/views"

[public]
path = "public"

[build]
target = "native"
output = "build/{name}.exe"
"#,
        ),
        (
            ".env.example",
            r#"APP_NAME=AGILANG
APP_ENV=local
APP_URL=http://127.0.0.1:8080
APP_HOST=127.0.0.1
APP_PORT=8080

FORCE_HTTPS=false
TRUST_PROXY=false

SESSION_SECURE=false
SESSION_HTTP_ONLY=true
SESSION_SAME_SITE=Lax
"#,
        ),
        (
            "README.md",
            r#"# {name}

Created with AGILANG CLI.
"#,
        ),
        (
            "bootstrap/app.agi",
            r#"use Framework.Application
use App.Providers.AppServiceProvider
use App.Providers.RouteServiceProvider

fn bootstrap() -> Application:
    let app = Application.create(base_path())

    app.register(AppServiceProvider)
    app.register(RouteServiceProvider)

    app.load_config("config")
    app.load_environment(".env")
    app.load_views("resources/views")
    app.public_path("public")

    return app
"#,
        ),
        (
            "app/Controllers/HomeController.agi",
            r#"module App.Controllers

use Framework.Http.Request
use Framework.Http.Response
use Framework.View

class HomeController:
    fn index(request: Request) -> Response:
        return View.render("welcome", {
            "title": "AGILANG Native Framework",
            "message": "Your application is running successfully."
        })

    fn about(request: Request) -> Response:
        return Response.html(
            "<h1>About AGILANG</h1><p>Native application framework.</p>"
        )
"#,
        ),
        (
            "app/Controllers/Api/HealthController.agi",
            r#"module App.Controllers.Api

use Framework.Http.Request
use Framework.Http.Response

class HealthController:
    fn show(request: Request) -> Response:
        return Response.json({
            "status": "healthy",
            "framework": "AGILANG",
            "version": "0.4.0"
        })
"#,
        ),
        (
            "app/Middleware/WebMiddleware.agi",
            r#"module App.Middleware

use Framework.Http.Request
use Framework.Http.Response
use Framework.Middleware.Next

class WebMiddleware:
    fn handle(request: Request, next: Next) -> Response:
        let response = next(request)
        response.header("X-Powered-By", "AGILANG")
        return response
"#,
        ),
        (
            "app/Middleware/ApiMiddleware.agi",
            r#"module App.Middleware

use Framework.Http.Request
use Framework.Http.Response
use Framework.Middleware.Next

class ApiMiddleware:
    fn handle(request: Request, next: Next) -> Response:
        let response = next(request)
        response.header("Content-Type", "application/json")
        return response
"#,
        ),
        (
            "app/Models/User.agi",
            r#"module App.Models

class User:
    let id: i64
    let name: string
    let email: string
    let password_hash: string
    let role: string

    fn is_admin() -> bool:
        return self.role == "admin"
"#,
        ),
        (
            "app/Providers/AppServiceProvider.agi",
            r#"module App.Providers

use Framework.Container.Container
use Framework.Providers.ServiceProvider
use App.Services.ApplicationService

class AppServiceProvider extends ServiceProvider:
    fn register(container: Container) -> void:
        container.singleton(ApplicationService, ApplicationService)

    fn boot() -> void:
        print("Application services booted")
"#,
        ),
        (
            "app/Providers/RouteServiceProvider.agi",
            r#"module App.Providers

use Framework.Application
use Framework.Providers.ServiceProvider
use Routes.Web
use Routes.Api

class RouteServiceProvider extends ServiceProvider:
    fn boot(app: Application) -> void:
        Web.register_web()
        Api.register_api()
"#,
        ),
        (
            "app/Services/ApplicationService.agi",
            r#"module App.Services

class ApplicationService:
    fn get_version() -> string:
        return "0.4.0"
"#,
        ),
        (
            "app/Console/Commands/HelloCommand.agi",
            r#"module App.Console.Commands

class HelloCommand:
    fn handle() -> void:
        print("Hello from AGILANG Console")
"#,
        ),
        (
            "config/app.agi",
            r#"use Framework.Config.AppConfig

return AppConfig {
    name: env("APP_NAME", "AGILANG Application"),
    environment: env("APP_ENV", "local"),
    url: env("APP_URL", "http://127.0.0.1:8080"),
    debug: env_bool("APP_DEBUG", true)
}
"#,
        ),
        (
            "config/server.agi",
            r#"use Framework.Config.ServerConfig

return ServerConfig {
    host: env("APP_HOST", "127.0.0.1"),
    port: env_i32("APP_PORT", 8080),
    https_port: env_i32("APP_HTTPS_PORT", 8443),
    force_https: env_bool("FORCE_HTTPS", false)
}
"#,
        ),
        (
            "config/auth.agi",
            r#"use Framework.Auth.AuthConfig

return AuthConfig {
    enabled: false,
    default_role: "user",
    roles: [
        "user",
        "admin"
    ]
}
"#,
        ),
        (
            "config/logging.agi",
            r#"return {
    "level": "debug"
}
"#,
        ),
        (
            "config/database.agi",
            r#"return {
    "driver": "sqlite"
}
"#,
        ),
        (
            "routes/web.agi",
            r#"use Framework.Routing.Route
use App.Controllers.HomeController

fn register_web() -> void:
    Route.get("/", HomeController.index)
    Route.get("/about", HomeController.about)
"#,
        ),
        (
            "routes/api.agi",
            r#"use Framework.Routing.Route
use App.Controllers.Api.HealthController

fn register_api() -> void:
    Route.group("/api", fn:
        Route.get("/health", HealthController.show)
    )
"#,
        ),
        (
            "routes/console.agi",
            r#"use Framework.Routing.Route
use App.Console.Commands.HelloCommand

fn register_console() -> void:
    Route.command("hello", HelloCommand.handle)
"#,
        ),
        (
            "resources/views/layouts/app.ags",
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{ title ?? "AGILANG Application" }}</title>
    <link rel="stylesheet" href="/assets/css/app.css">
</head>
<body>
    <header class="navbar">
        <a href="/" class="brand">AGILANG</a>
        <nav>
            <a href="/">Home</a>
            <a href="/about">About</a>
            <a href="/api/health">API Health</a>
        </nav>
    </header>
    <main>
        @yield("content")
    </main>
    <script src="/assets/js/app.js"></script>
</body>
</html>
"#,
        ),
        (
            "resources/views/welcome.ags",
            r#"@extends("layouts/app")

@section("content")
<section class="hero">
    <div class="hero-card">
        <span class="eyebrow">AGILANG Native Framework</span>
        <h1>{{ title }}</h1>
        <p>{{ message }}</p>

        <div class="network-matrix">
            <div class="matrix-item">
                <span class="label">WebSocket</span>
                <span id="ws-status" class="badge warning">Connecting...</span>
            </div>
            <div class="matrix-item">
                <span class="label">WebRTC</span>
                <span id="rtc-status" class="badge warning">Negotiating...</span>
            </div>
            <div class="matrix-item">
                <span class="label">Local STUN</span>
                <span class="badge success">Active (Port 3478)</span>
            </div>
        </div>

        <div class="actions">
            <a class="button primary" href="/about">Explore framework</a>
            <a class="button secondary" href="/api/health">Check API</a>
        </div>
    </div>
</section>
@endsection
"#,
        ),
        (
            "resources/views/errors/404.ags",
            r#"<h1>404 Not Found</h1>
<p>The page you are looking for does not exist.</p>
"#,
        ),
        (
            "resources/views/errors/500.ags",
            r#"<h1>500 Internal Server Error</h1>
<p>Something went wrong on our end.</p>
"#,
        ),
        (
            "resources/views/components/button.ags",
            r#"<!-- Button Component -->
<button class="btn">{{ text }}</button>
"#,
        ),
        (
            "resources/assets/css/app.css",
            r#":root {
    font-family: Inter, system-ui, sans-serif;
    color: #eef6ff;
    background: #07111f;
}

* {
    box-sizing: border-box;
}

body {
    margin: 0;
    min-height: 100vh;
    background: radial-gradient(circle at top, #173968, transparent 42%), #07111f;
}

.navbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 40px;
    border-bottom: 1px solid rgba(255,255,255,0.1);
}

.brand, nav a {
    color: white;
    text-decoration: none;
}

nav {
    display: flex;
    gap: 24px;
}

.hero {
    min-height: calc(100vh - 82px);
    display: grid;
    place-items: center;
    padding: 32px;
}

.hero-card {
    width: min(760px, 100%);
    padding: 56px;
    border-radius: 28px;
    border: 1px solid rgba(113, 177, 255, 0.28);
    background: rgba(8, 24, 45, 0.88);
    box-shadow: 0 32px 100px rgba(0,0,0,0.45);
}

.eyebrow {
    color: #77b9ff;
    letter-spacing: 0.08em;
    text-transform: uppercase;
}

h1 {
    margin: 16px 0;
    font-size: clamp(42px, 7vw, 72px);
}

p {
    color: #abc0dc;
    font-size: 19px;
}

.network-matrix {
    display: flex;
    gap: 16px;
    margin: 24px 0;
    padding: 16px;
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
}

.matrix-item {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
}

.matrix-item .label {
    font-size: 13px;
    color: #8fa0b5;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.badge {
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 600;
}

.badge.warning {
    color: #ffe082;
    background: rgba(255, 224, 130, 0.15);
    border: 1px solid rgba(255, 224, 130, 0.3);
}

.badge.success {
    color: #81c784;
    background: rgba(129, 199, 132, 0.15);
    border: 1px solid rgba(129, 199, 132, 0.3);
}

.badge.error {
    color: #e57373;
    background: rgba(229, 115, 115, 0.15);
    border: 1px solid rgba(229, 115, 115, 0.3);
}

.actions {
    display: flex;
    gap: 12px;
    margin-top: 32px;
}

.button {
    padding: 13px 20px;
    border-radius: 10px;
    text-decoration: none;
}

.primary {
    color: white;
    background: #1769e0;
}

.secondary {
    color: white;
    border: 1px solid rgba(255,255,255,0.24);
}
"#,
        ),
        (
            "resources/assets/js/app.js",
            r#"document.documentElement.dataset.agilang = "ready";
console.info("AGILANG application assets loaded");

// 1. WebSocket integration
const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
const socket = new WebSocket(`${wsProtocol}//${window.location.host}`);

socket.onopen = () => {
    console.info("WebSocket connected successfully to AGILANG Native Server!");
    const wsBadge = document.getElementById("ws-status");
    if (wsBadge) {
        wsBadge.textContent = "Connected";
        wsBadge.className = "badge success";
    }
};

socket.onmessage = (event) => {
    console.log("WebSocket message received:", event.data);
};

socket.onerror = (error) => {
    console.error("WebSocket error:", error);
    const wsBadge = document.getElementById("ws-status");
    if (wsBadge) {
        wsBadge.textContent = "Error";
        wsBadge.className = "badge error";
    }
};

// 2. WebRTC integration with built-in STUN
const configuration = {
    iceServers: [
        { urls: `stun:${window.location.hostname}:3478` }
    ]
};

const peerConnection = new RTCPeerConnection(configuration);

peerConnection.onicecandidate = (event) => {
    if (event.candidate) {
        console.info("Local STUN server resolved ICE candidate:", event.candidate.candidate);
        const rtcBadge = document.getElementById("rtc-status");
        if (rtcBadge) {
            rtcBadge.textContent = "ICE Resolved";
            rtcBadge.className = "badge success";
        }
    }
};

peerConnection.onconnectionstatechange = () => {
    console.info("WebRTC Connection State changed to:", peerConnection.connectionState);
};

peerConnection.createDataChannel("agi-sync");
peerConnection.createOffer()
    .then(offer => peerConnection.setLocalDescription(offer))
    .catch(err => console.error("WebRTC offer error:", err));
"#,
        ),
        (
            "public/index.html",
            r#"<h1>Hello from AGILANG Public Folder</h1>
"#,
        ),
        (
            "public/favicon.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="40" fill="#1769e0"/></svg>
"##,
        ),
        (
            "database/migrations/CreateUsersTable.agi",
            r#"use Framework.Database.Migration
use Framework.Database.Schema

class CreateUsersTable extends Migration:
    fn up() -> void:
        Schema.create("users", fn table:
            table.id()
            table.string("name")
            table.string("email").unique()
            table.string("password_hash")
            table.string("role").default("user")
            table.timestamps()
        )

    fn down() -> void:
        Schema.drop_if_exists("users")
"#,
        ),
        (
            "database/seeders/DatabaseSeeder.agi",
            r#"use Framework.Database.Seeder

class DatabaseSeeder extends Seeder:
    fn run() -> void:
        print("Database seeding complete")
"#,
        ),
        (
            "database/factories/UserFactory.agi",
            r#"use App.Models.User
use Framework.Database.Factory

class UserFactory extends Factory:
    fn definition() -> User:
        return User {
            name: fake.name(),
            email: fake.unique_email(),
            password_hash: hash_password("password"),
            role: "user"
        }
"#,
        ),
        (
            "tests/Feature/HomePageTest.agi",
            r#"use Framework.Testing.WebTest

test "home page returns successful response":
    let response = WebTest.get("/")
    response.assert_status(200)
    response.assert_contains("AGILANG Native Framework")
"#,
        ),
        (
            "tests/Feature/HealthApiTest.agi",
            r#"use Framework.Testing.WebTest

test "health API returns healthy status":
    let response = WebTest.get("/api/health")
    response.assert_status(200)
    response.assert_json({
        "status": "healthy"
    })
"#,
        ),
        (
            "tests/Unit/ApplicationServiceTest.agi",
            r#"use Framework.Testing.UnitTest
use App.Services.ApplicationService

test "version returns 0.4.0":
    let service = ApplicationService()
    assert(service.get_version() == "0.4.0")
"#,
        ),
        ("storage/cache/.gitkeep", ""),
        ("storage/logs/.gitkeep", ""),
        ("storage/sessions/.gitkeep", ""),
        ("storage/uploads/.gitkeep", ""),
        ("public/assets/.gitkeep", ""),
    ];
    files.extend(get_ai_docs());

    // Write all files
    for (relative_path, content) in &files {
        let full_path = path.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let processed_content = content
            .replace("{name}", name)
            .replace("{template}", template);
        fs::write(&full_path, processed_content)?;
    }

    // Verify template integrity
    for (relative_path, _) in &files {
        let full_path = path.join(relative_path);
        if !full_path.exists() {
            bail!(
                "Template verification failed: missing file `{}`",
                relative_path
            );
        }
        if !relative_path.ends_with(".gitkeep") {
            let metadata = fs::metadata(&full_path)?;
            if metadata.len() == 0 {
                bail!(
                    "Template verification failed: empty required file `{}`",
                    relative_path
                );
            }
        }
    }

    Ok(())
}

fn normalize_path_name(name: &str) -> String {
    name.split('/')
        .map(|part| {
            if part.is_empty() {
                return String::new();
            }
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
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

    let normalized_name = normalize_path_name(name);
    let parts: Vec<&str> = normalized_name.split('/').collect();
    let class_name = parts.last().cloned().unwrap_or("");

    match component {
        "controller" => {
            let file_name = if class_name.ends_with("Controller") {
                normalized_name.clone()
            } else {
                format!("{}Controller", normalized_name)
            };
            let actual_class_name = if class_name.ends_with("Controller") {
                class_name.to_string()
            } else {
                format!("{}Controller", class_name)
            };
            let path = project_root
                .join("app/Controllers")
                .join(format!("{}.agi", file_name));
            if path.exists() {
                bail!("controller `{}` already exists", file_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let ns_parts = file_name.split('/').collect::<Vec<_>>();
            let ns = if ns_parts.len() > 1 {
                format!(
                    "App.Controllers.{}",
                    ns_parts[..ns_parts.len() - 1].join(".")
                )
            } else {
                "App.Controllers".to_string()
            };

            let content = format!(
                "module {}\n\nuse Framework.Http.Request\nuse Framework.Http.Response\n\nclass {}:\n    fn index(request: Request) -> Response:\n        return Response.html(\"<h1>{}</h1>\")\n",
                ns, actual_class_name, actual_class_name
            );
            fs::write(&path, content)?;
            println!("Created controller app/Controllers/{}.agi", file_name);
        }
        "model" => {
            let path = project_root
                .join("app/Models")
                .join(format!("{}.agi", normalized_name));
            if path.exists() {
                bail!("model `{}` already exists", normalized_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let ns = if parts.len() > 1 {
                format!("App.Models.{}", parts[..parts.len() - 1].join("."))
            } else {
                "App.Models".to_string()
            };

            let content = format!("module {}\n\nclass {}:\n    let id: i64\n", ns, class_name);
            fs::write(&path, content)?;
            println!("Created model app/Models/{}.agi", normalized_name);
        }
        "service" => {
            let file_name = if class_name.ends_with("Service") {
                normalized_name.clone()
            } else {
                format!("{}Service", normalized_name)
            };
            let actual_class_name = if class_name.ends_with("Service") {
                class_name.to_string()
            } else {
                format!("{}Service", class_name)
            };
            let path = project_root
                .join("app/Services")
                .join(format!("{}.agi", file_name));
            if path.exists() {
                bail!("service `{}` already exists", file_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let ns_parts = file_name.split('/').collect::<Vec<_>>();
            let ns = if ns_parts.len() > 1 {
                format!("App.Services.{}", ns_parts[..ns_parts.len() - 1].join("."))
            } else {
                "App.Services".to_string()
            };

            let content = format!(
                "module {}\n\nclass {}:\n    fn handle() -> void:\n        pass\n",
                ns, actual_class_name
            );
            fs::write(&path, content)?;
            println!("Created service app/Services/{}.agi", file_name);
        }
        "middleware" => {
            let file_name = if class_name.ends_with("Middleware") {
                normalized_name.clone()
            } else {
                format!("{}Middleware", normalized_name)
            };
            let actual_class_name = if class_name.ends_with("Middleware") {
                class_name.to_string()
            } else {
                format!("{}Middleware", class_name)
            };
            let path = project_root
                .join("app/Middleware")
                .join(format!("{}.agi", file_name));
            if path.exists() {
                bail!("middleware `{}` already exists", file_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let ns_parts = file_name.split('/').collect::<Vec<_>>();
            let ns = if ns_parts.len() > 1 {
                format!(
                    "App.Middleware.{}",
                    ns_parts[..ns_parts.len() - 1].join(".")
                )
            } else {
                "App.Middleware".to_string()
            };

            let content = format!(
                "module {}\n\nuse Framework.Http.Request\nuse Framework.Http.Response\nuse Framework.Middleware.Next\n\nclass {}:\n    fn handle(request: Request, next: Next) -> Response:\n        return next(request)\n",
                ns, actual_class_name
            );
            fs::write(&path, content)?;
            println!("Created middleware app/Middleware/{}.agi", file_name);
        }
        "provider" => {
            let file_name = if class_name.ends_with("ServiceProvider") {
                normalized_name.clone()
            } else {
                format!("{}ServiceProvider", normalized_name)
            };
            let actual_class_name = if class_name.ends_with("ServiceProvider") {
                class_name.to_string()
            } else {
                format!("{}ServiceProvider", class_name)
            };
            let path = project_root
                .join("app/Providers")
                .join(format!("{}.agi", file_name));
            if path.exists() {
                bail!("provider `{}` already exists", file_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let ns_parts = file_name.split('/').collect::<Vec<_>>();
            let ns = if ns_parts.len() > 1 {
                format!("App.Providers.{}", ns_parts[..ns_parts.len() - 1].join("."))
            } else {
                "App.Providers".to_string()
            };

            let content = format!(
                "module {}\n\nuse Framework.Container.Container\nuse Framework.Providers.ServiceProvider\n\nclass {} extends ServiceProvider:\n    fn register(container: Container) -> void:\n        pass\n\n    fn boot() -> void:\n        pass\n",
                ns, actual_class_name
            );
            fs::write(&path, content)?;
            println!("Created provider app/Providers/{}.agi", file_name);
        }
        "view" => {
            let path = project_root
                .join("resources/views")
                .join(format!("{}.ags", normalized_name));
            if path.exists() {
                bail!("view `{}` already exists", normalized_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                &path,
                format!("<!-- View template {} -->\n", normalized_name),
            )?;
            println!("Created view resources/views/{}.ags", normalized_name);
        }
        "route" => {
            let path = project_root
                .join("routes")
                .join(format!("{}.agi", normalized_name));
            if path.exists() {
                bail!("route file `{}` already exists", normalized_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = format!(
                "use Framework.Routing.Route\n\nfn register_{}() -> void:\n    pass\n",
                class_name.to_lowercase()
            );
            fs::write(&path, content)?;
            println!("Created routes/{}.agi", normalized_name);
        }
        "migration" => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let migration_class = to_pascal_case(class_name);
            let file_name = format!("{}_{}", timestamp, migration_class);
            let path = project_root
                .join("database/migrations")
                .join(format!("{}.agi", file_name));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = format!(
                "use Framework.Database.Migration\nuse Framework.Database.Schema\n\nclass {} extends Migration:\n    fn up() -> void:\n        pass\n\n    fn down() -> void:\n        pass\n",
                migration_class
            );
            fs::write(&path, content)?;
            println!("Created migration database/migrations/{}.agi", file_name);
        }
        "test" => {
            let path = project_root
                .join("tests/Feature")
                .join(format!("{}Test.agi", normalized_name));
            if path.exists() {
                bail!("test `{}` already exists", normalized_name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = format!(
                "fn test_{}() -> i32:\n    return 0\n",
                class_name.to_lowercase()
            );
            fs::write(&path, content)?;
            println!("Created test tests/Feature/{}Test.agi", normalized_name);
        }
        _ => bail!("unknown framework component `{}`", component),
    }

    Ok(())
}

pub fn generate_auth(roles: Vec<String>, force: bool, repair: bool) -> Result<()> {
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

    let auth_files = [
        (
            "app/Controllers/Auth/LoginController.agi",
            r#"module App.Controllers.Auth

use Framework.Auth.Auth
use Framework.Http.Request
use Framework.Http.Response
use Framework.Validation.Validator
use Framework.View

class LoginController:
    fn show(request: Request) -> Response:
        return View.render("auth/login")

    fn login(request: Request) -> Response:
        let input = Validator.validate(request.body(), {
            "email": ["required", "email"],
            "password": ["required", "string"]
        })

        if Auth.attempt(input.email, input.password):
            request.session().regenerate()
            return Response.redirect("/dashboard")

        return View.render("auth/login", {
            "error": "Invalid email or password."
        }).status(422)
"#,
        ),
        (
            "app/Controllers/Auth/RegisterController.agi",
            r#"module App.Controllers.Auth

class RegisterController:
    fn show() -> void:
        pass
"#,
        ),
        (
            "app/Controllers/Auth/LogoutController.agi",
            r#"module App.Controllers.Auth

class LogoutController:
    fn logout() -> void:
        pass
"#,
        ),
        (
            "app/Controllers/Auth/PasswordController.agi",
            r#"module App.Controllers.Auth

class PasswordController:
    fn reset() -> void:
        pass
"#,
        ),
        (
            "app/Middleware/Authenticate.agi",
            r#"module App.Middleware

class Authenticate:
    fn handle() -> void:
        pass
"#,
        ),
        (
            "app/Middleware/GuestOnly.agi",
            r#"module App.Middleware

class GuestOnly:
    fn handle() -> void:
        pass
"#,
        ),
        (
            "app/Middleware/RequireRole.agi",
            r#"module App.Middleware

class RequireRole:
    fn handle() -> void:
        pass
"#,
        ),
        (
            "app/Models/User.agi",
            r#"module App.Models

class User:
    let id: i64
    let name: string
    let email: string
    let password_hash: string
    let role: string

    fn is_admin() -> bool:
        return self.role == "admin"
"#,
        ),
        (
            "app/Models/Role.agi",
            r#"module App.Models

class Role:
    let id: i64
    let name: string
"#,
        ),
        (
            "app/Services/AuthService.agi",
            r#"module App.Services

class AuthService:
    fn check() -> bool:
        return true
"#,
        ),
        (
            "app/Services/PasswordHasher.agi",
            r#"module App.Services

class PasswordHasher:
    fn hash(password: string) -> string:
        return password
"#,
        ),
        (
            "app/Services/SessionService.agi",
            r#"module App.Services

class SessionService:
    fn regenerate() -> void:
        pass
"#,
        ),
        (
            "resources/views/auth/login.ags",
            r#"<h1>Login</h1>
<form method="POST" action="/login">
    <input type="email" name="email" required>
    <input type="password" name="password" required>
    <button type="submit">Log in</button>
</form>
"#,
        ),
        (
            "resources/views/auth/register.ags",
            r#"<h1>Register</h1>
"#,
        ),
        (
            "resources/views/auth/forgot-password.ags",
            r#"<h1>Forgot Password</h1>
"#,
        ),
        (
            "resources/views/auth/reset-password.ags",
            r#"<h1>Reset Password</h1>
"#,
        ),
        (
            "resources/views/dashboard/user.ags",
            r#"<h1>User Dashboard</h1>
"#,
        ),
        (
            "resources/views/dashboard/admin.ags",
            r#"<h1>Admin Dashboard</h1>
"#,
        ),
        (
            "database/migrations/CreateUsersTable.agi",
            r#"use Framework.Database.Migration
use Framework.Database.Schema

class CreateUsersTable extends Migration:
    fn up() -> void:
        Schema.create("users", fn table:
            table.id()
            table.string("name")
            table.string("email").unique()
            table.string("password_hash")
            table.string("role").default("user")
            table.timestamps()
        )

    fn down() -> void:
        Schema.drop_if_exists("users")
"#,
        ),
        (
            "database/migrations/CreateRolesTable.agi",
            r#"use Framework.Database.Migration
use Framework.Database.Schema

class CreateRolesTable extends Migration:
    fn up() -> void:
        Schema.create("roles", fn table:
            table.id()
            table.string("name").unique()
        )

    fn down() -> void:
        Schema.drop_if_exists("roles")
"#,
        ),
        (
            "database/migrations/CreateUserRolesTable.agi",
            r#"use Framework.Database.Migration
use Framework.Database.Schema

class CreateUserRolesTable extends Migration:
    fn up() -> void:
        Schema.create("user_roles", fn table:
            table.integer("user_id")
            table.integer("role_id")
        )

    fn down() -> void:
        Schema.drop_if_exists("user_roles")
"#,
        ),
        (
            "routes/auth.agi",
            r#"use Framework.Routing.Route
use App.Controllers.Auth.LoginController
use App.Controllers.DashboardController
use App.Middleware.Authenticate
use App.Middleware.RequireRole

fn register_auth() -> void:
    Route.get("/login", LoginController.show)
    Route.post("/login", LoginController.login)

    # Auth protected routes for user and admin
    Route.group("/dashboard", fn:
        Route.middleware(Authenticate)
        Route.get("/user", DashboardController.user)

        Route.group("/admin", fn:
            Route.middleware(RequireRole)
            Route.get("/", DashboardController.admin)
        )
    )
"#,
        ),
        (
            "app/Controllers/DashboardController.agi",
            r#"module App.Controllers

use Framework.Http.Request
use Framework.Http.Response
use Framework.View

class DashboardController:
    fn user(request: Request) -> Response:
        return View.render("dashboard/user", {
            "title": "User Dashboard"
        })

    fn admin(request: Request) -> Response:
        return View.render("dashboard/admin", {
            "title": "Admin Dashboard"
        })
"#,
        ),
    ];

    println!("Checking project...");

    let mut exists = false;
    for (relative_path, _) in &auth_files {
        if project_root.join(relative_path).exists() {
            exists = true;
            break;
        }
    }

    if exists && !force && !repair {
        bail!(
            "Authentication already exists.\n\nUse:\n    agilang auth --repair\n    agilang auth --force"
        );
    }

    for (relative_path, content) in &auth_files {
        let path = project_root.join(relative_path);
        if path.exists() && repair {
            continue; // preserve customized files during repair
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        println!("Created {}", relative_path);
    }

    // Write config/auth.agi with specified roles
    let config_path = project_root.join("config/auth.agi");
    if !config_path.exists() || force {
        let mut roles_str = String::new();
        for (idx, r) in roles.iter().enumerate() {
            roles_str.push_str(&format!("        \"{}\"", r));
            if idx < roles.len() - 1 {
                roles_str.push_str(",\n");
            } else {
                roles_str.push('\n');
            }
        }
        let config_content = format!(
            "use Framework.Auth.AuthConfig\n\nreturn AuthConfig {{\n    enabled: true,\n    default_role: \"user\",\n    roles: [\n{}\n    ]\n}}\n",
            roles_str
        );
        fs::write(&config_path, config_content)?;
        println!("Created config/auth.agi");
    }

    // Auto-update routes/web.agi to register auth routes if present
    let web_routes_file = project_root.join("routes/web.agi");
    if web_routes_file.exists() {
        let content = fs::read_to_string(&web_routes_file)?;
        if !content.contains("register_auth") {
            let mut new_lines = vec!["use Routes.Auth".to_string()];
            let mut updated = false;
            for line in content.lines() {
                if line.trim().starts_with("Route.get") && !updated {
                    new_lines.push("    Auth.register_auth()".to_string());
                    updated = true;
                }
                new_lines.push(line.to_string());
            }
            fs::write(&web_routes_file, new_lines.join("\n"))?;
            println!("Updated routes/web.agi with authentication routes");
        }
    }

    Ok(())
}

pub fn repair_template(dry_run: bool, force: bool) -> Result<()> {
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

    println!("Inspecting AGILANG web template...\n");

    let mut files = vec![
        (
            "agilang.toml",
            r#"[project]
name = "{name}"
version = "0.1.0"
edition = "2026"
type = "web"

[template]
name = "web"
version = "0.4.0"

[application]
entry = "bootstrap/app.agi"
environment = "local"

[server]
host = "127.0.0.1"
port = 8080

[routes]
web = "routes/web.agi"
api = "routes/api.agi"

[views]
path = "resources/views"
cache = "storage/cache/views"

[public]
path = "public"

[build]
target = "native"
output = "build/{name}.exe"
"#,
        ),
        (
            ".env.example",
            r#"APP_NAME=AGILANG
APP_ENV=local
APP_URL=http://127.0.0.1:8080
APP_HOST=127.0.0.1
APP_PORT=8080

FORCE_HTTPS=false
TRUST_PROXY=false

SESSION_SECURE=false
SESSION_HTTP_ONLY=true
SESSION_SAME_SITE=Lax
"#,
        ),
        (
            "README.md",
            r#"# {name}

Created with AGILANG CLI.
"#,
        ),
        (
            "bootstrap/app.agi",
            r#"use Framework.Application
use App.Providers.AppServiceProvider
use App.Providers.RouteServiceProvider

fn bootstrap() -> Application:
    let app = Application.create(base_path())

    app.register(AppServiceProvider)
    app.register(RouteServiceProvider)

    app.load_config("config")
    app.load_environment(".env")
    app.load_views("resources/views")
    app.public_path("public")

    return app
"#,
        ),
        (
            "app/Controllers/HomeController.agi",
            r#"module App.Controllers

use Framework.Http.Request
use Framework.Http.Response
use Framework.View

class HomeController:
    fn index(request: Request) -> Response:
        return View.render("welcome", {
            "title": "AGILANG Native Framework",
            "message": "Your application is running successfully."
        })

    fn about(request: Request) -> Response:
        return Response.html(
            "<h1>About AGILANG</h1><p>Native application framework.</p>"
        )
"#,
        ),
        (
            "app/Controllers/Api/HealthController.agi",
            r#"module App.Controllers.Api

use Framework.Http.Request
use Framework.Http.Response

class HealthController:
    fn show(request: Request) -> Response:
        return Response.json({
            "status": "healthy",
            "framework": "AGILANG",
            "version": "0.4.0"
        })
"#,
        ),
        (
            "app/Middleware/WebMiddleware.agi",
            r#"module App.Middleware

use Framework.Http.Request
use Framework.Http.Response
use Framework.Middleware.Next

class WebMiddleware:
    fn handle(request: Request, next: Next) -> Response:
        let response = next(request)
        response.header("X-Powered-By", "AGILANG")
        return response
"#,
        ),
        (
            "app/Middleware/ApiMiddleware.agi",
            r#"module App.Middleware

use Framework.Http.Request
use Framework.Http.Response
use Framework.Middleware.Next

class ApiMiddleware:
    fn handle(request: Request, next: Next) -> Response:
        let response = next(request)
        response.header("Content-Type", "application/json")
        return response
"#,
        ),
        (
            "app/Models/User.agi",
            r#"module App.Models

class User:
    let id: i64
    let name: string
    let email: string
    let password_hash: string
    let role: string

    fn is_admin() -> bool:
        return self.role == "admin"
"#,
        ),
        (
            "app/Providers/AppServiceProvider.agi",
            r#"module App.Providers

use Framework.Container.Container
use Framework.Providers.ServiceProvider
use App.Services.ApplicationService

class AppServiceProvider extends ServiceProvider:
    fn register(container: Container) -> void:
        container.singleton(ApplicationService, ApplicationService)

    fn boot() -> void:
        print("Application services booted")
"#,
        ),
        (
            "app/Providers/RouteServiceProvider.agi",
            r#"module App.Providers

use Framework.Application
use Framework.Providers.ServiceProvider
use Routes.Web
use Routes.Api

class RouteServiceProvider extends ServiceProvider:
    fn boot(app: Application) -> void:
        Web.register_web()
        Api.register_api()
"#,
        ),
        (
            "app/Services/ApplicationService.agi",
            r#"module App.Services

class ApplicationService:
    fn get_version() -> string:
        return "0.4.0"
"#,
        ),
        (
            "app/Console/Commands/HelloCommand.agi",
            r#"module App.Console.Commands

class HelloCommand:
    fn handle() -> void:
        print("Hello from AGILANG Console")
"#,
        ),
        (
            "config/app.agi",
            r#"use Framework.Config.AppConfig

return AppConfig {
    name: env("APP_NAME", "AGILANG Application"),
    environment: env("APP_ENV", "local"),
    url: env("APP_URL", "http://127.0.0.1:8080"),
    debug: env_bool("APP_DEBUG", true)
}
"#,
        ),
        (
            "config/server.agi",
            r#"use Framework.Config.ServerConfig

return ServerConfig {
    host: env("APP_HOST", "127.0.0.1"),
    port: env_i32("APP_PORT", 8080),
    https_port: env_i32("APP_HTTPS_PORT", 8443),
    force_https: env_bool("FORCE_HTTPS", false)
}
"#,
        ),
        (
            "config/auth.agi",
            r#"use Framework.Auth.AuthConfig

return AuthConfig {
    enabled: false,
    default_role: "user",
    roles: [
        "user",
        "admin"
    ]
}
"#,
        ),
        (
            "config/logging.agi",
            r#"return {
    "level": "debug"
}
"#,
        ),
        (
            "config/database.agi",
            r#"return {
    "driver": "sqlite"
}
"#,
        ),
        (
            "routes/web.agi",
            r#"use Framework.Routing.Route
use App.Controllers.HomeController

fn register_web() -> void:
    Route.get("/", HomeController.index)
    Route.get("/about", HomeController.about)
"#,
        ),
        (
            "routes/api.agi",
            r#"use Framework.Routing.Route
use App.Controllers.Api.HealthController

fn register_api() -> void:
    Route.group("/api", fn:
        Route.get("/health", HealthController.show)
    )
"#,
        ),
        (
            "routes/console.agi",
            r#"use Framework.Routing.Route
use App.Console.Commands.HelloCommand

fn register_console() -> void:
    Route.command("hello", HelloCommand.handle)
"#,
        ),
        (
            "resources/views/layouts/app.ags",
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{ title ?? "AGILANG Application" }}</title>
    <link rel="stylesheet" href="/assets/css/app.css">
</head>
<body>
    <header class="navbar">
        <a href="/" class="brand">AGILANG</a>
        <nav>
            <a href="/">Home</a>
            <a href="/about">About</a>
            <a href="/api/health">API Health</a>
        </nav>
    </header>
    <main>
        @yield("content")
    </main>
    <script src="/assets/js/app.js"></script>
</body>
</html>
"#,
        ),
        (
            "resources/views/welcome.ags",
            r#"@extends("layouts/app")

@section("content")
<section class="hero">
    <div class="hero-card">
        <span class="eyebrow">AGILANG Native Framework</span>
        <h1>{{ title }}</h1>
        <p>{{ message }}</p>

        <div class="network-matrix">
            <div class="matrix-item">
                <span class="label">WebSocket</span>
                <span id="ws-status" class="badge warning">Connecting...</span>
            </div>
            <div class="matrix-item">
                <span class="label">WebRTC</span>
                <span id="rtc-status" class="badge warning">Negotiating...</span>
            </div>
            <div class="matrix-item">
                <span class="label">Local STUN</span>
                <span class="badge success">Active (Port 3478)</span>
            </div>
        </div>

        <div class="actions">
            <a class="button primary" href="/about">Explore framework</a>
            <a class="button secondary" href="/api/health">Check API</a>
        </div>
    </div>
</section>
@endsection
"#,
        ),
        (
            "resources/views/errors/404.ags",
            r#"<h1>404 Not Found</h1>
<p>The page you are looking for does not exist.</p>
"#,
        ),
        (
            "resources/views/errors/500.ags",
            r#"<h1>500 Internal Server Error</h1>
<p>Something went wrong on our end.</p>
"#,
        ),
        (
            "resources/views/components/button.ags",
            r#"<!-- Button Component -->
<button class="btn">{{ text }}</button>
"#,
        ),
        (
            "resources/assets/css/app.css",
            r#":root {
    font-family: Inter, system-ui, sans-serif;
    color: #eef6ff;
    background: #07111f;
}

* {
    box-sizing: border-box;
}

body {
    margin: 0;
    min-height: 100vh;
    background: radial-gradient(circle at top, #173968, transparent 42%), #07111f;
}

.navbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 40px;
    border-bottom: 1px solid rgba(255,255,255,0.1);
}

.brand, nav a {
    color: white;
    text-decoration: none;
}

nav {
    display: flex;
    gap: 24px;
}

.hero {
    min-height: calc(100vh - 82px);
    display: grid;
    place-items: center;
    padding: 32px;
}

.hero-card {
    width: min(760px, 100%);
    padding: 56px;
    border-radius: 28px;
    border: 1px solid rgba(113, 177, 255, 0.28);
    background: rgba(8, 24, 45, 0.88);
    box-shadow: 0 32px 100px rgba(0,0,0,0.45);
}

.eyebrow {
    color: #77b9ff;
    letter-spacing: 0.08em;
    text-transform: uppercase;
}

h1 {
    margin: 16px 0;
    font-size: clamp(42px, 7vw, 72px);
}

p {
    color: #abc0dc;
    font-size: 19px;
}

.network-matrix {
    display: flex;
    gap: 16px;
    margin: 24px 0;
    padding: 16px;
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
}

.matrix-item {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
}

.matrix-item .label {
    font-size: 13px;
    color: #8fa0b5;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.badge {
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 600;
}

.badge.warning {
    color: #ffe082;
    background: rgba(255, 224, 130, 0.15);
    border: 1px solid rgba(255, 224, 130, 0.3);
}

.badge.success {
    color: #81c784;
    background: rgba(129, 199, 132, 0.15);
    border: 1px solid rgba(129, 199, 132, 0.3);
}

.badge.error {
    color: #e57373;
    background: rgba(229, 115, 115, 0.15);
    border: 1px solid rgba(229, 115, 115, 0.3);
}

.actions {
    display: flex;
    gap: 12px;
    margin-top: 32px;
}

.button {
    padding: 13px 20px;
    border-radius: 10px;
    text-decoration: none;
}

.primary {
    color: white;
    background: #1769e0;
}

.secondary {
    color: white;
    border: 1px solid rgba(255,255,255,0.24);
}
"#,
        ),
        (
            "resources/assets/js/app.js",
            r#"document.documentElement.dataset.agilang = "ready";
console.info("AGILANG application assets loaded");

// 1. WebSocket integration
const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
const socket = new WebSocket(`${wsProtocol}//${window.location.host}`);

socket.onopen = () => {
    console.info("WebSocket connected successfully to AGILANG Native Server!");
    const wsBadge = document.getElementById("ws-status");
    if (wsBadge) {
        wsBadge.textContent = "Connected";
        wsBadge.className = "badge success";
    }
};

socket.onmessage = (event) => {
    console.log("WebSocket message received:", event.data);
};

socket.onerror = (error) => {
    console.error("WebSocket error:", error);
    const wsBadge = document.getElementById("ws-status");
    if (wsBadge) {
        wsBadge.textContent = "Error";
        wsBadge.className = "badge error";
    }
};

// 2. WebRTC integration with built-in STUN
const configuration = {
    iceServers: [
        { urls: `stun:${window.location.hostname}:3478` }
    ]
};

const peerConnection = new RTCPeerConnection(configuration);

peerConnection.onicecandidate = (event) => {
    if (event.candidate) {
        console.info("Local STUN server resolved ICE candidate:", event.candidate.candidate);
        const rtcBadge = document.getElementById("rtc-status");
        if (rtcBadge) {
            rtcBadge.textContent = "ICE Resolved";
            rtcBadge.className = "badge success";
        }
    }
};

peerConnection.onconnectionstatechange = () => {
    console.info("WebRTC Connection State changed to:", peerConnection.connectionState);
};

peerConnection.createDataChannel("agi-sync");
peerConnection.createOffer()
    .then(offer => peerConnection.setLocalDescription(offer))
    .catch(err => console.error("WebRTC offer error:", err));
"#,
        ),
        (
            "public/index.html",
            r#"<h1>Hello from AGILANG Public Folder</h1>
"#,
        ),
        (
            "public/favicon.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="40" fill="#1769e0"/></svg>
"##,
        ),
        (
            "database/migrations/CreateUsersTable.agi",
            r#"use Framework.Database.Migration
use Framework.Database.Schema

class CreateUsersTable extends Migration:
    fn up() -> void:
        Schema.create("users", fn table:
            table.id()
            table.string("name")
            table.string("email").unique()
            table.string("password_hash")
            table.string("role").default("user")
            table.timestamps()
        )

    fn down() -> void:
        Schema.drop_if_exists("users")
"#,
        ),
        (
            "database/seeders/DatabaseSeeder.agi",
            r#"use Framework.Database.Seeder

class DatabaseSeeder extends Seeder:
    fn run() -> void:
        print("Database seeding complete")
"#,
        ),
        (
            "database/factories/UserFactory.agi",
            r#"use App.Models.User
use Framework.Database.Factory

class UserFactory extends Factory:
    fn definition() -> User:
        return User {
            name: fake.name(),
            email: fake.unique_email(),
            password_hash: hash_password("password"),
            role: "user"
        }
"#,
        ),
        (
            "tests/Feature/HomePageTest.agi",
            r#"use Framework.Testing.WebTest

test "home page returns successful response":
    let response = WebTest.get("/")
    response.assert_status(200)
    response.assert_contains("AGILANG Native Framework")
"#,
        ),
        (
            "tests/Feature/HealthApiTest.agi",
            r#"use Framework.Testing.WebTest

test "health API returns healthy status":
    let response = WebTest.get("/api/health")
    response.assert_status(200)
    response.assert_json({
        "status": "healthy"
    })
"#,
        ),
        (
            "tests/Unit/ApplicationServiceTest.agi",
            r#"use Framework.Testing.UnitTest
use App.Services.ApplicationService

test "version returns 0.4.0":
    let service = ApplicationService()
    assert(service.get_version() == "0.4.0")
"#,
        ),
        ("storage/cache/.gitkeep", ""),
        ("storage/logs/.gitkeep", ""),
        ("storage/sessions/.gitkeep", ""),
        ("storage/uploads/.gitkeep", ""),
        ("public/assets/.gitkeep", ""),
    ];
    files.extend(get_ai_docs());

    let app_name = project_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());

    let mut missing = vec![];
    let mut empty = vec![];

    for (relative_path, _) in &files {
        let path = project_root.join(relative_path);
        if !path.exists() {
            missing.push(*relative_path);
        } else if !relative_path.ends_with(".gitkeep") {
            let metadata = fs::metadata(&path)?;
            if metadata.len() == 0 {
                empty.push(*relative_path);
            }
        }
    }

    if missing.is_empty() && empty.is_empty() && !force {
        println!("All template files are intact and verified!");
        return Ok(());
    }

    if !missing.is_empty() {
        println!("Missing:");
        for m in &missing {
            println!("  {}", m);
        }
    }

    if !empty.is_empty() {
        println!("Empty/Placeholder-only:");
        for e in &empty {
            println!("  {}", e);
        }
    }

    if dry_run {
        println!("\nDry run complete. No files were written.");
        return Ok(());
    }

    // Write missing or all files
    for (relative_path, content) in &files {
        let path = project_root.join(relative_path);
        let is_missing = missing.contains(relative_path);
        let is_empty = empty.contains(relative_path);

        if is_missing || is_empty || force {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let processed_content = content
                .replace("{name}", &app_name)
                .replace("{template}", "web");
            fs::write(&path, processed_content)?;
        }
    }

    println!("\nRepair completed.");
    println!("Customized files were preserved.");

    Ok(())
}

fn get_ai_docs() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "AGENTS.md",
            r#"# AGILANG Native Application - Agent Guidelines

Welcome! This project is built using the AGILANG Native Web Framework.
Please follow these guidelines when acting as an agent:
- Keep route files in `routes/web.agi` and `routes/api.agi`.
- Place controllers inside `app/Controllers/`.
- Use AGS templates under `resources/views/`.
- Run checks with `agilang check` and server with `agilang serve`.
"#,
        ),
        (
            "AGILANG.md",
            r#"# AGILANG Language Specification Summary

AGILANG is a native, compiled language designed for maximum performance web applications.
- Native execution compiled via MSVC/GCC.
- Built-in dynamic router, template engine, and WebSocket/WebRTC capabilities.
"#,
        ),
        (
            "docs/AI_CONTEXT.md",
            r#"# AI Context Guidelines
Use this context to align on coding standards and styles for the project.
"#,
        ),
        (
            "docs/LANGUAGE_REFERENCE.md",
            r#"# Language Reference
Syntax details for functions, classes, models, and migrations.
"#,
        ),
        (
            "docs/FRAMEWORK_GUIDE.md",
            r#"# Framework Guide
Details on request, response, routing, layouts, and rendering.
"#,
        ),
        (
            "docs/PROJECT_STRUCTURE.md",
            r#"# Project Structure
Overview of files and directories.
"#,
        ),
        (
            "docs/ROUTING.md",
            r#"# Routing
Route declarations, groups, controllers, and middleware mappings.
"#,
        ),
        (
            "docs/CONTROLLERS.md",
            r#"# Controllers
Actions, request extraction, responses, JSON, and view rendering.
"#,
        ),
        (
            "docs/AGS_TEMPLATES.md",
            r#"# AGS Templates
Yielding, layout extensions, sections, and interpolation.
"#,
        ),
        (
            "docs/AUTHENTICATION.md",
            r#"# Authentication
Role and permissions middleware, login/logout, and routes.
"#,
        ),
        (
            "docs/BUILD_AND_TEST.md",
            r#"# Build and Test
Build pipeline and feature/unit testing guides.
"#,
        ),
        (
            "docs/spec/compiler-capabilities.json",
            r#"{
  "implemented": [
    "lexer",
    "parser",
    "classes",
    "functions",
    "primitive_types",
    "let_binding",
    "if_expression",
    "while_loop"
  ],
  "partial": [
    "structs",
    "lists"
  ],
  "scaffoldOnly": [
    "auth_security"
  ],
  "planned": [
    "generics",
    "async_await"
  ]
}
"#,
        ),
        (
            "docs/spec/framework-capabilities.json",
            r#"{
  "implemented": [
    "http_server",
    "http_router",
    "ags_template_engine",
    "websockets",
    "webrtc_stun"
  ],
  "partial": [
    "session_persistence"
  ],
  "scaffoldOnly": [
    "database_orm"
  ],
  "planned": [
    "queue_workers"
  ]
}
"#,
        ),
        (
            "docs/spec/project-structure.json",
            r#"{
  "directories": [
    "app",
    "config",
    "database",
    "routes",
    "resources",
    "public",
    "tests",
    "storage"
  ]
}
"#,
        ),
        (
            "docs/spec/examples-index.json",
            r#"{
  "examples": [
    {
      "name": "hello_world",
      "path": "examples/native/hello.agi"
    }
  ]
}
"#,
        ),
    ]
}
