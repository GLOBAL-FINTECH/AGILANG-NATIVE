use agilang_framework_http::HttpMethod;
use std::collections::HashMap;

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

#[derive(Debug, Clone)]
pub struct RouteMatch<'a> {
    pub route: &'a Route,
    pub params: HashMap<String, String>,
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

    pub fn match_route<'a>(&'a self, method: &HttpMethod, path: &str) -> Option<RouteMatch<'a>> {
        self.routes.iter().find_map(|route| {
            if &route.method != method {
                return None;
            }
            match_path(&route.path, path).map(|params| RouteMatch { route, params })
        })
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

            if let Some(method) = parse_method(line) {
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

                                self.add(method.clone(), full_path, controller, action);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn parse_method(line: &str) -> Option<HttpMethod> {
    if line.contains("Route.get(") {
        Some(HttpMethod::Get)
    } else if line.contains("Route.post(") {
        Some(HttpMethod::Post)
    } else if line.contains("Route.put(") {
        Some(HttpMethod::Put)
    } else if line.contains("Route.patch(") {
        Some(HttpMethod::Patch)
    } else if line.contains("Route.delete(") {
        Some(HttpMethod::Delete)
    } else {
        None
    }
}

fn match_path(pattern: &str, path: &str) -> Option<HashMap<String, String>> {
    let pattern_segments = split_segments(pattern);
    let path_segments = split_segments(path);
    if pattern_segments.len() != path_segments.len() {
        return None;
    }

    let mut params = HashMap::new();
    for (pattern_segment, path_segment) in pattern_segments.iter().zip(path_segments.iter()) {
        if let Some(name) = pattern_segment
            .strip_prefix('{')
            .and_then(|segment| segment.strip_suffix('}'))
        {
            params.insert(name.to_string(), path_segment.to_string());
            continue;
        }
        if pattern_segment != path_segment {
            return None;
        }
    }
    Some(params)
}

fn split_segments(path: &str) -> Vec<&str> {
    if path == "/" {
        return Vec::new();
    }
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_routes() {
        let temp_dir = std::env::temp_dir().join("agi_route_test");
        fs::create_dir_all(&temp_dir).unwrap();

        let web_agi = "Route.get(\"/\", HomeController.index)\nRoute.post(\"/login\", LoginController.login)\nRoute.put(\"/posts/{id}\", PostController.update)\nRoute.group(\"/api\", fn:\n    Route.get(\"/health\", HealthController.show)\n)\n";
        let file_path = temp_dir.join("web.agi");
        fs::write(&file_path, web_agi).unwrap();

        let mut router = Router::new();
        router.load_routes_from_file(&file_path).unwrap();

        assert_eq!(router.routes.len(), 4);
        assert_eq!(router.routes[0].path, "/");
        assert_eq!(router.routes[0].controller, "HomeController");
        assert_eq!(router.routes[0].action, "index");

        assert_eq!(router.routes[1].method, HttpMethod::Post);
        assert_eq!(router.routes[1].path, "/login");
        assert_eq!(router.routes[1].controller, "LoginController");
        assert_eq!(router.routes[1].action, "login");

        assert_eq!(router.routes[2].method, HttpMethod::Put);
        assert_eq!(router.routes[2].path, "/posts/{id}");
        assert_eq!(router.routes[2].controller, "PostController");
        assert_eq!(router.routes[2].action, "update");

        assert_eq!(router.routes[3].path, "/api/health");
        assert_eq!(router.routes[3].controller, "HealthController");
        assert_eq!(router.routes[3].action, "show");

        let route_match = router
            .match_route(&HttpMethod::Put, "/posts/42")
            .expect("route should match");
        assert_eq!(route_match.route.controller, "PostController");
        assert_eq!(route_match.params.get("id").map(String::as_str), Some("42"));

        fs::remove_dir_all(temp_dir).ok();
    }
}
