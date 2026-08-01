use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

const DEFAULT_WEB_VIEW: &str = include_str!("../templates/welcome.ags");
const DEFAULT_WEB_CSS: &str = include_str!("../templates/assets/app.css");
const DEFAULT_WEB_JS: &str = include_str!("../templates/assets/app.js");
const DEFAULT_WEB_LOGO: &[u8] = include_bytes!("../templates/assets/agilang-logo.png");
const DEFAULT_AGIDB_STORE: &[u8] =
    b"AGIDB001{\n  \"format\": \"AGIDB001\",\n  \"version\": 1,\n  \"next_row_id\": 1,\n  \"tables\": {\n    \"users\": {\n      \"columns\": [\"id\", \"name\", \"email\", \"password_hash\", \"role\", \"created_at\", \"updated_at\"],\n      \"rows\": []\n    },\n    \"sessions\": {\n      \"columns\": [\"id\", \"user_id\", \"token\", \"expires_at\", \"created_at\"],\n      \"rows\": []\n    }\n  }\n}\n";
const HTTP_CLIENT_APP_WRAPPER: &str = r#"fn http_request(method: string, url: string, query: string, headers: string, body: string, timeout_ms: i64) -> string:
    return Native.Http.request({
        "method": method,
        "url": url,
        "query": query,
        "headers": headers,
        "body": body,
        "timeout_ms": timeout_ms
    })

fn http_get(url: string, query: string) -> string:
    return Native.Http.request({
        "method": "GET",
        "url": url,
        "query": query,
        "headers": "{}",
        "body": "",
        "timeout_ms": 30000
    })

fn http_post(url: string, headers: string, body: string) -> string:
    return Native.Http.request({
        "method": "POST",
        "url": url,
        "query": "{}",
        "headers": headers,
        "body": body,
        "timeout_ms": 30000
    })

fn http_post_json(url: string, body: string) -> string:
    return Native.Http.request({
        "method": "POST",
        "url": url,
        "query": "{}",
        "headers": "{\"Content-Type\":\"application/json\"}",
        "body": body,
        "timeout_ms": 30000
    })
"#;
const HTTP_CLIENT_RUNTIME_CONTRACT: &str = r#"# HTTP Client Runtime Contract

This project-local wrapper delegates outbound HTTP requests to `Native.Http.request`.

## Request shape

`App.Http.HttpClient.request(options)` passes a map with these fields to the runtime:

- `method` - HTTP method string such as `GET` or `POST`
- `url` - absolute `http://` or `https://` URL
- `query` - map of query keys to one or more values
- `headers` - map of header names to header values
- `body` - request body payload
- `timeout_ms` - timeout in milliseconds

## Query encoding

Query parameters are encoded by the native runtime using RFC 3986 percent-encoding.
Spaces become `%20`, not `+`.

## Runtime dependency

This wrapper requires an AGILANG runtime/toolchain build that exports the `http.request`
intrinsic through `agi_http_request_json`.
"#;

pub fn generate_project(name: &str, template: &str) -> Result<()> {
    let path = PathBuf::from(name);
    if path.exists() {
        bail!("directory `{}` already exists", name);
    }
    let project_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("project name must resolve to a final path segment"))?
        .to_string();

    let mut files = vec![
        (
            "agilang.toml",
            r#"[project]
name = "{name}"
version = "0.1.0"
edition = "2026"
type = "{template}"
toolchain = "{toolchain_version}"

[template]
name = "{template}"
version = "{toolchain_version}"

[application]
id = "{name}"
entry = "bootstrap/app.agi"
environment = "local"

[security]
fail_closed = true
require_tls_in_production = true
reject_plaintext_production_env = true
database_verification = true

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

[branding]
logo = ".agilang/branding/agilang-logo.png"
language_id = "agilang"
template_language_id = "agilang-ags"

[editor]
syntax_grammar = ".agilang/editor/agilang.tmLanguage.json"
ags_syntax_grammar = ".agilang/editor/ags.tmLanguage.json"

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

SESSION_SECURE=true
SESSION_HTTP_ONLY=true
SESSION_SAME_SITE=Strict
"#,
        ),
        (
            ".gitignore",
            r#".env
.env.*
!.env.example
*.key
*.pem
*.p12
*.secret
vault-recovery/
/build/
/target/
"#,
        ),
        (
            ".env.development.agi.enc",
            r#"AGIVLT
version=1
application={name}
environment=development
status=unsealed-placeholder
"#,
        ),
        (
            "README.md",
            r#"# {name}

Created with the native AGILANG CLI. This application runs with AGI backend code, AGS reactive frontend templates, generated browser JavaScript where required, and the Rust native runtime. Python is not required.

The official logo and syntax definitions are stored under `.agilang/`.
"#,
        ),
        (
            ".vscode/settings.json",
            r#"{
  "files.associations": {
    "*.agi": "agilang",
    "*.ags": "agilang-ags"
  }
}
"#,
        ),
        (
            ".vscode/extensions.json",
            r#"{
  "recommendations": ["global-fintech.agilang-language-support"]
}
"#,
        ),
        (
            ".agilang/editor/agilang.tmLanguage.json",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../editor/vscode-agilang/syntaxes/agilang.tmLanguage.json"
            )),
        ),
        (
            ".agilang/editor/ags.tmLanguage.json",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../editor/vscode-agilang/syntaxes/ags.tmLanguage.json"
            )),
        ),
        (
            "bootstrap/app.agi",
            r#"use Framework.Application
use App.Providers.AppServiceProvider
use App.Providers.RouteServiceProvider
use Native.Path

fn bootstrap() -> Application:
    let app = Application.create(Native.Path.cwd())

    app.register(AppServiceProvider)
    app.register(RouteServiceProvider)

    app.load_config("config")
    app.load_encrypted_environment(".env." + app.environment() + ".agi.enc", {
        "fail_closed": app.environment() != "local",
        "reject_plaintext_production_env": true
    })
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
            "app/Controllers/Auth/LoginController.agi",
            r#"module App.Controllers.Auth

use Framework.Http.Request
use Framework.Http.Response
use Framework.View

class LoginController:
    fn show(request: Request) -> Response:
        return View.render("auth/login", {
            "title": "Sign in",
            "csrf_field": ""
        })

    fn login(request: Request) -> Response:
        return View.render("dashboard/user", {
            "title": "Dashboard",
            "user_name": "Local AGILANG User",
            "database": "storage/database/main.agidb"
        })
"#,
        ),
        (
            "app/Controllers/Auth/RegisterController.agi",
            r#"module App.Controllers.Auth

use Framework.Http.Request
use Framework.Http.Response
use Framework.View

class RegisterController:
    fn show(request: Request) -> Response:
        return View.render("auth/register", {
            "title": "Create account",
            "csrf_field": ""
        })

    fn register(request: Request) -> Response:
        return View.render("dashboard/user", {
            "title": "Dashboard",
            "user_name": "Local AGILANG User",
            "database": "storage/database/main.agidb"
        })
"#,
        ),
        (
            "app/Controllers/Auth/LogoutController.agi",
            r#"module App.Controllers.Auth

use Framework.Http.Request
use Framework.Http.Response

class LogoutController:
    fn logout(request: Request) -> Response:
        return Response.html("<h1>Signed out</h1><p>Your local session has been closed.</p><p><a href=\"/login\">Sign in again</a></p>")
"#,
        ),
        (
            "app/Controllers/DashboardController.agi",
            r#"module App.Controllers

use Framework.Http.Request
use Framework.Http.Response
use Framework.View

class DashboardController:
    fn index(request: Request) -> Response:
        return View.render("dashboard/user", {
            "title": "Dashboard",
            "user_name": "Local AGILANG User",
            "database": "storage/database/main.agidb"
        })

    fn user(request: Request) -> Response:
        return View.render("dashboard/user", {
            "title": "Dashboard",
            "user_name": "Local AGILANG User",
            "database": "storage/database/main.agidb"
        })

    fn admin(request: Request) -> Response:
        return View.render("dashboard/admin", {
            "title": "Admin Dashboard",
            "database": "storage/database/main.agidb"
        })
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
            "version": "0.5.0"
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
        return "0.5.0"
"#,
        ),
        ("app/Policies/.gitkeep", ""),
        ("app/Requests/.gitkeep", ""),
        ("database/factories/.gitkeep", ""),
        ("tests/Unit/.gitkeep", ""),
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
    enabled: true,
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
    "default": "agidb",
    "connections": {
        "agidb": {
            "driver": "agidb",
            "database": "storage/database/main.agidb",
            "credential_source": "vault:database/roles/{name}-runtime",
            "pool_min": 2,
            "pool_max": 20,
            "connection_timeout_ms": 5000,
            "statement_timeout_ms": 15000
        },
        "sqlite": {
            "driver": "sqlite",
            "database": "storage/database/main.sqlite",
            "pool_min": 1,
            "pool_max": 10,
            "connection_timeout_ms": 5000,
            "statement_timeout_ms": 15000
        },
        "mysql": {
            "driver": "mysql",
            "host": env("DB_HOST", "127.0.0.1"),
            "port": env_i32("DB_PORT", 3306),
            "database": env("DB_DATABASE", "{name}"),
            "username": env("DB_USERNAME", ""),
            "password_secret": env("DB_PASSWORD_SECRET", "vault:database/mysql/{name}"),
            "tls": env("DB_TLS_MODE", "verify-full"),
            "pool_min": 2,
            "pool_max": 20,
            "connection_timeout_ms": 5000,
            "statement_timeout_ms": 15000
        }
    },
    "strict_driver_selection": true
}
"#,
        ),
        (
            "config/database.toml",
            r#"[database.primary]
driver = "agidb"
database = "storage/database/main.agidb"
credential_source = "vault:database/roles/{name}-runtime"
tls = "verify-full"
pool_min = 2
pool_max = 20
connection_timeout_ms = 5000
statement_timeout_ms = 15000
prepared_statements_only = true
raw_sql_requires_unsafe = true
audit_queries = true
"#,
        ),
        (
            "config/vault.toml",
            r#"[vault]
provider = "native"
namespace = "apps/{name}/APP_ENV"
address = "https://127.0.0.1:8720"
authentication = "application_identity"
fail_closed = true
audit_access = true
rotation_enabled = true
"#,
        ),
        (
            "security/policy.agi",
            r#"use Framework.Security.Policy

return Policy.secure_defaults({
    "fail_closed": true,
    "require_tls_in_production": true,
    "reject_plaintext_production_env": true,
    "prepared_statements_only": true,
    "audit_security_events": true
})
"#,
        ),
        (
            "security/permissions.agi",
            r#"use Framework.Security.Permissions

return Permissions.deny_by_default()
    .allow("application", "vault.read", "apps/{name}/APP_ENV/*")
    .allow("application", "database.connect", "primary")
"#,
        ),
        (
            "security/headers.agi",
            r#"return {
    "Content-Security-Policy": "default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'",
    "X-Content-Type-Options": "nosniff",
    "Referrer-Policy": "strict-origin-when-cross-origin",
    "Permissions-Policy": "camera=(), microphone=(), geolocation=()"
}
"#,
        ),
        (
            "security/rate-limits.agi",
            r#"return {
    "authentication": { "limit": 10, "window": "1m" },
    "api_per_user": { "limit": 120, "window": "1m" },
    "api_per_ip": { "limit": 500, "window": "1m" }
}
"#,
        ),
        (
            "vault/policy.hcl",
            r#"path "apps/{name}/*" {
  capabilities = ["read"]
}
path "database/creds/{name}-runtime" {
  capabilities = ["read"]
}
"#,
        ),
        (
            "vault/application-identity.json",
            r#"{
  "application_id": "{name}",
  "environment": "development",
  "public_identity_key": "GENERATE_DURING_INITIALIZATION",
  "private_key_storage": "operating-system-protected"
}
"#,
        ),
        (
            "vault/README.md",
            r#"# Application Vault

This namespace is isolated to this application. Generate the application identity and seal
environment secrets before deployment. Never commit recovery material or private keys.
"#,
        ),
        (
            "routes/web.agi",
            r#"use Framework.Routing.Route
use App.Controllers.HomeController
use App.Controllers.Auth.LoginController
use App.Controllers.Auth.RegisterController
use App.Controllers.Auth.LogoutController
use App.Controllers.DashboardController

fn register_web() -> void:
    Route.get("/", HomeController.index)
    Route.get("/about", HomeController.about)
    Route.get("/login", LoginController.show)
    Route.post("/login", LoginController.login)
    Route.get("/register", RegisterController.show)
    Route.post("/register", RegisterController.register)
    Route.get("/logout", LogoutController.logout)
    Route.get("/dashboard", DashboardController.index)
    Route.get("/dashboard/user", DashboardController.user)
    Route.get("/dashboard/admin", DashboardController.admin)
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
            "resources/views/auth/login.ags",
            r#"@page title="Sign in" robots="noindex,nofollow"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{ title }}</title>
    <link rel="stylesheet" href="/assets/css/app.css">
    <script src="/assets/js/app.js" defer></script>
</head>
<body>
    <main class="auth-shell">
        <section class="auth-panel">
            <a class="auth-brand" href="/">
                <img src="/assets/images/agilang-logo.png" alt="AGILANG">
                <span>AGILANG</span>
            </a>
            <h1>Sign in</h1>
            <p>Use your local AGIDB-backed account to open the application dashboard.</p>
            {{ error_message }}
            <form method="POST" action="/login" class="auth-form">
                {{ csrf_field }}
                <label>Email<input type="email" name="email" required autocomplete="email"></label>
                <label>Password<input type="password" name="password" required autocomplete="current-password"></label>
                <button class="btn primary" type="submit">Sign in</button>
            </form>
            <p class="auth-alt">No account yet? <a href="/register">Create one</a></p>
        </section>
    </main>
</body>
</html>
"#,
        ),
        (
            "resources/views/auth/register.ags",
            r#"@page title="Create account" robots="noindex,nofollow"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{ title }}</title>
    <link rel="stylesheet" href="/assets/css/app.css">
    <script src="/assets/js/app.js" defer></script>
</head>
<body>
    <main class="auth-shell">
        <section class="auth-panel">
            <a class="auth-brand" href="/">
                <img src="/assets/images/agilang-logo.png" alt="AGILANG">
                <span>AGILANG</span>
            </a>
            <h1>Create account</h1>
            <p>New users are stored in the local portable AGIDB database.</p>
            {{ error_message }}
            <form method="POST" action="/register" class="auth-form">
                {{ csrf_field }}
                <label>Name<input type="text" name="name" required autocomplete="name"></label>
                <label>Email<input type="email" name="email" required autocomplete="email"></label>
                <label>Password<input type="password" name="password" required autocomplete="new-password"></label>
                <button class="btn primary" type="submit">Create account</button>
            </form>
            <p class="auth-alt">Already registered? <a href="/login">Sign in</a></p>
        </section>
    </main>
</body>
</html>
"#,
        ),
        (
            "resources/views/dashboard/user.ags",
            r#"@page title="Dashboard" robots="noindex,nofollow"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{ title }}</title>
    <link rel="stylesheet" href="/assets/css/app.css">
    <script src="/assets/js/app.js" defer></script>
</head>
<body>
    <main class="dashboard-shell">
        <aside class="dashboard-nav">
            <a class="auth-brand" href="/">
                <img src="/assets/images/agilang-logo.png" alt="AGILANG">
                <span>AGILANG</span>
            </a>
            <a class="active" href="/dashboard">Overview</a>
            <a href="/dashboard/admin">Admin</a>
            <form method="POST" action="/logout" class="inline-logout">
                <input type="hidden" name="_csrf" value="{{ csrf_token }}">
                <button class="btn primary" type="submit">Sign out</button>
            </form>
        </aside>
        <section class="dashboard-main">
            <div class="dashboard-head">
                <div>
                    <p class="kicker">Local application</p>
                    <h1>{{ title }}</h1>
                </div>
                <span class="badge">AGIDB active</span>
            </div>
            <div class="dashboard-grid">
                <article class="card metric"><span>Database</span><strong>{{ database }}</strong><small>Portable AGIDB local store</small></article>
                <article class="card metric"><span>Authentication</span><strong>Enabled</strong><small>Users and sessions are app-owned</small></article>
                <article class="card metric"><span>Current user</span><strong>{{ user_name }}</strong><small>Rendered from the auth context</small></article>
            </div>
            <section class="card dashboard-panel">
                <h2>Recent activity</h2>
                <table>
                    <thead><tr><th>Event</th><th>Status</th><th>Source</th></tr></thead>
                    <tbody>
                        <tr><td>Application boot</td><td>Ready</td><td>AGILANG runtime</td></tr>
                        <tr><td>Database check</td><td>Writable</td><td>AGIDB</td></tr>
                        <tr><td>Dashboard render</td><td>Complete</td><td>AGS</td></tr>
                    </tbody>
                </table>
            </section>
        </section>
    </main>
</body>
</html>
"#,
        ),
        (
            "resources/views/dashboard/admin.ags",
            r#"@page title="Admin Dashboard" robots="noindex,nofollow"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{ title }}</title>
    <link rel="stylesheet" href="/assets/css/app.css">
    <script src="/assets/js/app.js" defer></script>
</head>
<body>
    <main class="dashboard-shell">
        <aside class="dashboard-nav">
            <a class="auth-brand" href="/">
                <img src="/assets/images/agilang-logo.png" alt="AGILANG">
                <span>AGILANG</span>
            </a>
            <a href="/dashboard">Overview</a>
            <a class="active" href="/dashboard/admin">Admin</a>
            <form method="POST" action="/logout" class="inline-logout">
                <input type="hidden" name="_csrf" value="{{ csrf_token }}">
                <button class="btn primary" type="submit">Sign out</button>
            </form>
        </aside>
        <section class="dashboard-main">
            <div class="dashboard-head">
                <div>
                    <p class="kicker">Administration</p>
                    <h1>{{ title }}</h1>
                </div>
                <span class="badge">AGIDB active</span>
            </div>
            <div class="dashboard-grid">
                <article class="card metric"><span>Users table</span><strong>Ready</strong><small>{{ database }}</small></article>
                <article class="card metric"><span>Sessions table</span><strong>Ready</strong><small>Local session persistence</small></article>
                <article class="card metric"><span>Storage mode</span><strong>Portable</strong><small>No external database required</small></article>
            </div>
        </section>
    </main>
</body>
</html>
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

test "version returns 0.5.0":
    let service = ApplicationService()
    assert(service.get_version() == "0.4.0")
"#,
        ),
        // Keep the application generator's default frontend in standalone files.
        // These entries intentionally override the legacy inline scaffold above.
        ("resources/views/welcome.ags", DEFAULT_WEB_VIEW),
        ("public/assets/css/app.css", DEFAULT_WEB_CSS),
        ("public/assets/js/app.js", DEFAULT_WEB_JS),
        ("storage/cache/.gitkeep", ""),
        ("storage/database/.gitkeep", ""),
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
            .replace("{name}", &project_name)
            .replace("{template}", template)
            .replace("{toolchain_version}", env!("CARGO_PKG_VERSION"));
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

    let logo_path = path.join(".agilang/branding/agilang-logo.png");
    if let Some(parent) = logo_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &logo_path,
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/branding/agilang-logo.png"
        )),
    )?;

    let hosted_logo_path = path.join("public/assets/images/agilang-logo.png");
    if let Some(parent) = hosted_logo_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(hosted_logo_path, DEFAULT_WEB_LOGO)?;

    let database_path = path.join("storage/database/main.agidb");
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(database_path, DEFAULT_AGIDB_STORE)?;

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

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (index, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            out.push('_');
        }
        if ch.is_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn ensure_parent_dir(path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn write_generated_file(path: &PathBuf, content: &str, force: bool, label: &str) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} `{}` already exists. Re-run with --force to overwrite it.",
            label,
            path.display()
        );
    }
    ensure_parent_dir(path)?;
    fs::write(path, content)?;
    Ok(())
}

fn nested_namespace(root: &str, file_name: &str) -> String {
    let ns_parts = file_name.split('/').collect::<Vec<_>>();
    if ns_parts.len() > 1 {
        format!("{root}.{}", ns_parts[..ns_parts.len() - 1].join("."))
    } else {
        root.to_string()
    }
}

fn pluralize(word: &str) -> String {
    if word.ends_with('s') {
        format!("{word}es")
    } else if word.ends_with('y') && word.len() > 1 {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join("agilang.toml").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    bail!("not in an AGILANG project (agilang.toml not found)")
}

pub fn install_http_client(force: bool) -> Result<()> {
    let project_root = find_project_root()?;
    let files = [
        ("app/Http/HttpClient.agi", HTTP_CLIENT_APP_WRAPPER),
        (
            "docs/HTTP_CLIENT_RUNTIME_CONTRACT.md",
            HTTP_CLIENT_RUNTIME_CONTRACT,
        ),
    ];

    let mut exists = false;
    for (relative_path, _) in &files {
        if project_root.join(relative_path).exists() {
            exists = true;
            break;
        }
    }

    if exists && !force {
        bail!(
            "HTTP client files already exist.\n\nUse:\n    agilang http-client --force"
        );
    }

    for (relative_path, content) in &files {
        let path = project_root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        println!("Created {}", relative_path);
    }

    Ok(())
}

pub fn make_component(component: &str, name: &str, force: bool) -> Result<()> {
    let project_root = find_project_root()?;
    let normalized_name = normalize_path_name(name);
    let parts: Vec<&str> = normalized_name.split('/').collect();
    let class_name = parts.last().cloned().unwrap_or("");
    let resource_name = class_name.trim_end_matches("Controller").trim_end_matches("Request");
    let resource_pascal = to_pascal_case(resource_name);
    let resource_snake = to_snake_case(&resource_pascal);
    let resource_table = pluralize(&resource_snake);

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
            let path = project_root.join("app/Controllers").join(format!("{}.agi", file_name));
            let ns = nested_namespace("App.Controllers", &file_name);
            let content = format!(
                "module {ns}\n\nuse App.Models.{resource_pascal}\nuse App.Requests.Store{resource_pascal}Request\nuse App.Requests.Update{resource_pascal}Request\nuse Framework.Http.Request\nuse Framework.Http.Response\nuse Framework.Validation.Validator\n\nclass {actual_class_name}:\n    fn index(request: Request) -> Response:\n        return Response.json({{\n            \"resource\": \"{resource_table}\",\n            \"repository\": \"orm\",\n            \"action\": \"index\"\n        }})\n\n    fn show(request: Request, id: i64) -> Response:\n        return Response.json({{\n            \"resource\": \"{resource_table}\",\n            \"repository\": \"orm\",\n            \"action\": \"show\",\n            \"id\": id\n        }})\n\n    fn store(request: Store{resource_pascal}Request) -> Response:\n        let payload = request.body()\n        Validator.validate(payload, {{\n            \"name\": [\"required\", \"string\", \"max:255\"]\n        }})\n        return Response.status(201).json({{\n            \"resource\": \"{resource_table}\",\n            \"repository\": \"orm\",\n            \"action\": \"store\",\n            \"validated_fields\": [\"name\"]\n        }})\n\n    fn update(request: Update{resource_pascal}Request, id: i64) -> Response:\n        let payload = request.body()\n        Validator.validate(payload, {{\n            \"name\": [\"required\", \"string\", \"max:255\"]\n        }})\n        return Response.json({{\n            \"resource\": \"{resource_table}\",\n            \"repository\": \"orm\",\n            \"action\": \"update\",\n            \"id\": id,\n            \"validated_fields\": [\"name\"]\n        }})\n\n    fn destroy(request: Request, id: i64) -> Response:\n        return Response.json({{\n            \"resource\": \"{resource_table}\",\n            \"repository\": \"orm\",\n            \"action\": \"destroy\",\n            \"id\": id,\n            \"authorization\": \"required\"\n        }})\n\n    fn restore(request: Request, id: i64) -> Response:\n        return Response.json({{\n            \"resource\": \"{resource_table}\",\n            \"repository\": \"orm\",\n            \"action\": \"restore\",\n            \"id\": id,\n            \"authorization\": \"required\"\n        }})\n"
            );
            write_generated_file(&path, &content, force, "controller")?;
            println!("Created controller app/Controllers/{}.agi", file_name);
        }
        "model" => {
            let path = project_root.join("app/Models").join(format!("{}.agi", normalized_name));
            let ns = nested_namespace("App.Models", &normalized_name);
            let content = format!(
                "module {ns}\n\nuse Framework.Database.Model\n\nclass {resource_pascal} extends Model:\n    table = \"{resource_table}\"\n    primary_key = \"id\"\n    fillable = [\"name\"]\n    hidden = []\n    casts = {{\n        \"id\": \"integer\"\n    }}\n    timestamps = true\n    soft_deletes = true\n"
            );
            write_generated_file(&path, &content, force, "model")?;
            println!("Created model app/Models/{}.agi", normalized_name);
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
            let path = project_root.join("app/Middleware").join(format!("{}.agi", file_name));
            let ns = nested_namespace("App.Middleware", &file_name);
            let content = format!(
                "module {ns}\n\nuse Framework.Http.Request\nuse Framework.Http.Response\nuse Framework.Middleware.Next\n\nclass {actual_class_name}:\n    fn handle(request: Request, next: Next) -> Response:\n        return next(request)\n"
            );
            write_generated_file(&path, &content, force, "middleware")?;
            println!("Created middleware app/Middleware/{}.agi", file_name);
        }
        "request" => {
            let file_name = if class_name.ends_with("Request") {
                normalized_name.clone()
            } else {
                format!("{}Request", normalized_name)
            };
            let actual_class_name = if class_name.ends_with("Request") {
                class_name.to_string()
            } else {
                format!("{}Request", class_name)
            };
            let path = project_root.join("app/Requests").join(format!("{}.agi", file_name));
            let ns = nested_namespace("App.Requests", &file_name);
            let content = format!(
                "module {ns}\n\nuse Framework.Validation.FormRequest\n\nclass {actual_class_name} extends FormRequest:\n    fn authorize() -> bool:\n        return true\n\n    fn rules() -> map:\n        return {{\n            \"name\": [\"required\", \"string\", \"max:255\"]\n        }}\n\n    fn validated_fields() -> list:\n        return [\"name\"]\n"
            );
            write_generated_file(&path, &content, force, "request")?;
            println!("Created request app/Requests/{}.agi", file_name);
        }
        "policy" => {
            let file_name = if class_name.ends_with("Policy") {
                normalized_name.clone()
            } else {
                format!("{}Policy", normalized_name)
            };
            let actual_class_name = if class_name.ends_with("Policy") {
                class_name.to_string()
            } else {
                format!("{}Policy", class_name)
            };
            let path = project_root.join("app/Policies").join(format!("{}.agi", file_name));
            let ns = nested_namespace("App.Policies", &file_name);
            let content = format!(
                "module {ns}\n\nclass {actual_class_name}:\n    fn view_any(user: map) -> bool:\n        return user.get(\"role\", \"\") == \"admin\"\n\n    fn view(user: map, model: map) -> bool:\n        return user.get(\"role\", \"\") == \"admin\"\n\n    fn create(user: map) -> bool:\n        return user.get(\"role\", \"\") == \"admin\"\n\n    fn update(user: map, model: map) -> bool:\n        return user.get(\"role\", \"\") == \"admin\"\n\n    fn delete(user: map, model: map) -> bool:\n        return user.get(\"role\", \"\") == \"admin\"\n"
            );
            write_generated_file(&path, &content, force, "policy")?;
            println!("Created policy app/Policies/{}.agi", file_name);
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
            let path = project_root.join("app/Services").join(format!("{}.agi", file_name));
            let ns = nested_namespace("App.Services", &file_name);
            let content = format!(
                "module {ns}\n\nclass {actual_class_name}:\n    fn handle() -> void:\n        pass\n"
            );
            write_generated_file(&path, &content, force, "service")?;
            println!("Created service app/Services/{}.agi", file_name);
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
            let path = project_root.join("app/Providers").join(format!("{}.agi", file_name));
            let ns = nested_namespace("App.Providers", &file_name);
            let content = format!(
                "module {ns}\n\nuse Framework.Container.Container\nuse Framework.Providers.ServiceProvider\n\nclass {actual_class_name} extends ServiceProvider:\n    fn register(container: Container) -> void:\n        pass\n\n    fn boot() -> void:\n        pass\n"
            );
            write_generated_file(&path, &content, force, "provider")?;
            println!("Created provider app/Providers/{}.agi", file_name);
        }
        "view" => {
            let path = project_root.join("resources/views").join(format!("{}.ags", normalized_name));
            let content = format!("<!-- View template {} -->\n", normalized_name);
            write_generated_file(&path, &content, force, "view")?;
            println!("Created view resources/views/{}.ags", normalized_name);
        }
        "route" => {
            let path = project_root.join("routes").join(format!("{}.agi", normalized_name));
            let content = format!(
                "use Framework.Routing.Route\n\nfn register_{}() -> void:\n    pass\n",
                class_name.to_lowercase()
            );
            write_generated_file(&path, &content, force, "route file")?;
            println!("Created routes/{}.agi", normalized_name);
        }
        "migration" => {
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
            let migration_class = to_pascal_case(class_name);
            let file_name = format!("{}_{}", timestamp, migration_class);
            let path = project_root.join("database/migrations").join(format!("{}.agi", file_name));
            let content = format!(
                "use Framework.Database.Migration\nuse Framework.Database.Schema\n\nclass {migration_class} extends Migration:\n    fn up() -> void:\n        Schema.create(\"{resource_table}\", fn table:\n            table.id()\n            table.string(\"name\")\n            table.timestamps()\n        )\n\n    fn down() -> void:\n        Schema.drop_if_exists(\"{resource_table}\")\n"
            );
            write_generated_file(&path, &content, force, "migration")?;
            println!("Created migration database/migrations/{}.agi", file_name);
        }
        "seeder" => {
            let file_name = if class_name.ends_with("Seeder") { normalized_name.clone() } else { format!("{}Seeder", normalized_name) };
            let actual_class_name = if class_name.ends_with("Seeder") { class_name.to_string() } else { format!("{}Seeder", class_name) };
            let path = project_root.join("database/seeders").join(format!("{}.agi", file_name));
            let content = format!(
                "use Framework.Database.Seeder\n\nclass {actual_class_name} extends Seeder:\n    fn run() -> void:\n        print(\"Seeding {resource_table}\")\n"
            );
            write_generated_file(&path, &content, force, "seeder")?;
            println!("Created seeder database/seeders/{}.agi", file_name);
        }
        "factory" => {
            let file_name = if class_name.ends_with("Factory") { normalized_name.clone() } else { format!("{}Factory", normalized_name) };
            let actual_class_name = if class_name.ends_with("Factory") { class_name.to_string() } else { format!("{}Factory", class_name) };
            let path = project_root.join("database/factories").join(format!("{}.agi", file_name));
            let content = format!(
                "class {actual_class_name}:\n    fn definition() -> map:\n        return {{\n            \"name\": \"Example {resource_pascal}\",\n            \"created_at\": \"seeded\",\n            \"updated_at\": \"seeded\"\n        }}\n"
            );
            write_generated_file(&path, &content, force, "factory")?;
            println!("Created factory database/factories/{}.agi", file_name);
        }
        "test" => {
            let path = project_root.join("tests/Feature").join(format!("{}Test.agi", normalized_name));
            let content = format!(
                "use Framework.Testing.WebTest\n\ntest \"{} resource scaffold is reachable\":\n    let response = WebTest.get(\"/\")\n    response.assert_status(200)\n    response.assert_contains(\"AGILANG\")\n",
                normalized_name
            );
            write_generated_file(&path, &content, force, "test")?;
            println!("Created test tests/Feature/{}Test.agi", normalized_name);
        }
        "resource" => {
            make_component("model", &resource_pascal, force)?;
            make_component("controller", &format!("{resource_pascal}Controller"), force)?;
            make_component("request", &format!("Store{resource_pascal}"), force)?;
            make_component("request", &format!("Update{resource_pascal}"), force)?;
            make_component("policy", &resource_pascal, force)?;
            make_component("migration", &format!("create_{}_table", resource_table), force)?;
            make_component("factory", &resource_pascal, force)?;
            make_component("seeder", &resource_pascal, force)?;
            make_component("test", &format!("{resource_pascal}Resource"), force)?;
            println!("Created resource bundle for {}", resource_pascal);
        }
        _ => bail!("unknown framework component `{}`", component),
    }

    Ok(())
}

pub fn generate_auth(roles: Vec<String>, force: bool, repair: bool) -> Result<()> {
    let project_root = find_project_root()?;

    let auth_files = [
        (
            "app/Controllers/Auth/LoginController.agi",
            r#"module App.Controllers.Auth

use Framework.Auth.AuthManager
use Framework.Http.Request
use Framework.Http.Response
use Framework.Security.CsrfTokenManager
use Framework.Security.RateLimiter
use Framework.Session.SessionStore
use Framework.Validation.Validator
use Framework.View

class LoginController:
    fn show(request: Request) -> Response:
        return View.render("auth/login", {
            "csrf_field": CsrfTokenManager.generate_field(request.session().csrf_secret)
        })

    fn login(request: Request) -> Response:
        let rate_status = RateLimiter.check("login:" + request.ip())
        if not rate_status.allowed:
            return Response.status(429).json({"error": "Too many failed login attempts. Please try again later."})

        let input = Validator.validate(request.body(), {
            "email": ["required", "email"],
            "password": ["required", "string"]
        })

        let user = AuthManager.authenticate(input.email, input.password)
        if user.is_some():
            let session = request.session().rotate()
            let cookie_header = session.build_cookie_header()
            return Response.redirect("/dashboard").header("Set-Cookie", cookie_header)

        return View.render("auth/login", {
            "error": "The supplied credentials are invalid.",
            "csrf_field": CsrfTokenManager.generate_field(request.session().csrf_secret)
        }).status(422)
"#,
        ),
        (
            "app/Controllers/Auth/RegisterController.agi",
            r#"module App.Controllers.Auth

use Framework.Auth.AuthManager
use Framework.Auth.HmacSha256PasswordHasher
use Framework.Database.DB
use Framework.Http.Request
use Framework.Http.Response
use Framework.Security.CsrfTokenManager
use Framework.Validation.Validator
use Framework.View

class RegisterController:
    fn show(request: Request) -> Response:
        return View.render("auth/register", {
            "csrf_field": CsrfTokenManager.generate_field(request.session().csrf_secret)
        })

    fn register(request: Request) -> Response:
        let input = Validator.validate(request.body(), {
            "name": ["required", "string", "min:2"],
            "email": ["required", "email"],
            "password": ["required", "string", "min:8", "confirmed"]
        })

        let existing = DB.table("users").where("email", "=", input.email).first()
        if existing.is_some():
            return View.render("auth/register", {
                "error": "The email address is already registered.",
                "csrf_field": CsrfTokenManager.generate_field(request.session().csrf_secret)
            }).status(422)

        let hasher = HmacSha256PasswordHasher.new()
        let password_hash = hasher.hash(input.password)

        let user = DB.table("users").insert({
            "name": input.name,
            "email": input.email,
            "password_hash": password_hash,
            "role": "user"
        })

        let session = request.session().rotate()
        let cookie_header = session.build_cookie_header()
        return Response.redirect("/dashboard").header("Set-Cookie", cookie_header)
"#,
        ),
        (
            "app/Controllers/Auth/LogoutController.agi",
            r#"module App.Controllers.Auth

use Framework.Http.Request
use Framework.Http.Response
use Framework.Session.CookieConfig

class LogoutController:
    fn logout(request: Request) -> Response:
        request.session().invalidate()
        let logout_cookie = CookieConfig.default().build_logout_header()
        return Response.redirect("/login").header("Set-Cookie", logout_cookie)
"#,
        ),
        (
            "app/Controllers/Auth/PasswordController.agi",
            r#"module App.Controllers.Auth

use Framework.Http.Request
use Framework.Http.Response

class PasswordController:
    fn reset(request: Request) -> Response:
        return Response.json({"message": "If the account exists, a password reset link has been dispatched."})
"#,
        ),
        (
            "app/Middleware/Authenticate.agi",
            r#"module App.Middleware

use Framework.Http.Request
use Framework.Http.Response

class Authenticate:
    fn handle(request: Request) -> Response:
        if not request.session().is_authenticated():
            return Response.redirect("/login")
        return null
"#,
        ),
        (
            "app/Middleware/GuestOnly.agi",
            r#"module App.Middleware

use Framework.Http.Request
use Framework.Http.Response

class GuestOnly:
    fn handle(request: Request) -> Response:
        if request.session().is_authenticated():
            return Response.redirect("/dashboard")
        return null
"#,
        ),
        (
            "app/Middleware/RequireRole.agi",
            r#"module App.Middleware

use Framework.Http.Request
use Framework.Http.Response

class RequireRole:
    let required_role: string

    fn handle(request: Request) -> Response:
        if not request.user().has_role(self.required_role):
            return Response.status(403).json({"error": "Forbidden: insufficient permissions."})
        return null
"#,
        ),
        (
            "app/Models/User.agi",
            r#"module App.Models

use Framework.Database.Model

class User extends Model:
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

use Framework.Database.Model

class Role extends Model:
    let id: i64
    let name: string
"#,
        ),
        (
            "app/Services/AuthService.agi",
            r#"module App.Services

use Framework.Auth.AuthManager

class AuthService:
    fn check() -> bool:
        return AuthManager.check()
"#,
        ),
        (
            "app/Services/PasswordHasher.agi",
            r#"module App.Services

use Framework.Auth.HmacSha256PasswordHasher

class PasswordHasher:
    fn hash(password: string) -> string:
        return HmacSha256PasswordHasher.new().hash(password)
"#,
        ),
        (
            "app/Services/SessionService.agi",
            r#"module App.Services

use Framework.Session.SessionStore

class SessionService:
    fn regenerate() -> void:
        SessionStore.rotate()
"#,
        ),
        (
            "resources/views/auth/login.ags",
            r#"<h1>Log In to AGILANG</h1>
<form method="POST" action="/login">
    {{ csrf_field }}
    <div class="form-group">
        <label for="email">Email Address</label>
        <input type="email" id="email" name="email" required placeholder="you@example.com">
    </div>
    <div class="form-group">
        <label for="password">Password</label>
        <input type="password" id="password" name="password" required>
    </div>
    <button type="submit" class="button primary">Log In</button>
</form>
"#,
        ),
        (
            "resources/views/auth/register.ags",
            r#"<h1>Create your AGILANG Account</h1>
<form method="POST" action="/register">
    {{ csrf_field }}
    <div class="form-group">
        <label for="name">Full Name</label>
        <input type="text" id="name" name="name" required placeholder="Jane Doe">
    </div>
    <div class="form-group">
        <label for="email">Email Address</label>
        <input type="email" id="email" name="email" required placeholder="jane@example.com">
    </div>
    <div class="form-group">
        <label for="password">Password</label>
        <input type="password" id="password" name="password" required>
    </div>
    <div class="form-group">
        <label for="password_confirmation">Confirm Password</label>
        <input type="password" id="password_confirmation" name="password_confirmation" required>
    </div>
    <button type="submit" class="button primary">Create Account</button>
</form>
"#,
        ),
        (
            "resources/views/auth/forgot-password.ags",
            r#"<h1>Reset Password</h1>
<form method="POST" action="/forgot-password">
    {{ csrf_field }}
    <div class="form-group">
        <label for="email">Email Address</label>
        <input type="email" id="email" name="email" required>
    </div>
    <button type="submit" class="button primary">Send Reset Link</button>
</form>
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
    let project_root = find_project_root()?;

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
version = "0.5.0"

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

[branding]
logo = ".agilang/branding/agilang-logo.png"
language_id = "agilang"
template_language_id = "agilang-ags"

[editor]
syntax_grammar = ".agilang/editor/agilang.tmLanguage.json"
ags_syntax_grammar = ".agilang/editor/ags.tmLanguage.json"

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

Created with the native AGILANG CLI. This application runs with AGI backend code, AGS reactive frontend templates, generated browser JavaScript where required, and the Rust native runtime. Python is not required.

The official logo and syntax definitions are stored under `.agilang/`.
"#,
        ),
        (
            ".vscode/settings.json",
            r#"{
  "files.associations": {
    "*.agi": "agilang",
    "*.ags": "agilang-ags"
  }
}
"#,
        ),
        (
            ".vscode/extensions.json",
            r#"{
  "recommendations": ["global-fintech.agilang-language-support"]
}
"#,
        ),
        (
            ".agilang/editor/agilang.tmLanguage.json",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../editor/vscode-agilang/syntaxes/agilang.tmLanguage.json"
            )),
        ),
        (
            ".agilang/editor/ags.tmLanguage.json",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../editor/vscode-agilang/syntaxes/ags.tmLanguage.json"
            )),
        ),
        (
            "bootstrap/app.agi",
            r#"use Framework.Application
use App.Providers.AppServiceProvider
use App.Providers.RouteServiceProvider
use Native.Path

fn bootstrap() -> Application:
    let app = Application.create(Native.Path.cwd())

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
            "version": "0.5.0"
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
        return "0.5.0"
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

test "version returns 0.5.0":
    let service = ApplicationService()
    assert(service.get_version() == "0.4.0")
"#,
        ),
        // Keep template repair aligned with newly generated applications.
        ("resources/views/welcome.ags", DEFAULT_WEB_VIEW),
        ("public/assets/css/app.css", DEFAULT_WEB_CSS),
        ("public/assets/js/app.js", DEFAULT_WEB_JS),
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
    let hosted_logo_relative = "public/assets/images/agilang-logo.png";
    let hosted_logo_missing = !project_root.join(hosted_logo_relative).is_file();

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

    if missing.is_empty() && empty.is_empty() && !hosted_logo_missing && !force {
        println!("All template files are intact and verified!");
        return Ok(());
    }

    if !missing.is_empty() {
        println!("Missing:");
        for m in &missing {
            println!("  {}", m);
        }
    }
    if hosted_logo_missing {
        println!("Missing:\n  {}", hosted_logo_relative);
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

    if hosted_logo_missing || force {
        let hosted_logo_path = project_root.join(hosted_logo_relative);
        if let Some(parent) = hosted_logo_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(hosted_logo_path, DEFAULT_WEB_LOGO)?;
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

pub fn validate_ai_context() -> Result<()> {
    let project_root = find_project_root()?;

    println!("AGILANG AI context validation\n");

    let required_files = get_ai_docs();
    let mut all_valid = true;

    for (relative_path, _) in &required_files {
        let path = project_root.join(relative_path);
        let name = relative_path;
        let mut status = "PASS".to_string();

        if !path.exists() {
            status = "FAIL (Missing)".to_string();
            all_valid = false;
        } else if let Ok(metadata) = std::fs::metadata(&path) {
            if metadata.len() == 0 {
                status = "FAIL (Empty)".to_string();
                all_valid = false;
            } else if relative_path.ends_with(".json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if serde_json::from_str::<serde_json::Value>(&content).is_err() {
                        status = "FAIL (Invalid JSON)".to_string();
                        all_valid = false;
                    }
                }
            }
        }

        println!("{:<32} {}", name, status);
    }

    if all_valid {
        println!("\nStatus: valid");
        Ok(())
    } else {
        bail!("\nStatus: invalid AI context");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let unique = format!(
            "{}-{}",
            prefix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn install_http_client_writes_wrapper_and_contract() {
        let root = unique_temp_dir("agilang-http-client-test");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("agilang.toml"), "[project]\nname = \"test\"\n").unwrap();

        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let result = install_http_client(false);

        std::env::set_current_dir(previous).unwrap();

        assert!(result.is_ok());
        assert!(root.join("app/Http/HttpClient.agi").exists());
        assert!(root.join("docs/HTTP_CLIENT_RUNTIME_CONTRACT.md").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resource_generation_emits_orm_aware_files() {
        let root = unique_temp_dir("agilang-resource-test");
        generate_project(root.to_string_lossy().as_ref(), "web").unwrap();

        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = make_component("resource", "Post", true);
        std::env::set_current_dir(previous).unwrap();

        assert!(result.is_ok());

        let model = fs::read_to_string(root.join("app/Models/Post.agi")).unwrap();
        assert!(model.contains("soft_deletes = true"));
        assert!(model.contains("\"id\": \"integer\""));

        let controller = fs::read_to_string(root.join("app/Controllers/PostController.agi")).unwrap();
        assert!(controller.contains("\"repository\": \"orm\""));
        assert!(controller.contains("validated_fields"));
        assert!(controller.contains("fn restore"));

        let request = fs::read_to_string(root.join("app/Requests/StorePostRequest.agi")).unwrap();
        assert!(request.contains("fn validated_fields() -> list:"));

        let test_file = fs::read_to_string(root.join("tests/Feature/PostResourceTest.agi")).unwrap();
        assert!(test_file.contains("Framework.Testing.WebTest"));

        let _ = fs::remove_dir_all(root);
    }
}
