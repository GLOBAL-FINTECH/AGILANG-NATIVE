use crate::{validate_mass_assignment, DatabaseConfig, FrameworkConnection};
use agilang_database_driver::{DatabaseConnection, DatabaseValue};
use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeCast {
    Integer,
    Float,
    Boolean,
    Json,
    String,
}

#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: usize,
    pub per_page: usize,
    pub total: usize,
    pub last_page: usize,
}

pub trait OrmModel: Sized {
    fn table_name() -> &'static str;

    fn primary_key() -> &'static str {
        "id"
    }

    fn fillable() -> &'static [&'static str] {
        &[]
    }

    fn guarded() -> &'static [&'static str] {
        &["password_hash", "role", "is_admin"]
    }

    fn hidden() -> &'static [&'static str] {
        &[]
    }

    fn casts() -> &'static [(&'static str, AttributeCast)] {
        &[]
    }

    fn uses_timestamps() -> bool {
        true
    }

    fn uses_soft_deletes() -> bool {
        false
    }

    fn from_attributes(attributes: HashMap<String, Value>) -> Result<Self>;
    fn attributes(&self) -> HashMap<String, Value>;
    fn set_attributes(&mut self, attributes: HashMap<String, Value>);

    fn to_public_json(&self) -> Value {
        let mut object = Map::new();
        for (key, value) in self.attributes() {
            if !Self::hidden().contains(&key.as_str()) {
                object.insert(key, value);
            }
        }
        Value::Object(object)
    }
}

pub struct ModelRepository<M: OrmModel> {
    conn: FrameworkConnection,
    _marker: PhantomData<M>,
}

impl<M: OrmModel> ModelRepository<M> {
    pub fn connect(config: &DatabaseConfig) -> Result<Self> {
        Ok(Self {
            conn: FrameworkConnection::connect(config)?,
            _marker: PhantomData,
        })
    }

    pub fn from_connection(conn: FrameworkConnection) -> Self {
        Self {
            conn,
            _marker: PhantomData,
        }
    }

    pub fn health_check(&mut self) -> Result<bool> {
        Ok(self.conn.health_check()?.healthy)
    }

    pub fn connection_mut(&mut self) -> &mut FrameworkConnection {
        &mut self.conn
    }

    pub fn create(&mut self, attributes: HashMap<String, Value>) -> Result<M> {
        let mut safe = validate_mass_assignment(&attributes, M::fillable(), M::guarded())?;
        apply_insert_defaults::<M>(&mut safe);
        let inserted = self.insert_row(M::table_name(), safe)?;
        M::from_attributes(apply_casts::<M>(inserted))
    }

    pub fn find(&mut self, id: i64) -> Result<Option<M>> {
        let rows = self.conn.query(
            &format!(
                "SELECT * FROM {} WHERE {} = ? LIMIT 1",
                M::table_name(),
                M::primary_key()
            ),
            &[DatabaseValue::Integer(id)],
        )?;

        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let attributes = apply_casts::<M>(db_row_to_json(row));
        if M::uses_soft_deletes() && is_soft_deleted(&attributes) {
            return Ok(None);
        }
        Ok(Some(M::from_attributes(attributes)?))
    }

    pub fn all(&mut self) -> Result<Vec<M>> {
        let rows = self
            .conn
            .query(&format!("SELECT * FROM {}", M::table_name()), &[])?;
        rows.into_iter()
            .map(|row| db_row_to_json(row))
            .filter(|row| !M::uses_soft_deletes() || !is_soft_deleted(row))
            .map(|row| M::from_attributes(apply_casts::<M>(row)))
            .collect()
    }

    pub fn where_eq(&mut self, column: &str, value: Value) -> Result<Vec<M>> {
        let rows = self.conn.query(
            &format!("SELECT * FROM {} WHERE {} = ?", M::table_name(), column),
            &[json_to_database_value(&value)],
        )?;
        rows.into_iter()
            .map(|row| db_row_to_json(row))
            .filter(|row| !M::uses_soft_deletes() || !is_soft_deleted(row))
            .map(|row| M::from_attributes(apply_casts::<M>(row)))
            .collect()
    }

    pub fn paginate(&mut self, page: usize, per_page: usize) -> Result<Page<M>> {
        let page = page.max(1);
        let per_page = per_page.max(1);
        let all = self.all()?;
        let total = all.len();
        let last_page = if total == 0 { 1 } else { total.div_ceil(per_page) };
        let start = ((page - 1) * per_page).min(total);
        let end = (start + per_page).min(total);
        Ok(Page {
            items: all.into_iter().skip(start).take(end - start).collect(),
            page,
            per_page,
            total,
            last_page,
        })
    }

    pub fn update(&mut self, id: i64, attributes: HashMap<String, Value>) -> Result<M> {
        let safe = validate_mass_assignment(&attributes, M::fillable(), M::guarded())?;
        let mut existing = self
            .find(id)?
            .ok_or_else(|| anyhow!("E6211 model `{}` not found", M::table_name()))?
            .attributes();
        for (key, value) in safe {
            existing.insert(key, value);
        }
        apply_update_defaults::<M>(&mut existing);
        self.update_row(M::table_name(), M::primary_key(), id, &existing)?;
        self.find(id)?
            .ok_or_else(|| anyhow!("E6212 model `{}` disappeared after update", M::table_name()))
    }

    pub fn save(&mut self, model: &mut M) -> Result<()> {
        let mut attributes = model.attributes();
        let id = extract_i64(attributes.get(M::primary_key()))
            .ok_or_else(|| anyhow!("E6213 cannot save `{}` without primary key", M::table_name()))?;
        apply_update_defaults::<M>(&mut attributes);
        self.update_row(M::table_name(), M::primary_key(), id, &attributes)?;
        let refreshed = self
            .find(id)?
            .ok_or_else(|| anyhow!("E6214 failed to refresh `{}` after save", M::table_name()))?;
        model.set_attributes(refreshed.attributes());
        Ok(())
    }

    pub fn delete(&mut self, id: i64) -> Result<()> {
        if M::uses_soft_deletes() {
            return self.soft_delete(id);
        }
        self.conn.execute(
            &format!(
                "DELETE FROM {} WHERE {} = ?",
                M::table_name(),
                M::primary_key()
            ),
            &[DatabaseValue::Integer(id)],
        )?;
        Ok(())
    }

    pub fn soft_delete(&mut self, id: i64) -> Result<()> {
        if !M::uses_soft_deletes() {
            bail!("E6215 model `{}` does not support soft deletes", M::table_name());
        }
        self.conn.execute(
            &format!(
                "UPDATE {} SET deleted_at = ? WHERE {} = ?",
                M::table_name(),
                M::primary_key()
            ),
            &[
                DatabaseValue::Text(now_timestamp()),
                DatabaseValue::Integer(id),
            ],
        )?;
        Ok(())
    }

    pub fn restore(&mut self, id: i64) -> Result<()> {
        if !M::uses_soft_deletes() {
            bail!("E6216 model `{}` does not support restore", M::table_name());
        }
        self.conn.execute(
            &format!(
                "UPDATE {} SET deleted_at = ? WHERE {} = ?",
                M::table_name(),
                M::primary_key()
            ),
            &[DatabaseValue::Null, DatabaseValue::Integer(id)],
        )?;
        Ok(())
    }

    pub fn find_including_trashed(&mut self, id: i64) -> Result<Option<M>> {
        let rows = self.conn.query(
            &format!(
                "SELECT * FROM {} WHERE {} = ? LIMIT 1",
                M::table_name(),
                M::primary_key()
            ),
            &[DatabaseValue::Integer(id)],
        )?;
        rows.into_iter()
            .next()
            .map(|row| M::from_attributes(apply_casts::<M>(db_row_to_json(row))))
            .transpose()
    }

    fn insert_row(&mut self, table: &str, attributes: HashMap<String, Value>) -> Result<HashMap<String, Value>> {
        let mut columns = attributes.keys().cloned().collect::<Vec<_>>();
        columns.sort();
        let placeholders = vec!["?"; columns.len()].join(", ");
        let values = columns
            .iter()
            .map(|column| {
                attributes
                    .get(column)
                    .map(json_to_database_value)
                    .ok_or_else(|| anyhow!("missing insert value for `{column}`"))
            })
            .collect::<Result<Vec<_>>>()?;
        let result = self.conn.execute(
            &format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table,
                columns.join(", "),
                placeholders
            ),
            &values,
        )?;
        let mut inserted = attributes;
        if !inserted.contains_key(M::primary_key()) {
            if let Some(id) = result.last_insert_id {
                inserted.insert(M::primary_key().to_string(), Value::from(id));
            }
        }
        Ok(inserted)
    }

    fn update_row(
        &mut self,
        table: &str,
        primary_key: &str,
        id: i64,
        attributes: &HashMap<String, Value>,
    ) -> Result<()> {
        let mut columns = attributes
            .keys()
            .filter(|column| column.as_str() != primary_key)
            .cloned()
            .collect::<Vec<_>>();
        columns.sort();
        let assignments = columns
            .iter()
            .map(|column| format!("{column} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut values = columns
            .iter()
            .map(|column| {
                attributes
                    .get(column)
                    .map(json_to_database_value)
                    .ok_or_else(|| anyhow!("missing update value for `{column}`"))
            })
            .collect::<Result<Vec<_>>>()?;
        values.push(DatabaseValue::Integer(id));
        self.conn.execute(
            &format!("UPDATE {} SET {} WHERE {} = ?", table, assignments, primary_key),
            &values,
        )?;
        Ok(())
    }
}

fn apply_insert_defaults<M: OrmModel>(attributes: &mut HashMap<String, Value>) {
    if M::uses_timestamps() {
        let now = Value::String(now_timestamp());
        attributes
            .entry("created_at".to_string())
            .or_insert_with(|| now.clone());
        attributes
            .entry("updated_at".to_string())
            .or_insert_with(|| now.clone());
    }
    if M::uses_soft_deletes() {
        attributes
            .entry("deleted_at".to_string())
            .or_insert(Value::Null);
    }
}

fn apply_update_defaults<M: OrmModel>(attributes: &mut HashMap<String, Value>) {
    if M::uses_timestamps() {
        attributes.insert("updated_at".to_string(), Value::String(now_timestamp()));
    }
}

fn apply_casts<M: OrmModel>(mut attributes: HashMap<String, Value>) -> HashMap<String, Value> {
    for (column, cast) in M::casts() {
        if let Some(value) = attributes.get_mut(*column) {
            *value = apply_cast(*cast, value.take());
        }
    }
    attributes
}

fn apply_cast(cast: AttributeCast, value: Value) -> Value {
    match cast {
        AttributeCast::Integer => match value {
            Value::String(inner) => inner
                .parse::<i64>()
                .map(Value::from)
                .unwrap_or(Value::Null),
            other => other,
        },
        AttributeCast::Float => match value {
            Value::String(inner) => inner
                .parse::<f64>()
                .map(Value::from)
                .unwrap_or(Value::Null),
            other => other,
        },
        AttributeCast::Boolean => match value {
            Value::String(inner) => Value::Bool(matches!(
                inner.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )),
            Value::Number(number) => Value::Bool(number.as_i64().unwrap_or_default() != 0),
            other => other,
        },
        AttributeCast::Json => match value {
            Value::String(inner) => serde_json::from_str(&inner).unwrap_or(Value::String(inner)),
            other => other,
        },
        AttributeCast::String => match value {
            Value::Null => Value::Null,
            Value::String(inner) => Value::String(inner),
            other => Value::String(other.to_string()),
        },
    }
}

fn db_row_to_json(row: HashMap<String, DatabaseValue>) -> HashMap<String, Value> {
    row.into_iter()
        .map(|(key, value)| {
            let json = match value {
                DatabaseValue::Null => Value::Null,
                DatabaseValue::Boolean(v) => Value::Bool(v),
                DatabaseValue::Integer(v) => Value::from(v),
                DatabaseValue::Float(v) => Value::from(v),
                DatabaseValue::Decimal(v)
                | DatabaseValue::Text(v)
                | DatabaseValue::DateTime(v) => Value::String(v),
                DatabaseValue::Binary(v) => {
                    Value::Array(v.into_iter().map(Value::from).collect::<Vec<_>>())
                }
                DatabaseValue::Json(v) => {
                    serde_json::from_str(&v).unwrap_or(Value::String(v))
                }
            };
            (key, json)
        })
        .collect()
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

fn extract_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(value)) => value.parse().ok(),
        _ => None,
    }
}

fn is_soft_deleted(attributes: &HashMap<String, Value>) -> bool {
    attributes
        .get("deleted_at")
        .is_some_and(|value| !value.is_null() && value != "")
}

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Clone)]
    struct User {
        attributes: HashMap<String, Value>,
    }

    impl OrmModel for User {
        fn table_name() -> &'static str {
            "users"
        }

        fn fillable() -> &'static [&'static str] {
            &["name", "email", "is_active", "profile"]
        }

        fn guarded() -> &'static [&'static str] {
            &["role", "password_hash"]
        }

        fn hidden() -> &'static [&'static str] {
            &["password_hash"]
        }

        fn casts() -> &'static [(&'static str, AttributeCast)] {
            &[
                ("id", AttributeCast::Integer),
                ("is_active", AttributeCast::Boolean),
                ("profile", AttributeCast::Json),
            ]
        }

        fn uses_soft_deletes() -> bool {
            true
        }

        fn from_attributes(attributes: HashMap<String, Value>) -> Result<Self> {
            Ok(Self { attributes })
        }

        fn attributes(&self) -> HashMap<String, Value> {
            self.attributes.clone()
        }

        fn set_attributes(&mut self, attributes: HashMap<String, Value>) {
            self.attributes = attributes;
        }
    }

    fn unique_suffix(name: &str) -> String {
        format!(
            "{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    fn agidb_config(name: &str) -> DatabaseConfig {
        DatabaseConfig::agidb(
            std::env::temp_dir()
                .join(format!("agilang-orm-{}.agidb", unique_suffix(name)))
                .to_string_lossy()
                .to_string(),
        )
    }

    fn sqlite_config(name: &str) -> DatabaseConfig {
        DatabaseConfig::sqlite(
            std::env::temp_dir()
                .join(format!("agilang-orm-{}.sqlite", unique_suffix(name)))
                .to_string_lossy()
                .to_string(),
        )
    }

    fn mysql_config() -> Option<DatabaseConfig> {
        let config = DatabaseConfig::mysql(
            std::env::var("AGILANG_TEST_MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            std::env::var("AGILANG_TEST_MYSQL_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3306),
            std::env::var("AGILANG_TEST_MYSQL_DATABASE")
                .unwrap_or_else(|_| "agilang_tranche42_test".to_string()),
            std::env::var("AGILANG_TEST_MYSQL_USER").unwrap_or_else(|_| "root".to_string()),
            std::env::var("AGILANG_TEST_MYSQL_PASSWORD").unwrap_or_default(),
        );
        let mut conn = FrameworkConnection::connect(&config).ok()?;
        match conn.health_check() {
            Ok(health) if health.healthy => Some(config),
            _ => None,
        }
    }

    fn prepare_schema(config: &DatabaseConfig) -> Result<()> {
        let mut conn = FrameworkConnection::connect(config)?;
        let _ = conn.execute("DROP TABLE IF EXISTS users", &[]);
        let create_sql = match config.driver {
            crate::FrameworkDriver::Agidb => {
                "CREATE TABLE users (id INTEGER, name TEXT, email TEXT, is_active INTEGER, profile TEXT, created_at TEXT, updated_at TEXT, deleted_at TEXT)"
            }
            crate::FrameworkDriver::Sqlite => {
                "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, email TEXT, is_active INTEGER, profile TEXT, created_at TEXT, updated_at TEXT, deleted_at TEXT)"
            }
            crate::FrameworkDriver::Mysql => {
                "CREATE TABLE users (id BIGINT AUTO_INCREMENT PRIMARY KEY, name TEXT, email TEXT, is_active INTEGER, profile TEXT, created_at TEXT, updated_at TEXT, deleted_at TEXT)"
            }
        };
        conn.execute(create_sql, &[])
        .with_context(|| format!("failed to create users table for {:?}", config.driver))?;
        Ok(())
    }

    fn orm_conformance(config: DatabaseConfig) -> Result<()> {
        prepare_schema(&config)?;
        let mut repo = ModelRepository::<User>::connect(&config)?;
        assert!(repo.health_check()?);

        let mut attrs = HashMap::new();
        attrs.insert("name".to_string(), Value::String("Alice".to_string()));
        attrs.insert(
            "email".to_string(),
            Value::String("alice@example.com".to_string()),
        );
        attrs.insert("is_active".to_string(), Value::Bool(true));
        attrs.insert(
            "profile".to_string(),
            serde_json::json!({"plan":"pro","tags":["orm"]}),
        );

        let created = repo.create(attrs)?;
        let id = extract_i64(created.attributes().get("id")).context("created id missing")?;
        assert_eq!(created.to_public_json().get("password_hash"), None);
        assert!(created.attributes().contains_key("created_at"));
        assert!(created.attributes().contains_key("updated_at"));

        let found = repo.find(id)?.context("created user not found")?;
        assert_eq!(found.attributes().get("name"), Some(&Value::String("Alice".to_string())));
        assert_eq!(found.attributes().get("is_active"), Some(&Value::Bool(true)));
        assert!(found
            .attributes()
            .get("profile")
            .is_some_and(|value| value.is_object()));

        let by_email = repo.where_eq("email", Value::String("alice@example.com".to_string()))?;
        assert_eq!(by_email.len(), 1);

        let mut update = HashMap::new();
        update.insert("name".to_string(), Value::String("Alice Updated".to_string()));
        let updated = repo.update(id, update)?;
        assert_eq!(
            updated.attributes().get("name"),
            Some(&Value::String("Alice Updated".to_string()))
        );

        let page = repo.paginate(1, 10)?;
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);

        repo.soft_delete(id)?;
        assert!(repo.find(id)?.is_none());
        assert!(repo.find_including_trashed(id)?.is_some());

        repo.restore(id)?;
        assert!(repo.find(id)?.is_some());

        repo.delete(id)?;
        assert!(repo.find(id)?.is_none());

        let mut forbidden = HashMap::new();
        forbidden.insert("role".to_string(), Value::String("admin".to_string()));
        let err = repo.create(forbidden).unwrap_err();
        assert!(err.to_string().contains("E6210"));

        Ok(())
    }

    #[test]
    fn agidb_orm_conformance() {
        orm_conformance(agidb_config("agidb")).unwrap();
    }

    #[test]
    fn sqlite_orm_conformance() {
        orm_conformance(sqlite_config("sqlite")).unwrap();
    }

    #[test]
    fn mysql_orm_conformance() {
        let Some(config) = mysql_config() else {
            eprintln!("skipping mysql orm conformance: real mysql server unavailable");
            return;
        };
        orm_conformance(config).unwrap();
    }
}
