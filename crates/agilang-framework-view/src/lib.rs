use std::collections::HashMap;
use std::path::PathBuf;

pub struct ViewEngine {
    views_dir: PathBuf,
}

impl ViewEngine {
    pub fn new(views_dir: PathBuf) -> Self {
        ViewEngine { views_dir }
    }

    pub fn render(
        &self,
        view_name: &str,
        data: &HashMap<String, String>,
    ) -> Result<String, String> {
        let view_path = self.views_dir.join(format!("{}.ags", view_name));
        if !view_path.exists() {
            return Err(format!(
                "View `{}` not found at {}",
                view_name,
                view_path.display()
            ));
        }

        let mut content = std::fs::read_to_string(&view_path)
            .map_err(|e| format!("Failed to read view `{}`: {}", view_name, e))?;

        // Check if it extends a layout
        if content.contains("@extends") {
            let layout_name = self.extract_extends(&content)?;
            let layout_path = self.views_dir.join(format!("{}.ags", layout_name));
            if !layout_path.exists() {
                return Err(format!(
                    "Layout `{}` not found at {}",
                    layout_name,
                    layout_path.display()
                ));
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
        self.compile_document(&content)
    }

    fn compile_document(&self, content: &str) -> Result<String, String> {
        if !content.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("@page ") || line.starts_with("@fetch ") || line.starts_with("@live ")
        }) {
            if content.contains("@page ")
                || content.contains("@fetch ")
                || content.contains("@live ")
            {
                return Err("AGS-E3101: UnprocessedDirective".to_string());
            }
            return Ok(content.to_string());
        }

        let registry = agilang_agi_ags_bridge::TypeRegistry::new();
        agilang_ags_compiler::compile_ags_template(agilang_ags_compiler::CompileRequest {
            source: content,
            file_name: "<framework-view>",
            type_registry: &registry,
            initial_state: None,
        })
        .map(|output| output.html)
        .map_err(|error| error.to_string())
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
        let mut cursor = 0;
        while let Some(relative_start) = result[cursor..].find("{{") {
            let start = cursor + relative_start;
            if let Some(end) = result[start..].find("}}") {
                let real_end = start + end;
                let raw_expr = &result[start + 2..real_end];
                let expr = raw_expr.trim();

                let replacement = if expr.contains("??") {
                    let parts: Vec<&str> = expr.split("??").collect();
                    let key = parts[0].trim();
                    let fallback = parts[1].trim().trim_matches('"').trim_matches('\'');
                    Some(
                        data.get(key)
                            .cloned()
                            .unwrap_or_else(|| fallback.to_string()),
                    )
                } else {
                    data.get(expr).cloned()
                };

                if let Some(replacement) = replacement {
                    result.replace_range(start..real_end + 2, &replacement);
                    cursor = start + replacement.len();
                } else {
                    cursor = real_end + 2;
                }
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

    #[test]
    fn consumes_document_directives_and_applies_page_metadata() {
        let engine = ViewEngine::new(PathBuf::new());
        let input = r#"@page title="Native Framework" seo_description="Native applications." robots="index,follow"
@fetch framework from "/api/framework/status"
@live framework from "/api/framework/status" every 2200

<!DOCTYPE html>
<html><head><title>Old title</title></head><body>Ready</body></html>"#;

        let output = engine.compile_document(input).unwrap();
        assert!(!output.contains("@page"));
        assert!(!output.contains("@fetch"));
        assert!(!output.contains("@live"));
        assert!(output.contains("<title>Native Framework</title>"));
        assert!(output.contains("<meta name=\"description\" content=\"Native applications.\">"));
        assert!(output.contains("<meta name=\"robots\" content=\"index,follow\">"));
        assert!(output.starts_with("<!DOCTYPE html>"));
        assert!(output.contains("application/agilang-hydration"));
        assert!(output.contains("\"interval_ms\":2200"));
        assert!(output.contains("pagehide"));
    }

    #[test]
    fn rejects_unprocessed_directives_after_compilation() {
        let engine = ViewEngine::new(PathBuf::new());
        let input = "<html><body><p>@live broken from \"/api/broken\" every 10</p></body></html>";
        assert_eq!(
            engine.compile_document(input).unwrap_err(),
            "AGS-E3101: UnprocessedDirective"
        );
    }
}
