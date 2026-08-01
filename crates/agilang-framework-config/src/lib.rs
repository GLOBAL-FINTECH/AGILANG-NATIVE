use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub env: HashMap<String, String>,
}

impl Config {
    pub fn load(project_root: &Path) -> Self {
        let mut env = HashMap::new();
        let env_path = project_root.join(".env");
        if env_path.exists() {
            if let Ok(content) = std::fs::read_to_string(env_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some(pos) = line.find('=') {
                        let key = line[..pos].trim().to_string();
                        let val = line[pos + 1..].trim().to_string();
                        env.insert(key, val);
                    }
                }
            }
        }
        Config { env }
    }

    pub fn get(&self, key: &str, default: &str) -> String {
        self.env
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.env
            .get(key)
            .map(|val| val.to_lowercase() == "true")
            .unwrap_or(default)
    }

    pub fn get_i32(&self, key: &str, default: i32) -> i32 {
        self.env
            .get(key)
            .and_then(|val| val.parse::<i32>().ok())
            .unwrap_or(default)
    }
}
