use std::collections::HashMap;
use std::path::PathBuf;

pub struct ViewEngine {
    views_dir: PathBuf,
}

impl ViewEngine {
    pub fn new(views_dir: PathBuf) -> Self {
        ViewEngine { views_dir }
    }

    pub fn render(&self, view_name: &str, data: &HashMap<String, String>) -> Result<String, String> {
        let view_path = self.views_dir.join(format!("{}.ags", view_name));
        if !view_path.exists() {
            return Err(format!("View `{}` not found at {}", view_name, view_path.display()));
        }

        let mut content = std::fs::read_to_string(&view_path)
            .map_err(|e| format!("Failed to read view `{}`: {}", view_name, e))?;

        // Check if it extends a layout
        if content.contains("@extends") {
            let layout_name = self.extract_extends(&content)?;
            let layout_path = self.views_dir.join(format!("{}.ags", layout_name));
            if !layout_path.exists() {
                return Err(format!("Layout `{}` not found at {}", layout_name, layout_path.display()));
            }

            let layout_content = std::fs::read_to_string(&layout_path)
                .map_err(|e| format!("Failed to read layout `{}`: {}", layout_name, e))?;

            // Extract sections from view
            let sections = self.extract_sections(&content);

            // Interpolate yields in layout
            let mut final_content = layout_content;
            for (section_name, section_body) in sections {
                let yield_tag = format!("@yield(\"{}\")", section_name);
                final_content = final_content.replace(&yield_tag, &section_body);
            }

            content = final_content;
        }

        // Interpolate data values
        content = self.interpolate(&content, data);

        Ok(content)
    }

    fn extract_extends(&self, content: &str) -> Result<String, String> {
        if let Some(start) = content.find("@extends(\"") {
            let sub = &content[start + 10..];
            if let Some(end) = sub.find("\")") {
                return Ok(sub[..end].to_string());
            }
        }
        Err("Malformed @extends directive".to_string())
    }

    fn extract_sections(&self, content: &str) -> HashMap<String, String> {
        let mut sections = HashMap::new();
        let mut cursor = 0;

        while let Some(start) = content[cursor..].find("@section(\"") {
            let real_start = cursor + start;
            let sub = &content[real_start + 10..];
            if let Some(end_name) = sub.find("\")") {
                let section_name = sub[..end_name].to_string();
                let body_start = real_start + 10 + end_name + 2;
                if let Some(end_section) = content[body_start..].find("@endsection") {
                    let section_body = &content[body_start..body_start + end_section];
                    sections.insert(section_name, section_body.trim().to_string());
                    cursor = body_start + end_section + 11;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        sections
    }

    fn interpolate(&self, content: &str, data: &HashMap<String, String>) -> String {
        let mut result = content.to_string();
        while let Some(start) = result.find("{{") {
            if let Some(end) = result[start..].find("}}") {
                let real_end = start + end;
                let raw_expr = &result[start + 2..real_end];
                let expr = raw_expr.trim();

                let replacement = if expr.contains("??") {
                    let parts: Vec<&str> = expr.split("??").collect();
                    let key = parts[0].trim();
                    let fallback = parts[1].trim().trim_matches('"').trim_matches('\'');
                    data.get(key).cloned().unwrap_or_else(|| fallback.to_string())
                } else {
                    data.get(expr).cloned().unwrap_or_default()
                };

                result.replace_range(start..real_end + 2, &replacement);
            } else {
                break;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_view_interpolation() {
        let temp_dir = std::env::temp_dir().join("agi_view_test");
        fs::create_dir_all(&temp_dir).unwrap();

        let layout = "<!DOCTYPE html><html><body>@yield(\"content\")</body></html>";
        let view = "@extends(\"layout\")\n@section(\"content\")\n<h1>{{ title ?? \"Default Title\" }}</h1>\n<p>{{ message }}</p>\n@endsection";

        fs::write(temp_dir.join("layout.ags"), layout).unwrap();
        fs::write(temp_dir.join("welcome.ags"), view).unwrap();

        let engine = ViewEngine::new(temp_dir.clone());
        let mut data = HashMap::new();
        data.insert("message".to_string(), "Hello World!".to_string());

        let output = engine.render("welcome", &data).unwrap();
        assert!(output.contains("<h1>Default Title</h1>"));
        assert!(output.contains("<p>Hello World!</p>"));
        assert!(output.contains("<html><body>"));

        fs::remove_dir_all(temp_dir).ok();
    }
}
