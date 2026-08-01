use agilang_database_agidb::AgiDbConnection;
use agilang_database_driver::{DatabaseConnection, DatabaseValue};
use anyhow::Result;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

pub type DatabaseRow = HashMap<String, Value>;
pub type TableRows = Vec<DatabaseRow>;

static DATABASE_STORE: Lazy<Mutex<HashMap<String, TableRows>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
thread_local! {
    static DATABASE_PATH_OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone)]
pub struct WhereCondition {
    pub column: String,
    pub operator: String,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct QueryBuilder {
    pub table_name: String,
    pub conditions: Vec<WhereCondition>,
    pub order_by_column: Option<(String, String)>,
    pub limit_count: Option<usize>,
    pub offset_count: Option<usize>,
}

impl QueryBuilder {
    pub fn table(table: impl Into<String>) -> Self {
        Self {
            table_name: table.into(),
            conditions: Vec::new(),
            order_by_column: None,
            limit_count: None,
            offset_count: None,
        }
    }

    pub fn where_clause(mut self, column: &str, operator: &str, value: Value) -> Self {
        self.conditions.push(WhereCondition {
            column: column.to_string(),
            operator: operator.to_string(),
            value,
        });
        self
    }

    pub fn order_by(mut self, column: &str, direction: &str) -> Self {
        self.order_by_column = Some((column.to_string(), direction.to_uppercase()));
        self
    }

    pub fn limit(mut self, count: usize) -> Self {
        self.limit_count = Some(count);
        self
    }

    pub fn to_sql(&self) -> (String, Vec<Value>) {
        let mut sql = format!("SELECT * FROM {}", self.table_name);
        let mut params = Vec::new();

        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            let clauses: Vec<String> = self
                .conditions
                .iter()
                .map(|c| {
                    params.push(c.value.clone());
                    format!("{} {} ?", c.column, c.operator)
                })
                .collect();
            sql.push_str(&clauses.join(" AND "));
        }

        if let Some((col, dir)) = &self.order_by_column {
            sql.push_str(&format!(" ORDER BY {} {}", col, dir));
        }

        if let Some(lim) = self.limit_count {
            sql.push_str(&format!(" LIMIT {}", lim));
        }

        (sql, params)
    }

    pub fn get(&self) -> Vec<HashMap<String, Value>> {
        if let Ok(rows) = self.get_from_agidb() {
            return rows;
        }

        let guard = DATABASE_STORE.lock().unwrap();
        let empty = Vec::new();
        let rows = guard.get(&self.table_name).unwrap_or(&empty);

        let mut filtered: Vec<HashMap<String, Value>> = rows
            .iter()
            .filter(|row| {
                for cond in &self.conditions {
                    if let Some(val) = row.get(&cond.column) {
                        if cond.operator == "=" && val != &cond.value {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        if let Some(lim) = self.limit_count {
            filtered.truncate(lim);
        }

        filtered
    }

    pub fn first(&self) -> Option<HashMap<String, Value>> {
        self.get().into_iter().next()
    }

    pub fn insert(
        table: &str,
        mut record: HashMap<String, Value>,
    ) -> Result<HashMap<String, Value>> {
        ensure_agidb_table(table, record.keys().map(String::as_str).collect())?;
        let mut columns = record.keys().cloned().collect::<Vec<_>>();
        columns.sort();
        let values = columns
            .iter()
            .map(|column| json_to_database_value(record.get(column).unwrap()))
            .collect::<Vec<_>>();
        let placeholders = vec!["?"; columns.len()].join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders
        );
        let mut conn = default_agidb_connection();
        let result = conn.execute(&sql, &values)?;
        if let Some(id) = result.last_insert_id {
            record.entry("id".to_string()).or_insert(Value::from(id));
        }

        let mut guard = DATABASE_STORE.lock().unwrap();
        let table_rows = guard.entry(table.to_string()).or_default();

        if !record.contains_key("id") {
            let next_id = (table_rows.len() + 1) as i64;
            record.insert("id".to_string(), serde_json::json!(next_id));
        }

        table_rows.push(record.clone());
        Ok(record)
    }

    pub fn clear_table(table: &str) {
        let mut conn = default_agidb_connection();
        let _ = conn.execute(&format!("DELETE FROM {}", table), &[]);

        let mut guard = DATABASE_STORE.lock().unwrap();
        guard.remove(table);
    }
    pub fn paginate(&self, per_page: usize) -> PaginatedResult {
        let all = self.get();
        let total = all.len();
        let last_page = if total == 0 {
            1
        } else {
            total.div_ceil(per_page)
        };
        let data = all.into_iter().take(per_page).collect();

        PaginatedResult {
            data,
            current_page: 1,
            per_page,
            total,
            last_page,
        }
    }

    fn get_from_agidb(&self) -> Result<Vec<HashMap<String, Value>>> {
        let mut conn = default_agidb_connection();
        let (sql, params) = self.to_sql();
        let params = params
            .iter()
            .map(json_to_database_value)
            .collect::<Vec<_>>();
        let rows = conn.query(&sql, &params)?;
        Ok(rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|(key, value)| (key, database_value_to_json(value)))
                    .collect()
            })
            .collect())
    }
}

pub fn set_thread_local_database_path(path: impl Into<String>) {
    DATABASE_PATH_OVERRIDE.with(|slot| {
        *slot.borrow_mut() = Some(path.into());
    });
}

pub fn clear_thread_local_database_path() {
    DATABASE_PATH_OVERRIDE.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

fn default_agidb_connection() -> AgiDbConnection {
    if let Some(path) = DATABASE_PATH_OVERRIDE.with(|slot| slot.borrow().clone()) {
        return AgiDbConnection::new(path);
    }

    #[cfg(test)]
    {
        let thread_id = format!("{:?}", std::thread::current().id())
            .replace("ThreadId(", "")
            .replace(')', "");
        let path = std::env::temp_dir()
            .join(format!(
                "agilang-framework-database-{}-{}.agidb",
                std::process::id(),
                thread_id
            ))
            .to_string_lossy()
            .into_owned();
        AgiDbConnection::new(path)
    }

    #[cfg(not(test))]
    {
        let path = std::env::var("AGIDB_DATABASE")
            .or_else(|_| std::env::var("DATABASE_PATH"))
            .unwrap_or_else(|_| "storage/database/main.agidb".to_string());
        AgiDbConnection::new(path)
    }
}

fn ensure_agidb_table(table: &str, columns: Vec<&str>) -> Result<()> {
    let mut all_columns = vec!["id".to_string()];
    for column in columns {
        if !all_columns.iter().any(|existing| existing == column) {
            all_columns.push(column.to_string());
        }
    }
    let definitions = all_columns
        .into_iter()
        .map(|column| {
            if column == "id" {
                "id INTEGER".to_string()
            } else {
                format!("{} TEXT", column)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut conn = default_agidb_connection();
    conn.execute(&format!("CREATE TABLE {} ({})", table, definitions), &[])?;
    Ok(())
}

fn json_to_database_value(value: &Value) -> DatabaseValue {
    match value {
        Value::Null => DatabaseValue::Null,
        Value::Bool(v) => DatabaseValue::Boolean(*v),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                DatabaseValue::Integer(i)
            } else if let Some(f) = v.as_f64() {
                DatabaseValue::Float(f)
            } else {
                DatabaseValue::Text(v.to_string())
            }
        }
        Value::String(v) => DatabaseValue::Text(v.clone()),
        Value::Array(_) | Value::Object(_) => DatabaseValue::Json(value.to_string()),
    }
}

fn database_value_to_json(value: DatabaseValue) -> Value {
    match value {
        DatabaseValue::Null => Value::Null,
        DatabaseValue::Boolean(v) => Value::Bool(v),
        DatabaseValue::Integer(v) => Value::from(v),
        DatabaseValue::Float(v) => Value::from(v),
        DatabaseValue::Decimal(v) | DatabaseValue::Text(v) | DatabaseValue::DateTime(v) => {
            Value::String(v)
        }
        DatabaseValue::Binary(v) => Value::Array(v.into_iter().map(Value::from).collect()),
        DatabaseValue::Json(v) => serde_json::from_str(&v).unwrap_or(Value::String(v)),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PaginatedResult {
    pub data: Vec<DatabaseRow>,
    pub current_page: usize,
    pub per_page: usize,
    pub total: usize,
    pub last_page: usize,
}

pub fn validate_mass_assignment(
    input: &HashMap<String, Value>,
    fillable: &[&str],
    guarded: &[&str],
) -> anyhow::Result<HashMap<String, Value>> {
    let mut safe = HashMap::new();
    for (k, v) in input {
        if guarded.contains(&k.as_str()) {
            anyhow::bail!("E6210 Unsafe mass assignment: property `{}` is guarded", k);
        }
        if !fillable.is_empty() && !fillable.contains(&k.as_str()) {
            anyhow::bail!(
                "E6210 Unsafe mass assignment: property `{}` is not fillable",
                k
            );
        }
        safe.insert(k.clone(), v.clone());
    }
    Ok(safe)
}

pub trait Model: Sized {
    fn table_name() -> &'static str;
    fn fillable() -> &'static [&'static str] {
        &[]
    }
    fn guarded() -> &'static [&'static str] {
        &["password_hash", "role", "is_admin"]
    }
    fn from_row(row: HashMap<String, Value>) -> Result<Self>;
    fn to_row(&self) -> HashMap<String, Value>;
}

pub struct DB;

impl DB {
    pub fn table(table: &str) -> QueryBuilder {
        QueryBuilder::table(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_query_builder_sql_compilation() {
        let query = QueryBuilder::table("users")
            .where_clause("status", "=", json!("active"))
            .order_by("created_at", "DESC")
            .limit(10);

        let (sql, params) = query.to_sql();
        assert_eq!(
            sql,
            "SELECT * FROM users WHERE status = ? ORDER BY created_at DESC LIMIT 10"
        );
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], json!("active"));
    }

    #[test]
    fn test_in_memory_insert_and_query() {
        QueryBuilder::clear_table("users");

        let mut record = HashMap::new();
        record.insert("name".into(), json!("Alice"));
        record.insert("email".into(), json!("alice@example.com"));
        record.insert("status".into(), json!("active"));

        let inserted = QueryBuilder::insert("users", record).unwrap();
        assert_eq!(inserted.get("id").unwrap(), &json!(1));

        let results = QueryBuilder::table("users")
            .where_clause("email", "=", json!("alice@example.com"))
            .get();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("name").unwrap(), &json!("Alice"));
    }

    #[test]
    fn test_mass_assignment_protection() {
        let mut input = HashMap::new();
        input.insert("name".to_string(), json!("Bob"));
        input.insert("role".to_string(), json!("admin"));

        let fillable = vec!["name"];
        let guarded = vec!["role"];

        let res = validate_mass_assignment(&input, &fillable, &guarded);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("E6210"));
    }
}
