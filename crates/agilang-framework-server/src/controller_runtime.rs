use crate::{
    build_json_response, build_text_response, handle_auth_request, load_session_by_cookie,
    framework_manifest::{load_framework_manifest, ModelManifest, RequestManifest},
    load_user_by_id, parse_form_body, redirect_response, AuthSessionRecord, AuthUserRecord,
};
use agilang_database_driver::{DatabaseConnection, DatabaseDriverKind, DatabaseValue};
use agilang_framework_database::{DatabaseConfig, FrameworkConnection, FrameworkDriver};
use agilang_framework_http::{HttpMethod, Request, Response};
use agilang_framework_routing::{Route, RouteMatch, Router};
use agilang_framework_validation::Validator;
use agilang_framework_view::ViewEngine;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn execute_request(
    router: &Router,
    view_engine: &ViewEngine,
    project_root: &Path,
    req: &Request,
) -> Response {
    let path = req.path();

    if let Some((status, json)) = crate::native_cj_response(req, project_root) {
        return build_json_response(status, json);
    }

    match crate::handle_webrtc_request(req, project_root) {
        Ok(Some(response)) => return response,
        Ok(None) => {}
        Err(error) => {
            return framework_error_response(path, 500, &error);
        }
    }

    if let Some(json) = crate::builtin_api_response(path) {
        return build_json_response(200, json);
    }

    match handle_auth_request(req, view_engine, project_root) {
        Ok(Some(response)) => return response,
        Ok(None) => {}
        Err(error) => {
            return framework_error_response(path, 500, &error);
        }
    }

    let public_path = project_root.join("public").join(path.trim_start_matches('/'));
    let is_ags_source = public_path.extension().and_then(|value| value.to_str()) == Some("ags");
    if public_path.is_file() && !path.ends_with('/') && !is_ags_source {
        return match fs::read(&public_path) {
            Ok(content) => {
                let ext = public_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                let mut response = Response {
                    status: 200,
                    headers: HashMap::new(),
                    body: content,
                };
                response
                    .headers
                    .insert("Content-Type".to_string(), static_mime(ext).to_string());
                response
            }
            Err(error) => framework_error_response(path, 500, &error.to_string()),
        };
    }

    let route_match = match router.match_route(&req.method, path) {
        Some(route_match) => route_match,
        None => return framework_error_response(path, 404, "Route not defined."),
    };

    match dispatch_controller(project_root, req, route_match) {
        Ok(response) => response,
        Err(error) => framework_error_response(path, error.status, &error.message),
    }
}

struct DispatchError {
    status: u16,
    message: String,
}

impl DispatchError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

type ResourceDescriptor = ModelManifest;
type RequestDescriptor = RequestManifest;

fn dispatch_controller(
    project_root: &Path,
    req: &Request,
    route_match: RouteMatch<'_>,
) -> Result<Response, DispatchError> {
    let route = route_match.route;
    match (route.controller.as_str(), route.action.as_str()) {
        ("HomeController", "index") => {
            let mut data = HashMap::new();
            data.insert("title".to_string(), "AGILANG Native Framework".to_string());
            data.insert(
                "message".to_string(),
                "Your application is running successfully.".to_string(),
            );
            return render_view(project_root, "welcome", data);
        }
        ("HomeController", "about") => {
            return Ok(build_text_response(
                200,
                "text/html; charset=utf-8",
                "<h1>About AGILANG</h1><p>Native application framework.</p>".to_string(),
            ));
        }
        ("HealthController", "show") => {
            return Ok(build_json_response(
                200,
                json!({
                    "status": "healthy",
                    "framework": "AGILANG",
                    "version": "0.5.0"
                })
                .to_string(),
            ));
        }
        _ => {}
    }

    dispatch_generated_resource(project_root, req, route, &route_match.params)
}

fn dispatch_generated_resource(
    project_root: &Path,
    req: &Request,
    route: &Route,
    params: &HashMap<String, String>,
) -> Result<Response, DispatchError> {
    let Some(resource_name) = route.controller.strip_suffix("Controller") else {
        return Err(DispatchError::new(
            500,
            format!("Controller `{}` is not registered", route.controller),
        ));
    };

    let manifest = load_framework_manifest(project_root)
        .map_err(|error| DispatchError::new(500, error.to_string()))?;
    let descriptor = if let Some(manifest) = manifest.as_ref() {
        manifest
            .models
            .get(resource_name)
            .cloned()
            .ok_or_else(|| {
                DispatchError::new(500, format!("Model manifest for `{resource_name}` not found"))
            })?
    } else {
        let model_path = project_root.join("app/Models").join(format!("{resource_name}.agi"));
        if !model_path.exists() {
            return Err(DispatchError::new(
                500,
                format!("Controller `{}` is not registered", route.controller),
            ));
        }
        parse_model_descriptor(&model_path)
            .map_err(|error| DispatchError::new(500, error.to_string()))?
    };
    let request_name = match route.action.as_str() {
        "store" => Some(format!("Store{resource_name}Request")),
        "update" => Some(format!("Update{resource_name}Request")),
        _ => None,
    };
    let request_descriptor = request_name
        .as_ref()
        .map(|name| {
            if let Some(manifest) = manifest.as_ref() {
                manifest
                    .requests
                    .get(name)
                    .cloned()
                    .ok_or_else(|| anyhow!("Request manifest for `{name}` not found"))
            } else {
                let path = project_root.join("app/Requests").join(format!("{name}.agi"));
                parse_request_descriptor(&path)
            }
        })
        .transpose()
        .map_err(|error| DispatchError::new(500, error.to_string()))?;

    let auth_state = load_auth_state(project_root, req)
        .map_err(|error| DispatchError::new(500, error.to_string()))?;
    let requires_mutation_auth = matches!(route.action.as_str(), "store" | "update" | "destroy" | "restore");
    if requires_mutation_auth {
        let Some((session, user)) = auth_state.as_ref() else {
            return Err(DispatchError::new(403, "Forbidden"));
        };
        verify_csrf(req, session)?;
        if user.role != "admin" {
            return Err(DispatchError::new(403, "Forbidden"));
        }
    }

    let config = load_database_config(project_root)
        .map_err(|error| DispatchError::new(500, error.to_string()))?;
    let mut conn = FrameworkConnection::connect(&config)
        .map_err(|error| DispatchError::new(500, error.to_string()))?;

    match route.action.as_str() {
        "index" => {
            let rows = list_rows(&mut conn, &descriptor)
                .map_err(|error| DispatchError::new(500, error.to_string()))?;
            let data = rows
                .into_iter()
                .map(|row| sanitize_row(row, &descriptor))
                .collect::<Vec<_>>();
            Ok(build_json_response(
                200,
                json!({ "data": data }).to_string(),
            ))
        }
        "show" => {
            let id = parse_id_param(params)?;
            let row = find_row(&mut conn, &descriptor, id, false)
                .map_err(|error| DispatchError::new(500, error.to_string()))?;
            let Some(row) = row else {
                return Err(DispatchError::new(404, "Not Found"));
            };
            Ok(build_json_response(
                200,
                json!({ "data": sanitize_row(row, &descriptor) }).to_string(),
            ))
        }
        "store" => {
            let request_descriptor = request_descriptor
                .as_ref()
                .ok_or_else(|| DispatchError::new(500, "Missing request descriptor"))?;
            let payload = parse_request_payload(req)
                .map_err(|error| DispatchError::new(400, error.to_string()))?;
            validate_payload(&payload, request_descriptor)?;
            let insertable = filter_allowed_fields(&payload, &descriptor, request_descriptor);
            conn.begin_transaction()
                .map_err(|error| DispatchError::new(500, error.to_string()))?;
            let result = (|| -> Result<Response> {
                let response = create_row(&mut conn, &descriptor, insertable)?;
                conn.commit()?;
                Ok(response)
            })();
            match result {
                Ok(response) => Ok(response),
                Err(error) => {
                    let _ = conn.rollback();
                    Err(DispatchError::new(500, error.to_string()))
                }
            }
        }
        "update" => {
            let request_descriptor = request_descriptor
                .as_ref()
                .ok_or_else(|| DispatchError::new(500, "Missing request descriptor"))?;
            let id = parse_id_param(params)?;
            let payload = parse_request_payload(req)
                .map_err(|error| DispatchError::new(400, error.to_string()))?;
            validate_payload(&payload, request_descriptor)?;
            let updatable = filter_allowed_fields(&payload, &descriptor, request_descriptor);
            conn.begin_transaction()
                .map_err(|error| DispatchError::new(500, error.to_string()))?;
            let result = (|| -> Result<Response> {
                let existing = find_row(&mut conn, &descriptor, id, true)?;
                if existing.is_none() {
                    bail!("404");
                }
                update_row(&mut conn, &descriptor, id, updatable)?;
                let row = find_row(&mut conn, &descriptor, id, false)?
                    .ok_or_else(|| anyhow!("404"))?;
                conn.commit()?;
                Ok(build_json_response(
                    200,
                    json!({ "data": sanitize_row(row, &descriptor) }).to_string(),
                ))
            })();
            match result {
                Ok(response) => Ok(response),
                Err(error) if error.to_string() == "404" => {
                    let _ = conn.rollback();
                    Err(DispatchError::new(404, "Not Found"))
                }
                Err(error) => {
                    let _ = conn.rollback();
                    Err(DispatchError::new(500, error.to_string()))
                }
            }
        }
        "destroy" => {
            let id = parse_id_param(params)?;
            conn.begin_transaction()
                .map_err(|error| DispatchError::new(500, error.to_string()))?;
            let result = (|| -> Result<Response> {
                let existing = find_row(&mut conn, &descriptor, id, true)?;
                if existing.is_none() {
                    bail!("404");
                }
                soft_delete_row(&mut conn, &descriptor, id)?;
                conn.commit()?;
                Ok(build_json_response(200, json!({ "deleted": true }).to_string()))
            })();
            match result {
                Ok(response) => Ok(response),
                Err(error) if error.to_string() == "404" => {
                    let _ = conn.rollback();
                    Err(DispatchError::new(404, "Not Found"))
                }
                Err(error) => {
                    let _ = conn.rollback();
                    Err(DispatchError::new(500, error.to_string()))
                }
            }
        }
        "restore" => {
            let id = parse_id_param(params)?;
            conn.begin_transaction()
                .map_err(|error| DispatchError::new(500, error.to_string()))?;
            let result = (|| -> Result<Response> {
                let existing = find_row(&mut conn, &descriptor, id, true)?;
                if existing.is_none() {
                    bail!("404");
                }
                restore_row(&mut conn, &descriptor, id)?;
                let row = find_row(&mut conn, &descriptor, id, false)?
                    .ok_or_else(|| anyhow!("404"))?;
                conn.commit()?;
                Ok(build_json_response(
                    200,
                    json!({ "data": sanitize_row(row, &descriptor) }).to_string(),
                ))
            })();
            match result {
                Ok(response) => Ok(response),
                Err(error) if error.to_string() == "404" => {
                    let _ = conn.rollback();
                    Err(DispatchError::new(404, "Not Found"))
                }
                Err(error) => {
                    let _ = conn.rollback();
                    Err(DispatchError::new(500, error.to_string()))
                }
            }
        }
        _ => Err(DispatchError::new(
            500,
            format!("Action `{}` is not registered", route.action),
        )),
    }
}

fn render_view(
    project_root: &Path,
    view: &str,
    data: HashMap<String, String>,
) -> Result<Response, DispatchError> {
    let engine = ViewEngine::new(project_root.join("resources/views"));
    let html = engine
        .render(view, &data)
        .map_err(|error| DispatchError::new(500, error.to_string()))?;
    Ok(Response::html(&html))
}

fn load_auth_state(
    project_root: &Path,
    req: &Request,
) -> Result<Option<(AuthSessionRecord, AuthUserRecord)>> {
    let mut conn = crate::open_auth_db(project_root).map_err(|error| anyhow!(error))?;
    let Some(session) = load_session_by_cookie(&mut conn, req).map_err(|error| anyhow!(error))? else {
        return Ok(None);
    };
    let Some(user) = load_user_by_id(&mut conn, &session.user_id).map_err(|error| anyhow!(error))? else {
        return Ok(None);
    };
    Ok(Some((session, user)))
}

fn verify_csrf(req: &Request, session: &AuthSessionRecord) -> Result<(), DispatchError> {
    if !matches!(
        req.method,
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch | HttpMethod::Delete
    ) {
        return Ok(());
    }

    let header_token = req
        .headers
        .get("x-csrf-token")
        .or_else(|| req.headers.get("X-CSRF-Token"))
        .cloned();
    let form_token = parse_form_body(req)
        .get("_csrf")
        .and_then(|values| values.first().cloned());
    let token = header_token.or(form_token).unwrap_or_default();
    if token == session.csrf_secret {
        Ok(())
    } else {
        Err(DispatchError::new(403, "Invalid CSRF token."))
    }
}

fn parse_id_param(params: &HashMap<String, String>) -> Result<i64, DispatchError> {
    params
        .get("id")
        .ok_or_else(|| DispatchError::new(400, "Missing route parameter `id`"))?
        .parse::<i64>()
        .map_err(|_| DispatchError::new(400, "Invalid route parameter `id`"))
}

fn parse_model_descriptor(path: &Path) -> Result<ResourceDescriptor> {
    let content = fs::read_to_string(path)?;
    Ok(ResourceDescriptor {
        table: parse_string_assignment(&content, "table").unwrap_or_else(|| "records".to_string()),
        primary_key: parse_string_assignment(&content, "primary_key")
            .unwrap_or_else(|| "id".to_string()),
        fillable: parse_list_assignment(&content, "fillable"),
        hidden: parse_list_assignment(&content, "hidden"),
        casts: parse_map_assignment(&content, "casts"),
        timestamps: parse_bool_assignment(&content, "timestamps").unwrap_or(false),
        soft_deletes: parse_bool_assignment(&content, "soft_deletes").unwrap_or(false),
    })
}

fn parse_request_descriptor(path: &Path) -> Result<RequestDescriptor> {
    let content = fs::read_to_string(path)?;
    Ok(RequestDescriptor {
        rules: parse_rules_map(&content),
        validated_fields: parse_list_after(&content, "fn validated_fields() -> list:"),
    })
}

fn load_database_config(project_root: &Path) -> Result<DatabaseConfig> {
    let config_path = project_root.join("config/database.agi");
    let content = fs::read_to_string(config_path)?;
    let default_driver = parse_string_in_map(&content, "\"default\"")
        .unwrap_or_else(|| "agidb".to_string());
    let block = extract_connection_block(&content, &default_driver)
        .ok_or_else(|| anyhow!("database connection `{default_driver}` not found"))?;
    let driver_name =
        parse_string_in_map(&block, "\"driver\"").unwrap_or_else(|| default_driver.clone());
    let strict = parse_bool_in_map(&content, "\"strict_driver_selection\"").unwrap_or(true);
    match driver_name.as_str() {
        "agidb" => Ok(DatabaseConfig {
            driver: FrameworkDriver::Agidb,
            database: resolve_database_path(project_root, &parse_string_in_map(&block, "\"database\"")
                .unwrap_or_else(|| "storage/database/main.agidb".to_string())),
            host: None,
            port: None,
            username: None,
            password: None,
            strict_driver_selection: strict,
        }),
        "sqlite" => Ok(DatabaseConfig {
            driver: FrameworkDriver::Sqlite,
            database: resolve_database_path(project_root, &parse_string_in_map(&block, "\"database\"")
                .unwrap_or_else(|| "storage/database/main.sqlite".to_string())),
            host: None,
            port: None,
            username: None,
            password: None,
            strict_driver_selection: strict,
        }),
        "mysql" => Ok(DatabaseConfig {
            driver: FrameworkDriver::Mysql,
            database: resolve_env_value(
                project_root,
                parse_string_in_map(&block, "\"database\"").unwrap_or_default().as_str(),
            ),
            host: Some(resolve_env_value(
                project_root,
                parse_string_in_map(&block, "\"host\"").unwrap_or_default().as_str(),
            )),
            port: parse_number_in_map(&block, "\"port\"").map(|value| value as u16),
            username: Some(resolve_env_value(
                project_root,
                parse_string_in_map(&block, "\"username\"").unwrap_or_default().as_str(),
            )),
            password: Some(resolve_env_value(
                project_root,
                parse_string_in_map(&block, "\"password\"").unwrap_or_default().as_str(),
            )),
            strict_driver_selection: strict,
        }),
        other => bail!("unsupported database driver `{other}`"),
    }
}

fn resolve_database_path(project_root: &Path, raw: &str) -> String {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        raw.to_string()
    } else {
        project_root.join(raw).to_string_lossy().to_string()
    }
}

fn resolve_env_value(project_root: &Path, raw: &str) -> String {
    if let Some((key, fallback)) = parse_env_expression(raw) {
        load_env_file(project_root).remove(&key).unwrap_or(fallback)
    } else {
        raw.to_string()
    }
}

fn parse_env_expression(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("env(") {
        return None;
    }
    let inner = trimmed.trim_start_matches("env(").trim_end_matches(')');
    let parts = split_top_level(inner, ',');
    let key = parts.first()?.trim().trim_matches('"').trim_matches('\'').to_string();
    let fallback = parts
        .get(1)
        .map(|value| value.trim().trim_matches('"').trim_matches('\'').to_string())
        .unwrap_or_default();
    Some((key, fallback))
}

fn load_env_file(project_root: &Path) -> HashMap<String, String> {
    let path = project_root.join(".env");
    let Ok(content) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn validate_payload(
    payload: &HashMap<String, Value>,
    request_descriptor: &RequestDescriptor,
) -> Result<(), DispatchError> {
    Validator::validate(payload, &request_descriptor.rules).map_err(|errors| {
        let errors = errors
            .into_iter()
            .map(|error| {
                json!({
                    "field": error.field,
                    "message": error.message
                })
            })
            .collect::<Vec<_>>();
        DispatchError::new(422, json!({ "errors": errors }).to_string())
    })
}

fn filter_allowed_fields(
    payload: &HashMap<String, Value>,
    descriptor: &ResourceDescriptor,
    request_descriptor: &RequestDescriptor,
) -> HashMap<String, Value> {
    let mut output = HashMap::new();
    for field in &request_descriptor.validated_fields {
        if descriptor.fillable.iter().any(|fillable| fillable == field) {
            if let Some(value) = payload.get(field) {
                output.insert(field.clone(), value.clone());
            }
        }
    }
    output
}

fn create_row(
    conn: &mut FrameworkConnection,
    descriptor: &ResourceDescriptor,
    mut fields: HashMap<String, Value>,
) -> Result<Response> {
    if !fields.contains_key(&descriptor.primary_key) {
        fields.insert(
            descriptor.primary_key.clone(),
            Value::from(next_primary_key(conn, descriptor)?),
        );
    }
    apply_timestamps(descriptor, &mut fields, true);
    let mut columns = fields.keys().cloned().collect::<Vec<_>>();
    columns.sort();
    let placeholders = columns.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let params = columns
        .iter()
        .map(|key| json_to_db_value(fields.get(key).cloned().unwrap_or(Value::Null)))
        .collect::<Vec<_>>();
    let inserted_id = fields
        .get(&descriptor.primary_key)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("insert failed"))?;
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES ({})",
            descriptor.table,
            columns.join(", "),
            placeholders
        ),
        &params,
    )?;
    let inserted = conn
        .query(
            &format!(
                "SELECT * FROM {} WHERE {} = ? LIMIT 1",
                descriptor.table, descriptor.primary_key
            ),
            &[DatabaseValue::Integer(inserted_id)],
        )?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("insert failed"))?;
    Ok(build_json_response(
        201,
        json!({ "data": sanitize_row(inserted, descriptor) }).to_string(),
    ))
}

fn next_primary_key(conn: &mut FrameworkConnection, descriptor: &ResourceDescriptor) -> Result<i64> {
    let rows = conn.query(&format!("SELECT * FROM {}", descriptor.table), &[])?;
    let next = rows
        .into_iter()
        .filter_map(|row| match row.get(&descriptor.primary_key) {
            Some(DatabaseValue::Integer(value)) => Some(*value),
            Some(DatabaseValue::Text(value)) => value.parse::<i64>().ok(),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        + 1;
    Ok(next)
}

fn update_row(
    conn: &mut FrameworkConnection,
    descriptor: &ResourceDescriptor,
    id: i64,
    mut fields: HashMap<String, Value>,
) -> Result<()> {
    apply_timestamps(descriptor, &mut fields, false);
    let mut columns = fields.keys().cloned().collect::<Vec<_>>();
    columns.sort();
    let assignments = columns
        .iter()
        .map(|column| format!("{column} = ?"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut params = columns
        .iter()
        .map(|key| json_to_db_value(fields.get(key).cloned().unwrap_or(Value::Null)))
        .collect::<Vec<_>>();
    params.push(DatabaseValue::Integer(id));
    conn.execute(
        &format!(
            "UPDATE {} SET {} WHERE {} = ?",
            descriptor.table, assignments, descriptor.primary_key
        ),
        &params,
    )?;
    Ok(())
}

fn soft_delete_row(conn: &mut FrameworkConnection, descriptor: &ResourceDescriptor, id: i64) -> Result<()> {
    if descriptor.soft_deletes {
        conn.execute(
            &format!(
                "UPDATE {} SET deleted_at = ? WHERE {} = ?",
                descriptor.table, descriptor.primary_key
            ),
            &[DatabaseValue::Text(chrono::Utc::now().to_rfc3339()), DatabaseValue::Integer(id)],
        )?;
    } else {
        conn.execute(
            &format!(
                "DELETE FROM {} WHERE {} = ?",
                descriptor.table, descriptor.primary_key
            ),
            &[DatabaseValue::Integer(id)],
        )?;
    }
    Ok(())
}

fn restore_row(conn: &mut FrameworkConnection, descriptor: &ResourceDescriptor, id: i64) -> Result<()> {
    conn.execute(
        &format!(
            "UPDATE {} SET deleted_at = ? WHERE {} = ?",
            descriptor.table, descriptor.primary_key
        ),
        &[DatabaseValue::Null, DatabaseValue::Integer(id)],
    )?;
    Ok(())
}

fn find_row(
    conn: &mut FrameworkConnection,
    descriptor: &ResourceDescriptor,
    id: i64,
    include_trashed: bool,
) -> Result<Option<HashMap<String, DatabaseValue>>> {
    if conn.driver_kind() == DatabaseDriverKind::AgiDb {
        let rows = conn.query(&format!("SELECT * FROM {}", descriptor.table), &[])?;
        return Ok(rows
            .into_iter()
            .filter(|row| include_trashed || !is_soft_deleted(row))
            .find(|row| matches_primary_key(row, &descriptor.primary_key, id)));
    }
    let mut sql = format!(
        "SELECT * FROM {} WHERE {} = ?",
        descriptor.table, descriptor.primary_key
    );
    if descriptor.soft_deletes && !include_trashed {
        sql.push_str(" AND deleted_at IS NULL");
    }
    sql.push_str(" LIMIT 1");
    Ok(conn
        .query(&sql, &[DatabaseValue::Integer(id)])?
        .into_iter()
        .next())
}

fn list_rows(
    conn: &mut FrameworkConnection,
    descriptor: &ResourceDescriptor,
) -> Result<Vec<HashMap<String, DatabaseValue>>> {
    let mut rows = conn.query(&format!("SELECT * FROM {}", descriptor.table), &[])?;
    if descriptor.soft_deletes {
        rows.retain(|row| !is_soft_deleted(row));
    }
    rows.sort_by_key(|row| primary_key_value(row, &descriptor.primary_key));
    Ok(rows)
}

fn matches_primary_key(row: &HashMap<String, DatabaseValue>, key: &str, id: i64) -> bool {
    primary_key_value(row, key) == id
}

fn primary_key_value(row: &HashMap<String, DatabaseValue>, key: &str) -> i64 {
    match row.get(key) {
        Some(DatabaseValue::Integer(value)) => *value,
        Some(DatabaseValue::Text(value)) => value.parse().unwrap_or_default(),
        _ => 0,
    }
}

fn is_soft_deleted(row: &HashMap<String, DatabaseValue>) -> bool {
    match row.get("deleted_at") {
        None | Some(DatabaseValue::Null) => false,
        Some(DatabaseValue::Text(value)) => !value.is_empty(),
        _ => true,
    }
}

fn sanitize_row(row: HashMap<String, DatabaseValue>, descriptor: &ResourceDescriptor) -> Value {
    let mut map = Map::new();
    for (key, value) in row {
        if descriptor.hidden.iter().any(|hidden| hidden == &key) {
            continue;
        }
        map.insert(key.clone(), cast_value(&key, db_value_to_json(value), descriptor));
    }
    Value::Object(map)
}

fn cast_value(key: &str, value: Value, descriptor: &ResourceDescriptor) -> Value {
    match descriptor.casts.get(key).map(String::as_str) {
        Some("integer") => value
            .as_i64()
            .map(Value::from)
            .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()).map(Value::from))
            .unwrap_or(value),
        Some("boolean") => value
            .as_bool()
            .map(Value::from)
            .or_else(|| value.as_i64().map(|number| Value::from(number != 0)))
            .unwrap_or(value),
        _ => value,
    }
}

fn parse_request_payload(req: &Request) -> Result<HashMap<String, Value>> {
    let content_type = req.headers.get("content-type").cloned().unwrap_or_default();
    if content_type.starts_with("application/json") {
        let value: Value = serde_json::from_slice(&req.body)?;
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("JSON body must be an object"))?;
        return Ok(object
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect());
    }
    Ok(parse_form_body(req)
        .into_iter()
        .map(|(key, values)| {
            let value = values.into_iter().next().map(Value::from).unwrap_or(Value::Null);
            (key, value)
        })
        .collect())
}

fn apply_timestamps(
    descriptor: &ResourceDescriptor,
    fields: &mut HashMap<String, Value>,
    creating: bool,
) {
    if !descriptor.timestamps {
        return;
    }
    let now = Value::String(chrono::Utc::now().to_rfc3339());
    fields.insert("updated_at".to_string(), now.clone());
    if creating {
        fields.insert("created_at".to_string(), now);
    }
}

fn json_to_db_value(value: Value) -> DatabaseValue {
    match value {
        Value::Null => DatabaseValue::Null,
        Value::Bool(value) => DatabaseValue::Integer(if value { 1 } else { 0 }),
        Value::Number(value) => value
            .as_i64()
            .map(DatabaseValue::Integer)
            .unwrap_or_else(|| DatabaseValue::Text(value.to_string())),
        Value::String(value) => DatabaseValue::Text(value),
        other => DatabaseValue::Text(other.to_string()),
    }
}

fn db_value_to_json(value: DatabaseValue) -> Value {
    match value {
        DatabaseValue::Null => Value::Null,
        DatabaseValue::Integer(value) => Value::from(value),
        DatabaseValue::Float(value) => Value::from(value),
        DatabaseValue::Text(value) => Value::from(value),
        DatabaseValue::Decimal(value) => Value::from(value),
        DatabaseValue::DateTime(value) => Value::from(value),
        DatabaseValue::Json(value) => serde_json::from_str(&value).unwrap_or(Value::from(value)),
        DatabaseValue::Boolean(value) => Value::from(value),
        DatabaseValue::Binary(bytes) => Value::from(format!("{:x?}", bytes)),
    }
}

fn framework_error_response(path: &str, status: u16, message: &str) -> Response {
    if path.starts_with("/api/") || message.starts_with('{') {
        let body = if message.starts_with('{') {
            message.to_string()
        } else {
            json!({ "error": message }).to_string()
        };
        build_json_response(status, body)
    } else if status == 302 {
        redirect_response("/login")
    } else {
        build_text_response(
            status,
            "text/html; charset=utf-8",
            format!("<h1>{status}</h1><p>{}</p>", message),
        )
    }
}

fn static_mime(ext: &str) -> &'static str {
    match ext {
        "css" => "text/css",
        "js" => "application/javascript",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "html" => "text/html",
        _ => "application/octet-stream",
    }
}

fn parse_string_assignment(content: &str, name: &str) -> Option<String> {
    let marker = format!("{name} =");
    let index = content.find(&marker)?;
    let remainder = &content[index + marker.len()..];
    let start = remainder.find('"').or_else(|| remainder.find('\''))?;
    let rest = &remainder[start + 1..];
    let end = rest.find('"').or_else(|| rest.find('\''))?;
    Some(rest[..end].to_string())
}

fn parse_bool_assignment(content: &str, name: &str) -> Option<bool> {
    let marker = format!("{name} =");
    let index = content.find(&marker)?;
    let remainder = content[index + marker.len()..].trim_start();
    if remainder.starts_with("true") {
        Some(true)
    } else if remainder.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn parse_list_assignment(content: &str, name: &str) -> Vec<String> {
    let marker = format!("{name} =");
    let Some(index) = content.find(&marker) else {
        return Vec::new();
    };
    parse_bracket_list(&content[index + marker.len()..])
}

fn parse_map_assignment(content: &str, name: &str) -> HashMap<String, String> {
    let marker = format!("{name} =");
    let Some(index) = content.find(&marker) else {
        return HashMap::new();
    };
    let remainder = &content[index + marker.len()..];
    let Some(start) = remainder.find('{') else {
        return HashMap::new();
    };
    let Some(end) = remainder[start..].find('}') else {
        return HashMap::new();
    };
    split_top_level(&remainder[start + 1..start + end], ',')
        .into_iter()
        .filter_map(|entry| {
            let (key, value) = entry.split_once(':')?;
            Some((
                key.trim().trim_matches('"').trim_matches('\'').to_string(),
                value.trim().trim_matches('"').trim_matches('\'').to_string(),
            ))
        })
        .collect()
}

fn parse_rules_map(content: &str) -> HashMap<String, Vec<String>> {
    let marker = "fn rules() -> map:";
    let Some(index) = content.find(marker) else {
        return HashMap::new();
    };
    let remainder = &content[index + marker.len()..];
    let Some(start) = remainder.find('{') else {
        return HashMap::new();
    };
    let Some(end) = remainder[start..].find('}') else {
        return HashMap::new();
    };
    split_top_level(&remainder[start + 1..start + end], ',')
        .into_iter()
        .filter_map(|entry| {
            let (key, value) = entry.split_once(':')?;
            Some((
                key.trim().trim_matches('"').trim_matches('\'').to_string(),
                parse_inline_list(value),
            ))
        })
        .collect()
}

fn parse_list_after(content: &str, marker: &str) -> Vec<String> {
    let Some(index) = content.find(marker) else {
        return Vec::new();
    };
    parse_bracket_list(&content[index + marker.len()..])
}

fn parse_bracket_list(content: &str) -> Vec<String> {
    let Some(start) = content.find('[') else {
        return Vec::new();
    };
    let Some(end) = content[start..].find(']') else {
        return Vec::new();
    };
    parse_inline_list(&content[start..start + end + 1])
}

fn parse_inline_list(content: &str) -> Vec<String> {
    let trimmed = content.trim().trim_start_matches('[').trim_end_matches(']');
    split_top_level(trimmed, ',')
        .into_iter()
        .map(|value| value.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn parse_string_in_map(content: &str, key: &str) -> Option<String> {
    let marker = format!("{key}:");
    let index = content.find(&marker)?;
    let remainder = &content[index + marker.len()..];
    if let Some(start) = remainder.find('"').or_else(|| remainder.find('\'')) {
        let rest = &remainder[start + 1..];
        let end = rest.find('"').or_else(|| rest.find('\''))?;
        Some(rest[..end].to_string())
    } else {
        let end = remainder.find([',', '\n', '}']).unwrap_or(remainder.len());
        Some(remainder[..end].trim().to_string())
    }
}

fn parse_bool_in_map(content: &str, key: &str) -> Option<bool> {
    let marker = format!("{key}:");
    let index = content.find(&marker)?;
    let remainder = content[index + marker.len()..].trim_start();
    if remainder.starts_with("true") {
        Some(true)
    } else if remainder.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn parse_number_in_map(content: &str, key: &str) -> Option<i64> {
    let marker = format!("{key}:");
    let index = content.find(&marker)?;
    let remainder = content[index + marker.len()..].trim_start();
    let end = remainder.find([',', '\n', '}']).unwrap_or(remainder.len());
    remainder[..end].trim().parse().ok()
}

fn extract_connection_block(content: &str, connection: &str) -> Option<String> {
    let marker = format!("\"{connection}\":");
    let index = content.find(&marker)?;
    let remainder = &content[index + marker.len()..];
    let start = remainder.find('{')?;
    let mut depth = 0i32;
    let mut end_index = None;
    for (offset, ch) in remainder[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_index = Some(start + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    end_index.map(|end| remainder[start..=end].to_string())
}

fn split_top_level(content: &str, delimiter: char) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut curly = 0i32;
    let mut square = 0i32;
    let mut paren = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    for ch in content.chars() {
        match ch {
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '{' if !in_single && !in_double => curly += 1,
            '}' if !in_single && !in_double => curly -= 1,
            '[' if !in_single && !in_double => square += 1,
            ']' if !in_single && !in_double => square -= 1,
            '(' if !in_single && !in_double => paren += 1,
            ')' if !in_single && !in_double => paren -= 1,
            _ => {}
        }
        if ch == delimiter && !in_single && !in_double && curly == 0 && square == 0 && paren == 0 {
            result.push(current.trim().to_string());
            current.clear();
            continue;
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}
