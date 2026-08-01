//! Deterministic AGS compilation pipeline.

use agilang_agi_ags_bridge::TypeRegistry;
use agilang_ags_semantic::{SemanticAnalyzer, TypeRegistry as SemanticTypeRegistry};
use agilang_view_ir::{Document, TextBinding, ViewNode};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct CompileRequest<'a> {
    pub source: &'a str,
    pub file_name: &'a str,
    pub type_registry: &'a TypeRegistry,
    pub initial_state: Option<Value>,
}

#[derive(Debug)]
pub struct CompileError {
    code: &'static str,
    message: String,
}

impl CompileError {
    pub fn code(&self) -> &str {
        self.code
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for CompileError {}

#[derive(Debug)]
pub struct DependencyMap(BTreeMap<String, Vec<u32>>);

impl DependencyMap {
    pub fn for_path(&self, path: &str) -> &[u32] {
        self.0.get(path).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn as_json(&self) -> Value {
        json!(self.0)
    }
}

#[derive(Debug)]
pub struct CompileOutput {
    pub html: String,
    pub javascript: String,
    pub view: Value,
    pub dependencies: DependencyMap,
    pub manifest: Value,
    pub state_blocks: LiveStateBlocks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveState {
    Uninitialized,
    Loading,
    Ready,
    Refreshing,
    Stale,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct LiveStateBlocks {
    pub loading: Option<String>,
    pub error: Option<String>,
    pub stale: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveStateMachine {
    state: LiveState,
    last_valid_state: Option<Value>,
}

impl Default for LiveStateMachine {
    fn default() -> Self {
        Self {
            state: LiveState::Uninitialized,
            last_valid_state: None,
        }
    }
}

impl LiveStateMachine {
    pub fn state(&self) -> LiveState {
        self.state
    }
    pub fn last_valid_state(&self) -> Option<&Value> {
        self.last_valid_state.as_ref()
    }

    pub fn transition(&mut self, next: LiveState) -> Result<(), String> {
        let valid = matches!(
            (self.state, next),
            (LiveState::Uninitialized, LiveState::Loading)
                | (LiveState::Loading, LiveState::Ready)
                | (LiveState::Ready, LiveState::Refreshing)
                | (LiveState::Refreshing, LiveState::Ready | LiveState::Stale)
                | (LiveState::Stale, LiveState::Ready | LiveState::Error)
                | (LiveState::Error, LiveState::Loading)
        );
        if !valid {
            return Err(format!(
                "invalid live-state transition: {:?} -> {:?}",
                self.state, next
            ));
        }
        self.state = next;
        Ok(())
    }

    pub fn accept(&mut self, value: Value) -> Result<(), String> {
        self.last_valid_state = Some(value);
        self.transition(LiveState::Ready)
    }
}

pub fn compile_ags(request: CompileRequest<'_>) -> Result<CompileOutput, CompileError> {
    // State-block lowering is a later pipeline stage. The primary document is compiled here;
    // keeping the boundary explicit prevents error-scope bindings from leaking into page scope.
    let primary_source = request
        .source
        .split("\n@loading")
        .next()
        .unwrap_or(request.source);
    let tokens = agilang_ags_lexer::tokenize(primary_source)
        .map_err(|message| error("AGS-E1001", request.file_name, message))?;
    let ast = agilang_ags_parser::parse(tokens)
        .map_err(|message| error("AGS-E1101", request.file_name, message))?;
    let analyzer = SemanticAnalyzer::new(SemanticTypeRegistry::from_bridge(
        request.type_registry.clone(),
    ));
    let ir = analyzer.analyze(&ast).map_err(|message| {
        let code = if message.starts_with("AGS-E2301") {
            "AGS-E2301"
        } else {
            "AGS-E2001"
        };
        error(code, request.file_name, message)
    })?;

    let initial = request.initial_state.unwrap_or_else(|| json!({}));
    let state_blocks = extract_state_blocks(request.source);
    let dependencies = dependencies(&ir);
    let html = render_document(&ir, &initial);
    reject_unprocessed_directives(&html, request.file_name)?;
    let javascript = hydration_javascript(&ir, &initial, &dependencies, &state_blocks);
    let manifest = manifest(&ir);
    let view = view_json(&ir);
    Ok(CompileOutput {
        html,
        javascript,
        view,
        dependencies,
        manifest,
        state_blocks,
    })
}

/// Compiles a complete HTML-oriented AGS template without reserializing its markup.
///
/// This is the framework view path for production pages containing styles, scripts, SVG, and
/// other browser-native content. The directive preamble still passes through the AGS
/// lexer/parser/semantic pipeline, while the approved HTML body is preserved byte-for-byte
/// apart from metadata and hydration injection.
pub fn compile_ags_template(request: CompileRequest<'_>) -> Result<CompileOutput, CompileError> {
    let (directive_source, template_html) = split_directive_preamble(request.source);
    if directive_source.trim().is_empty() {
        return Err(error(
            "AGS-E1002",
            request.file_name,
            "an AGS template compilation request requires a directive preamble".into(),
        ));
    }

    let parse_source = format!("{}\n<main></main>", directive_source);
    let tokens = agilang_ags_lexer::tokenize(&parse_source)
        .map_err(|message| error("AGS-E1001", request.file_name, message))?;
    let ast = agilang_ags_parser::parse(tokens)
        .map_err(|message| error("AGS-E1101", request.file_name, message))?;
    let analyzer = SemanticAnalyzer::new(SemanticTypeRegistry::from_bridge(
        request.type_registry.clone(),
    ));
    let ir = analyzer.analyze(&ast).map_err(|message| {
        error(
            if message.starts_with("AGS-E2301") {
                "AGS-E2301"
            } else {
                "AGS-E2001"
            },
            request.file_name,
            message,
        )
    })?;

    let initial = request.initial_state.unwrap_or_else(|| json!({}));
    let dependencies = dependencies(&ir);
    let state_blocks = LiveStateBlocks::default();
    let javascript = hydration_javascript(&ir, &initial, &dependencies, &state_blocks);
    let mut manifest = manifest(&ir);
    let view = view_json(&ir);
    let (bound_html, template_bindings) = lower_template_bindings(template_html, &initial);
    manifest["bindings"] = json!(template_bindings);
    let mut html = apply_page_metadata(&bound_html, &ir);
    let state = serde_json::to_string(&initial)
        .expect("JSON values always serialize")
        .replace('<', "\\u003c");
    let manifest_json = serde_json::to_string(&manifest)
        .expect("manifest values always serialize")
        .replace('<', "\\u003c");
    let hydration = format!(
        "<script type=\"application/agilang-state\">{state}</script>\
<script type=\"application/agilang-hydration\">{manifest_json}</script>\
<script>{javascript}</script>"
    );
    html = inject_before(&html, "</body>", &hydration);
    reject_unprocessed_directives(&html, request.file_name)?;

    Ok(CompileOutput {
        html,
        javascript,
        view,
        dependencies,
        manifest,
        state_blocks,
    })
}

fn split_directive_preamble(source: &str) -> (String, &str) {
    let mut directives = String::new();
    let mut body_offset = 0;
    let mut cursor = 0;
    for segment in source.split_inclusive('\n') {
        let trimmed = segment.trim();
        if trimmed.is_empty() || is_compiler_directive(trimmed) {
            if is_compiler_directive(trimmed) {
                directives.push_str(trimmed);
                directives.push('\n');
            }
            cursor += segment.len();
            body_offset = cursor;
            continue;
        }
        break;
    }
    (directives, &source[body_offset..])
}

fn is_compiler_directive(line: &str) -> bool {
    line.starts_with("@page ") || line.starts_with("@fetch ") || line.starts_with("@live ")
}

fn lower_template_bindings(html: &str, initial: &Value) -> (String, Vec<String>) {
    let mut output = String::with_capacity(html.len());
    let mut bindings = Vec::new();
    let mut remainder = html;
    while let Some(start) = remainder.find("{{") {
        output.push_str(&remainder[..start]);
        let expression = &remainder[start + 2..];
        let Some(end) = expression.find("}}") else {
            output.push_str(&remainder[start..]);
            return (output, bindings);
        };
        let path = expression[..end].trim();
        let segments: Vec<&str> = path.split('.').collect();
        if segments.len() >= 2
            && segments
                .iter()
                .all(|segment| segment.chars().all(|ch| ch.is_alphanumeric() || ch == '_'))
        {
            let value = segments[1..]
                .iter()
                .fold(initial.get(segments[0]), |value, segment| {
                    value?.get(segment)
                });
            output.push_str(&format!(
                "<span data-ags-bind=\"{}\">{}</span>",
                escape_html(path),
                escape_html(&display_value(value))
            ));
            if !bindings.iter().any(|binding| binding == path) {
                bindings.push(path.to_string());
            }
        } else {
            output.push_str(&remainder[start..start + 2 + end + 2]);
        }
        remainder = &expression[end + 2..];
    }
    output.push_str(remainder);
    (output, bindings)
}

fn apply_page_metadata(html: &str, doc: &Document) -> String {
    let mut output = set_head_element(
        html,
        "<title>",
        "</title>",
        &format!("<title>{}</title>", escape_html(&doc.title)),
    );
    if let Some(value) = &doc.seo_description {
        output = set_named_meta(&output, "description", value);
    }
    if let Some(value) = &doc.robots {
        output = set_named_meta(&output, "robots", value);
    }
    output
}

fn set_head_element(html: &str, start_marker: &str, end_marker: &str, element: &str) -> String {
    if let (Some(start), Some(end)) = (html.find(start_marker), html.find(end_marker)) {
        let mut output = html.to_string();
        output.replace_range(start..end + end_marker.len(), element);
        output
    } else {
        inject_before(html, "</head>", element)
    }
}

fn set_named_meta(html: &str, name: &str, value: &str) -> String {
    let marker = format!("name=\"{}\"", name);
    if let Some(name_position) = html.find(&marker) {
        if let (Some(start), Some(relative_end)) = (
            html[..name_position].rfind('<'),
            html[name_position..].find('>'),
        ) {
            let mut output = html.to_string();
            output.replace_range(
                start..name_position + relative_end + 1,
                &format!(
                    "<meta name=\"{}\" content=\"{}\">",
                    name,
                    escape_html(value)
                ),
            );
            return output;
        }
    }
    inject_before(
        html,
        "</head>",
        &format!(
            "<meta name=\"{}\" content=\"{}\">",
            name,
            escape_html(value)
        ),
    )
}

fn inject_before(html: &str, marker: &str, content: &str) -> String {
    if let Some(position) = html.rfind(marker) {
        let mut output = html.to_string();
        output.insert_str(position, content);
        output
    } else {
        format!("{html}{content}")
    }
}

fn extract_state_blocks(source: &str) -> LiveStateBlocks {
    fn block(source: &str, directive: &str) -> Option<String> {
        let lines: Vec<&str> = source.lines().collect();
        let start = lines
            .iter()
            .position(|line| line.trim_start().starts_with(directive))?;
        let value = lines[start + 1..]
            .iter()
            .copied()
            .take_while(|line| !line.trim_start().starts_with('@'))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        (!value.is_empty()).then_some(value)
    }
    LiveStateBlocks {
        loading: block(source, "@loading "),
        error: block(source, "@error "),
        stale: block(source, "@stale "),
    }
}

fn error(code: &'static str, file: &str, message: String) -> CompileError {
    CompileError {
        code,
        message: format!("{code}: {message}\n  --> {file}"),
    }
}

fn reject_unprocessed_directives(html: &str, file: &str) -> Result<(), CompileError> {
    const DIRECTIVES: [&str; 3] = ["@page ", "@fetch ", "@live "];
    if let Some(directive) = DIRECTIVES
        .iter()
        .find(|directive| html.contains(**directive))
    {
        return Err(error(
            "AGS-E3101",
            file,
            format!(
                "unprocessed directive `{}` reached rendered HTML",
                directive.trim()
            ),
        ));
    }
    Ok(())
}

fn render_document(doc: &Document, initial: &Value) -> String {
    let mut body = String::new();
    render_node(&doc.root_node, initial, &mut body);
    for source in doc.data_sources.keys() {
        body.push_str(&format!(
            "<aside id=\"ags-state-{}\" data-ags-state-host=\"{}\" aria-live=\"polite\" hidden></aside>",
            escape_html(source), escape_html(source)
        ));
    }
    let description = doc
        .seo_description
        .as_ref()
        .map(|value| {
            format!(
                "<meta name=\"description\" content=\"{}\">",
                escape_html(value)
            )
        })
        .unwrap_or_default();
    let robots = doc
        .robots
        .as_ref()
        .map(|value| format!("<meta name=\"robots\" content=\"{}\">", escape_html(value)))
        .unwrap_or_default();
    let state = serde_json::to_string(initial)
        .expect("JSON values always serialize")
        .replace('<', "\\u003c");
    format!("<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{}</title>{}{}</head><body>{}<script type=\"application/agilang-state\">{}</script></body></html>",
        escape_html(&doc.title), description, robots, body, state)
}

fn render_node(node: &ViewNode, initial: &Value, output: &mut String) {
    output.push('<');
    output.push_str(&node.tag);
    output.push_str(&format!(" id=\"node-{}\"", node.id.0));
    for (name, value) in &node.attributes {
        output.push_str(&format!(" {}=\"{}\"", name, escape_html(value)));
    }
    output.push('>');
    for binding in &node.text_bindings {
        match binding {
            TextBinding::Static(text) => output.push_str(&escape_html(text)),
            TextBinding::Dynamic(binding) => {
                let value = lookup(initial, &binding.source, &binding.field_path);
                output.push_str(&escape_html(&display_value(value)));
            }
        }
    }
    for child in &node.children {
        render_node(child, initial, output);
    }
    output.push_str("</");
    output.push_str(&node.tag);
    output.push('>');
}

fn lookup<'a>(state: &'a Value, source: &str, fields: &[String]) -> Option<&'a Value> {
    let mut value = state.get(source).unwrap_or(state);
    for field in fields {
        value = value.get(field)?;
    }
    Some(value)
}

fn display_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Null) | None => String::new(),
        Some(value) => value.to_string(),
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn dependencies(doc: &Document) -> DependencyMap {
    let mut paths: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    collect_dependencies(&doc.root_node, &mut paths);
    DependencyMap(paths)
}

fn collect_dependencies(node: &ViewNode, paths: &mut BTreeMap<String, Vec<u32>>) {
    for binding in &node.text_bindings {
        if let TextBinding::Dynamic(binding) = binding {
            let path = format!("{}.{}", binding.source, binding.field_path.join("."));
            paths.entry(path).or_default().push(node.id.0);
        }
    }
    for child in &node.children {
        collect_dependencies(child, paths);
    }
}

fn hydration_javascript(
    doc: &Document,
    initial: &Value,
    dependencies: &DependencyMap,
    blocks: &LiveStateBlocks,
) -> String {
    let sources: Vec<Value> = doc
        .data_sources
        .values()
        .map(|source| {
            json!({
                "name": source.name, "endpoint": source.endpoint, "interval_ms": source.interval_ms
            })
        })
        .collect();
    let state_templates = json!({
        "loading": blocks.loading,
        "error": blocks.error,
        "stale": blocks.stale,
    });
    let binding_pattern = json!(r"\{\{\s*([^}]+?)\s*\}\}");
    format!(
        r#"(() => {{
  'use strict';
  const state = {};
  const dependencies = {};
  const sources = {};
  const stateTemplates = {};
  const transitions = {{ Uninitialized: ['Loading'], Loading: ['Ready'], Ready: ['Refreshing'], Refreshing: ['Ready', 'Stale'], Stale: ['Ready', 'Error'], Error: ['Loading'] }};
  const liveStates = Object.fromEntries(sources.map(source => [source.name, state[source.name] ? 'Ready' : 'Uninitialized']));
  const timers = new Map();
  const requests = new Map();
  function escapeHtml(value) {{
    return String(value ?? '').replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;');
  }}
  function resolve(path, source, error) {{
    if (path === 'error.message') return error?.message || 'Live data is temporarily unavailable.';
    const parts = path.split('.');
    let value = parts.shift() === source ? state[source] : undefined;
    for (const part of parts) value = value?.[part];
    return value;
  }}
  function renderState(source, next, error) {{
    const host = document.getElementById(`ags-state-${{source}}`);
    if (!host) return;
    const template = stateTemplates[next.toLowerCase()];
    if (!template || next === 'Ready' || next === 'Refreshing') {{ host.hidden = true; host.replaceChildren(); return; }}
    host.innerHTML = template.replace(new RegExp({}, 'g'), (_, path) => escapeHtml(resolve(path, source, error)));
    host.hidden = false;
  }}
  function transition(source, next, error) {{
    const current = liveStates[source];
    if (!(transitions[current] || []).includes(next)) throw new Error(`invalid live-state transition: ${{current}} -> ${{next}}`);
    liveStates[source] = next;
    document.documentElement.dataset[`ags${{source}}State`] = next;
    renderState(source, next, error);
  }}
  function updateChanged(source, next) {{
    for (const [field, value] of Object.entries(next)) {{
      if (state[source]?.[field] === value) continue;
      state[source] = state[source] || {{}}; state[source][field] = value;
      for (const id of dependencies[`${{source}}.${{field}}`] || []) {{
        const node = document.getElementById(`node-${{id}}`); if (node) node.textContent = value;
      }}
      document.querySelectorAll(`[data-ags-bind="${{source}}.${{field}}"]`).forEach(node => {{ node.textContent = value ?? ''; }});
    }}
  }}
  async function poll(source) {{
    const previous = requests.get(source.name);
    if (previous) previous.abort();
    const controller = new AbortController();
    requests.set(source.name, controller);
    try {{
      if (liveStates[source.name] === 'Error') transition(source.name, 'Loading');
      else if (liveStates[source.name] === 'Uninitialized') transition(source.name, 'Loading');
      else if (liveStates[source.name] === 'Ready') transition(source.name, 'Refreshing');
      const response = await fetch(source.endpoint, {{ signal: controller.signal }});
      if (!response.ok) throw new Error(`HTTP ${{response.status}}`);
      updateChanged(source.name, await response.json());
      transition(source.name, 'Ready');
    }} catch (error) {{
      if (error.name === 'AbortError') return;
      const current = liveStates[source.name];
      if (current === 'Loading') renderState(source.name, 'Error', error);
      else if (current === 'Refreshing') transition(source.name, 'Stale', error);
      else if (current === 'Stale') transition(source.name, 'Error', error);
      console.warn('AGS live source failed; preserving last valid state', source.name, error);
    }} finally {{
      if (requests.get(source.name) === controller) requests.delete(source.name);
    }}
  }}
  function dispose() {{
    for (const timer of timers.values()) clearInterval(timer);
    timers.clear();
    for (const request of requests.values()) request.abort();
    requests.clear();
  }}
  for (const source of sources) {{
    if (liveStates[source.name] === 'Uninitialized') void poll(source);
    if (source.interval_ms > 0) timers.set(source.name, setInterval(() => void poll(source), source.interval_ms));
  }}
  window.addEventListener('pagehide', dispose, {{ once: true }});
  document.addEventListener('ags:dispose', dispose, {{ once: true }});
}})();
"#,
        initial,
        dependencies.as_json(),
        Value::Array(sources),
        state_templates,
        binding_pattern
    )
}

fn manifest(doc: &Document) -> Value {
    let live_sources: Vec<Value> = doc.data_sources.values().map(|source| {
        let response_type = match &source.response_type {
            agilang_view_ir::TypeInfo::Struct { name, .. } => name.clone(),
            _ => "unknown".into(),
        };
        json!({"name": source.name, "type": response_type, "endpoint": source.endpoint, "interval_ms": source.interval_ms})
    }).collect();
    let bindings: Vec<String> = doc
        .all_bindings
        .iter()
        .map(|binding| format!("{}.{}", binding.source, binding.field_path.join(".")))
        .collect();
    json!({"page": {"title": doc.title, "seo_description": doc.seo_description, "robots": doc.robots},
        "live_sources": live_sources, "bindings": bindings})
}

fn view_json(doc: &Document) -> Value {
    fn serialize_node(current: &ViewNode) -> Value {
        json!({"id": current.id.0, "tag": current.tag, "attributes": current.attributes,
            "children": current.children.iter().map(serialize_node).collect::<Vec<_>>()})
    }
    serialize_node(&doc.root_node)
}

pub fn test_chain_state() -> Value {
    json!({"chain": {"chain_id": 1990, "symbol": "SBQ", "status": "online",
        "height": 1450900, "finalized_height": 1450898, "validator_count": 42,
        "transaction_count": 987654, "consensus": "Proof of Stake", "head_hash": "0xabc"}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_agi_ags_bridge::sibaq_type_registry;

    #[test]
    fn compiles_sibaq_dashboard_end_to_end() {
        let source = include_str!("../../../examples/ags/sibaq-dashboard.ags");
        let registry = sibaq_type_registry();
        let output = compile_ags(CompileRequest {
            source,
            file_name: "sibaq-dashboard.ags",
            type_registry: &registry,
            initial_state: Some(test_chain_state()),
        })
        .unwrap();
        assert!(output.html.contains("1990"));
        assert!(output.html.contains("SBQ"));
        assert!(output.html.contains("Smart Chain Dashboard"));
        assert!(!output.html.contains(">--<"));
        assert!(output.javascript.contains("/api/status"));
        assert!(output.javascript.contains("1000"));
        assert_eq!(output.dependencies.for_path("chain.height").len(), 1);
    }

    #[test]
    fn rejects_unknown_sibaq_field() {
        let source =
            "@live chain: ChainStatus from \"/api/status\" every 1000\n<p>{{ chain.heigth }}</p>";
        let registry = sibaq_type_registry();
        let error = compile_ags(CompileRequest {
            source,
            file_name: "bad.ags",
            type_registry: &registry,
            initial_state: None,
        })
        .unwrap_err();
        assert_eq!(error.code(), "AGS-E2301");
        assert!(error.message().contains("height"));
    }

    #[test]
    fn hydration_does_not_duplicate_initial_fetch() {
        let source =
            "@live chain: ChainStatus from \"/api/status\" every 1000\n<p>{{ chain.height }}</p>";
        let registry = sibaq_type_registry();
        let output = compile_ags(CompileRequest {
            source,
            file_name: "test.ags",
            type_registry: &registry,
            initial_state: Some(test_chain_state()),
        })
        .unwrap();
        assert!(!output.javascript.contains("fetch(source.endpoint);\n  for"));
        assert!(output
            .javascript
            .contains("if (state[source]?.[field] === value) continue"));
    }

    #[test]
    fn lowers_reactive_state_blocks() {
        let source = include_str!("../../../examples/ags/sibaq-dashboard.ags");
        let registry = sibaq_type_registry();
        let output = compile_ags(CompileRequest {
            source,
            file_name: "sibaq-dashboard.ags",
            type_registry: &registry,
            initial_state: Some(test_chain_state()),
        })
        .unwrap();
        assert!(output
            .state_blocks
            .loading
            .as_deref()
            .unwrap()
            .contains("Connecting"));
        assert!(output
            .state_blocks
            .error
            .as_deref()
            .unwrap()
            .contains("unavailable"));
        assert!(output
            .state_blocks
            .stale
            .as_deref()
            .unwrap()
            .contains("Last update"));
        assert!(output.javascript.contains("preserving last valid state"));
        assert!(output.javascript.contains("Refreshing: ['Ready', 'Stale']"));
        assert!(output.html.contains("id=\"ags-state-chain\""));
        assert!(output.javascript.contains("function renderState"));
        assert!(output
            .javascript
            .contains("host.innerHTML = template.replace"));
    }

    #[test]
    fn live_state_machine_rejects_impossible_states_and_preserves_data() {
        let mut machine = LiveStateMachine::default();
        assert!(machine.transition(LiveState::Ready).is_err());
        machine.transition(LiveState::Loading).unwrap();
        machine.accept(json!({"height": 10})).unwrap();
        machine.transition(LiveState::Refreshing).unwrap();
        machine.transition(LiveState::Stale).unwrap();
        assert_eq!(machine.last_valid_state().unwrap()["height"], 10);
        machine.transition(LiveState::Ready).unwrap();
    }

    #[test]
    fn missing_initial_state_loads_immediately_without_discarding_runtime_errors() {
        let source = "@live chain: ChainStatus from \"/api/status\" every 1000\n<p>{{ chain.height }}</p>\n@loading chain:\n  <p>Loading</p>\n@error chain as error:\n  <p>{{ error.message }}</p>";
        let registry = sibaq_type_registry();
        let output = compile_ags(CompileRequest {
            source,
            file_name: "state.ags",
            type_registry: &registry,
            initial_state: None,
        })
        .unwrap();
        assert!(output
            .javascript
            .contains("if (liveStates[source.name] === 'Uninitialized') void poll(source)"));
        assert!(output
            .javascript
            .contains("escapeHtml(resolve(path, source, error))"));
        assert!(output
            .javascript
            .contains("renderState(source.name, 'Error', error)"));
    }

    #[test]
    fn compiles_full_html_template_through_directive_pipeline() {
        let source = r#"@page title="Native Framework" seo_description="Native apps." robots="index,follow"
@fetch framework from "/api/framework/status"
@live framework from "/api/framework/status" every 2200
<!DOCTYPE html>
<html><head><title>Old</title></head><body><style>.hero{display:grid}</style><main class="hero">Ready</main></body></html>"#;
        let registry = TypeRegistry::new();
        let output = compile_ags_template(CompileRequest {
            source,
            file_name: "home.ags",
            type_registry: &registry,
            initial_state: None,
        })
        .unwrap();

        assert!(!output.html.contains("@page "));
        assert!(!output.html.contains("@fetch "));
        assert!(!output.html.contains("@live "));
        assert!(output.html.contains("<title>Native Framework</title>"));
        assert!(output
            .html
            .contains("<meta name=\"robots\" content=\"index,follow\">"));
        assert!(output.html.contains(".hero{display:grid}"));
        assert!(output.html.contains("application/agilang-hydration"));
        assert_eq!(output.manifest["live_sources"][0]["interval_ms"], 2200);
        assert!(output.javascript.contains("AbortController"));
        assert!(output.javascript.contains("pagehide"));
        assert!(output.javascript.contains("ags:dispose"));
    }
}
