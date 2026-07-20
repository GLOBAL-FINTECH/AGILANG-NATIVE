use anyhow::Result;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

pub type DatabaseRow = HashMap<String, Value>;
pub type TableRows = Vec<DatabaseRow>;

static DATABASE_STORE: Lazy<Mutex<HashMap<String, TableRows>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

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
        let mut guard = DATABASE_STORE.lock().unwrap();
        guard.remove(table);
    }
}

pub trait Model: Sized {
    fn table_name() -> &'static str;
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
}
