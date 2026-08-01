use agilang_framework_database::{DatabaseRow, QueryBuilder};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;

pub trait Seeder {
    fn run(&self) -> Result<()>;
}

pub struct SeederExecutor;

impl SeederExecutor {
    pub fn first_or_create(
        table: &str,
        attributes: HashMap<String, Value>,
        values: HashMap<String, Value>,
    ) -> Result<DatabaseRow> {
        let mut query = QueryBuilder::table(table);
        for (col, val) in &attributes {
            query = query.where_clause(col, "=", val.clone());
        }

        if let Some(existing) = query.first() {
            Ok(existing)
        } else {
            let mut merged = attributes;
            for (k, v) in values {
                merged.insert(k, v);
            }
            QueryBuilder::insert(table, merged)
        }
    }

    pub fn assign_first_user_role_transactional(
        name: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<DatabaseRow> {
        let existing_users = QueryBuilder::table("users").get();
        let role = if existing_users.is_empty() {
            "admin"
        } else {
            "user"
        };

        let mut record = HashMap::new();
        record.insert("name".to_string(), json!(name));
        record.insert("email".to_string(), json!(email));
        record.insert("password_hash".to_string(), json!(password_hash));
        record.insert("role".to_string(), json!(role));

        QueryBuilder::insert("users", record)
    }
}

pub struct UserFactory {
    count: usize,
}

impl UserFactory {
    pub fn new() -> Self {
        Self { count: 1 }
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn create(&self) -> Result<Vec<DatabaseRow>> {
        let mut created = Vec::new();
        for i in 1..=self.count {
            let mut record = HashMap::new();
            record.insert("name".to_string(), json!(format!("User {}", i)));
            record.insert("email".to_string(), json!(format!("user{}@example.com", i)));
            record.insert("password_hash".to_string(), json!("$agilang$dev_hash$123"));
            record.insert("role".to_string(), json!("user"));

            let row = QueryBuilder::insert("users", record)?;
            created.push(row);
        }
        Ok(created)
    }
}

impl Default for UserFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_framework_database::{
        clear_thread_local_database_path, set_thread_local_database_path,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_db_path(name: &str) -> String {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("agilang-framework-seeding-{name}-{stamp}.agidb"))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn test_first_user_becomes_admin_and_second_user_becomes_user() {
        let path = test_db_path("first-user-role");
        set_thread_local_database_path(path);
        QueryBuilder::clear_table("users");

        let u1 = SeederExecutor::assign_first_user_role_transactional(
            "Admin User",
            "admin@example.com",
            "hash1",
        )
        .unwrap();
        assert_eq!(u1.get("role").unwrap(), &json!("admin"));

        let u2 = SeederExecutor::assign_first_user_role_transactional(
            "Regular User",
            "user@example.com",
            "hash2",
        )
        .unwrap();
        assert_eq!(u2.get("role").unwrap(), &json!("user"));
        clear_thread_local_database_path();
    }

    #[test]
    fn test_idempotent_first_or_create() {
        let path = test_db_path("first-or-create");
        set_thread_local_database_path(path);
        QueryBuilder::clear_table("roles");

        let mut attr = HashMap::new();
        attr.insert("name".to_string(), json!("admin"));

        let r1 = SeederExecutor::first_or_create("roles", attr.clone(), HashMap::new()).unwrap();
        let r2 = SeederExecutor::first_or_create("roles", attr, HashMap::new()).unwrap();

        assert_eq!(r1.get("id"), r2.get("id"));
        clear_thread_local_database_path();
    }
}
