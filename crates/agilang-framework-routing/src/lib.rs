use agilang_framework_http::HttpMethod;

#[derive(Debug, Clone)]
pub struct Route {
    pub method: HttpMethod,
    pub path: String,
    pub controller: String,
    pub action: String,
}

#[derive(Default)]
pub struct Router {
    pub routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Router { routes: vec![] }
    }

    pub fn add(&mut self, method: HttpMethod, path: String, controller: &str, action: &str) {
        self.routes.push(Route {
            method,
            path,
            controller: controller.to_string(),
            action: action.to_string(),
        });
    }

    pub fn match_route(&self, method: &HttpMethod, path: &str) -> Option<&Route> {
        self.routes
            .iter()
            .find(|r| &r.method == method && r.path == path)
    }

    pub fn load_routes_from_file(&mut self, file_path: &std::path::Path) -> std::io::Result<()> {
        if !file_path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(file_path)?;
        let mut current_prefix = String::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with("Route.group(") {
                if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                    let sub = &line[start + 1..];
                    if let Some(end) = sub.find('"').or_else(|| sub.find('\'')) {
                        current_prefix = sub[..end].to_string();
                    }
                }
            }

            // If a group ends or reset prefix (simple heuristics for minimal flow)
            if line == ")" || line == "}" {
                current_prefix.clear();
            }

            if line.contains("Route.get(") {
                let path_start = line.find('"').or_else(|| line.find('\''));
                if let Some(p_start) = path_start {
                    let sub = &line[p_start + 1..];
                    let path_end = sub.find('"').or_else(|| sub.find('\''));
                    if let Some(p_end) = path_end {
                        let path = &sub[..p_end];
                        let full_path = format!("{}{}", current_prefix, path);

                        if let Some(comma_pos) = line.find(',') {
                            let rhs = line[comma_pos + 1..].trim().trim_matches(')').trim();
                            if let Some(dot_pos) = rhs.rfind('.') {
                                let controller_raw = rhs[..dot_pos].trim();
                                let action = rhs[dot_pos + 1..].trim();

                                // Normalise controller name to match the file system structure
                                // If rhs was App.Controllers.HomeController, dot_pos is after Controllers.
                                // So controller_raw is App.Controllers.HomeController
                                // We want to remove the App.Controllers prefix if present to find it easily
                                let controller = controller_raw
                                    .strip_prefix("App.Controllers.")
                                    .unwrap_or(controller_raw);

                                self.add(HttpMethod::Get, full_path, controller, action);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_routes() {
        let temp_dir = std::env::temp_dir().join("agi_route_test");
        fs::create_dir_all(&temp_dir).unwrap();

        let web_agi = "Route.get(\"/\", HomeController.index)\nRoute.group(\"/api\", fn:\n    Route.get(\"/health\", HealthController.show)\n)\n";
        let file_path = temp_dir.join("web.agi");
        fs::write(&file_path, web_agi).unwrap();

        let mut router = Router::new();
        router.load_routes_from_file(&file_path).unwrap();

        assert_eq!(router.routes.len(), 2);
        assert_eq!(router.routes[0].path, "/");
        assert_eq!(router.routes[0].controller, "HomeController");
        assert_eq!(router.routes[0].action, "index");

        assert_eq!(router.routes[1].path, "/api/health");
        assert_eq!(router.routes[1].controller, "HealthController");
        assert_eq!(router.routes[1].action, "show");

        fs::remove_dir_all(temp_dir).ok();
    }
}
