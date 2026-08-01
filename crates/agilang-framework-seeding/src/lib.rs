use agilang_framework_database::{AttributeCast, DatabaseConfig, ModelRepository, OrmModel};
use agilang_database_driver::DatabaseConnection;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub type DatabaseRow = HashMap<String, Value>;

pub trait Seeder {
    fn run(&self) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedIdempotencyPolicy {
    KeepExisting,
    Replace,
    FailOnExisting,
}

#[derive(Debug, Clone)]
pub struct ProjectSeedConfig {
    pub database: DatabaseConfig,
}

impl ProjectSeedConfig {
    pub fn from_project_root(project_root: &Path) -> Result<Self> {
        let env = load_env(project_root)?;
        let config_path = project_root.join("config/database.agi");
        let config = std::fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;

        let default_driver =
            extract_default_driver(&config).unwrap_or_else(|| "agidb".to_string());
        let database = match default_driver.as_str() {
            "agidb" => DatabaseConfig::agidb(resolve_database_path(
                project_root,
                &env,
                extract_connection_value(&config, "agidb", "database")
                    .unwrap_or_else(|| "storage/database/main.agidb".to_string()),
            )),
            "sqlite" => DatabaseConfig::sqlite(resolve_database_path(
                project_root,
                &env,
                extract_connection_value(&config, "sqlite", "database")
                    .unwrap_or_else(|| "storage/database/main.sqlite".to_string()),
            )),
            "mysql" => DatabaseConfig::mysql(
                env.get("DB_HOST")
                    .cloned()
                    .or_else(|| extract_connection_value(&config, "mysql", "host"))
                    .unwrap_or_else(|| "127.0.0.1".to_string()),
                env.get("DB_PORT")
                    .and_then(|value| value.parse::<u16>().ok())
                    .or_else(|| {
                        extract_connection_value(&config, "mysql", "port")
                            .and_then(|value| value.parse::<u16>().ok())
                    })
                    .unwrap_or(3306),
                env.get("DB_DATABASE")
                    .cloned()
                    .or_else(|| extract_connection_value(&config, "mysql", "database"))
                    .unwrap_or_else(|| "app".to_string()),
                env.get("DB_USERNAME").cloned().unwrap_or_default(),
                env.get("DB_PASSWORD").cloned().unwrap_or_default(),
            ),
            other => bail!("E6401 unsupported configured seed driver `{other}`"),
        };

        Ok(Self { database })
    }
}

pub struct SeederExecutor;

impl SeederExecutor {
    pub fn first_or_create<M: OrmModel>(
        repo: &mut ModelRepository<M>,
        attributes: HashMap<String, Value>,
        values: HashMap<String, Value>,
        policy: SeedIdempotencyPolicy,
    ) -> Result<DatabaseRow> {
        let existing = find_existing(repo, &attributes)?;
        match (existing, policy) {
            (Some(row), SeedIdempotencyPolicy::KeepExisting) => Ok(row),
            (Some(_), SeedIdempotencyPolicy::FailOnExisting) => {
                bail!("E6402 seed conflict for `{}`", M::table_name())
            }
            (Some(row), SeedIdempotencyPolicy::Replace) => {
                let id = row
                    .get(M::primary_key())
                    .and_then(as_i64)
                    .ok_or_else(|| anyhow!("existing seed row missing primary key"))?;
                let mut merged = row;
                for (key, value) in attributes {
                    merged.insert(key, value);
                }
                for (key, value) in values {
                    merged.insert(key, value);
                }
                let updated = repo.update(id, merged)?;
                Ok(updated.attributes())
            }
            (None, _) => {
                let mut merged = attributes;
                for (key, value) in values {
                    merged.insert(key, value);
                }
                let created = repo.create(merged)?;
                Ok(created.attributes())
            }
        }
    }

    pub fn assign_first_user_role_transactional(
        config: &DatabaseConfig,
        name: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<DatabaseRow> {
        let mut repo = ModelRepository::<SeedUser>::connect(config)?;
        repo.connection_mut().begin_transaction()?;
        let execution = (|| -> Result<DatabaseRow> {
            let existing_users = repo.all()?;
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

            let created = repo.create(record)?;
            Ok(created.attributes())
        })();

        match execution {
            Ok(row) => {
                repo.connection_mut().commit()?;
                Ok(row)
            }
            Err(err) => {
                let _ = repo.connection_mut().rollback();
                Err(err)
            }
        }
    }

    pub fn seed_default_users(config: &DatabaseConfig, count: usize, deterministic: bool) -> Result<Vec<DatabaseRow>> {
        UserFactory::new()
            .count(count)
            .deterministic(deterministic)
            .create_with_config(config, SeedIdempotencyPolicy::KeepExisting)
    }
}

pub struct UserFactory {
    count: usize,
    deterministic: bool,
}

impl UserFactory {
    pub fn new() -> Self {
        Self {
            count: 1,
            deterministic: false,
        }
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn deterministic(mut self, deterministic: bool) -> Self {
        self.deterministic = deterministic;
        self
    }

    pub fn create_with_config(
        &self,
        config: &DatabaseConfig,
        policy: SeedIdempotencyPolicy,
    ) -> Result<Vec<DatabaseRow>> {
        let mut repo = ModelRepository::<SeedUser>::connect(config)?;
        repo.connection_mut().begin_transaction()?;
        let execution = (|| -> Result<Vec<DatabaseRow>> {
            let mut created = Vec::new();
            for i in 1..=self.count {
                let mut attributes = HashMap::new();
                attributes.insert(
                    "email".to_string(),
                    json!(if self.deterministic {
                        format!("test-user-{}@example.com", i)
                    } else {
                        format!("user-{}-{}@example.com", i, unique_seed_suffix())
                    }),
                );
                let mut values = HashMap::new();
                values.insert("name".to_string(), json!(format!("User {}", i)));
                values.insert("password_hash".to_string(), json!("$agilang$seed$placeholder"));
                values.insert("role".to_string(), json!("user"));
                let row = SeederExecutor::first_or_create(&mut repo, attributes, values, policy)?;
                created.push(row);
            }
            Ok(created)
        })();

        match execution {
            Ok(rows) => {
                repo.connection_mut().commit()?;
                Ok(rows)
            }
            Err(err) => {
                let _ = repo.connection_mut().rollback();
                Err(err)
            }
        }
    }
}

impl Default for UserFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
struct SeedUser {
    attributes: HashMap<String, Value>,
}

impl OrmModel for SeedUser {
    fn table_name() -> &'static str {
        "users"
    }

    fn fillable() -> &'static [&'static str] {
        &["name", "email", "password_hash", "role"]
    }

    fn guarded() -> &'static [&'static str] {
        &[]
    }

    fn casts() -> &'static [(&'static str, AttributeCast)] {
        &[("id", AttributeCast::Integer)]
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

fn find_existing<M: OrmModel>(
    repo: &mut ModelRepository<M>,
    attributes: &HashMap<String, Value>,
) -> Result<Option<DatabaseRow>> {
    let Some((first_key, first_value)) = attributes.iter().next() else {
        return Ok(None);
    };
    let rows = repo.where_eq(first_key, first_value.clone())?;
    for row in rows {
        let attrs = row.attributes();
        let matches = attributes
            .iter()
            .all(|(key, expected)| attrs.get(key) == Some(expected));
        if matches {
            return Ok(Some(attrs));
        }
    }
    Ok(None)
}

fn load_env(project_root: &Path) -> Result<HashMap<String, String>> {
    let mut env = HashMap::new();
    let env_path = project_root.join(".env");
    if env_path.exists() {
        let content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("failed to read {}", env_path.display()))?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                env.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    Ok(env)
}

fn extract_default_driver(config: &str) -> Option<String> {
    if let Some(pos) = config.find("\"default\":") {
        let rest = &config[pos + 10..];
        return extract_string_literal(rest);
    }
    None
}

fn extract_connection_value(config: &str, connection: &str, key: &str) -> Option<String> {
    let marker = format!("\"{connection}\":");
    let start = config.find(&marker)?;
    let section = &config[start..];
    let key_marker = format!("\"{key}\":");
    let key_pos = section.find(&key_marker)?;
    extract_string_literal(&section[key_pos + key_marker.len()..])
}

fn extract_string_literal(input: &str) -> Option<String> {
    let start = input.find('"')?;
    let rest = &input[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn resolve_database_path(project_root: &Path, env: &HashMap<String, String>, value: String) -> String {
    let expanded = if let Some(name) = env.get("APP_NAME") {
        value.replace("{name}", name)
    } else {
        value
    };
    let path = PathBuf::from(&expanded);
    if path.is_absolute() {
        expanded
    } else {
        project_root.join(path).to_string_lossy().to_string()
    }
}

fn unique_seed_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_database_driver::DatabaseConnection;
    use agilang_framework_database::{FrameworkConnection, FrameworkDriver};
    use std::time::{SystemTime, UNIX_EPOCH};

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
                .join(format!("agilang-seeding-{}.agidb", unique_suffix(name)))
                .to_string_lossy()
                .to_string(),
        )
    }

    fn sqlite_config(name: &str) -> DatabaseConfig {
        DatabaseConfig::sqlite(
            std::env::temp_dir()
                .join(format!("agilang-seeding-{}.sqlite", unique_suffix(name)))
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
            FrameworkDriver::Agidb => {
                "CREATE TABLE users (id INTEGER, name TEXT, email TEXT, password_hash TEXT, role TEXT, created_at TEXT, updated_at TEXT)"
            }
            FrameworkDriver::Sqlite => {
                "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, email TEXT, password_hash TEXT, role TEXT, created_at TEXT, updated_at TEXT)"
            }
            FrameworkDriver::Mysql => {
                "CREATE TABLE users (id BIGINT AUTO_INCREMENT PRIMARY KEY, name TEXT, email TEXT, password_hash TEXT, role TEXT, created_at TEXT, updated_at TEXT)"
            }
        };
        conn.execute(create_sql, &[])?;
        Ok(())
    }

    fn seeding_conformance(config: DatabaseConfig) -> Result<()> {
        prepare_schema(&config)?;
        let admin = SeederExecutor::assign_first_user_role_transactional(
            &config,
            "Admin",
            "admin@example.com",
            "hash-1",
        )?;
        assert_eq!(admin.get("role"), Some(&json!("admin")));

        let user = SeederExecutor::assign_first_user_role_transactional(
            &config,
            "User",
            "user@example.com",
            "hash-2",
        )?;
        assert_eq!(user.get("role"), Some(&json!("user")));

        let deterministic = UserFactory::new()
            .count(2)
            .deterministic(true)
            .create_with_config(&config, SeedIdempotencyPolicy::KeepExisting)?;
        assert_eq!(deterministic.len(), 2);

        let repeated = UserFactory::new()
            .count(2)
            .deterministic(true)
            .create_with_config(&config, SeedIdempotencyPolicy::KeepExisting)?;
        assert_eq!(repeated.len(), 2);

        let mut repo = ModelRepository::<SeedUser>::connect(&config)?;
        let rows = repo.all()?;
        assert_eq!(rows.len(), 4);

        let conflict = UserFactory::new()
            .count(1)
            .deterministic(true)
            .create_with_config(&config, SeedIdempotencyPolicy::FailOnExisting);
        assert!(conflict.is_err());

        Ok(())
    }

    #[test]
    fn agidb_seeding_conformance() {
        seeding_conformance(agidb_config("agidb")).unwrap();
    }

    #[test]
    fn sqlite_seeding_conformance() {
        seeding_conformance(sqlite_config("sqlite")).unwrap();
    }

    #[test]
    fn mysql_seeding_conformance() {
        let Some(config) = mysql_config() else {
            eprintln!("skipping mysql seeding conformance: real mysql server unavailable");
            return;
        };
        seeding_conformance(config).unwrap();
    }

    #[test]
    fn project_seed_config_uses_database_selection() {
        let root = std::env::temp_dir().join(unique_suffix("project-seed-config"));
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join(".env"),
            "APP_NAME=testapp\nDB_HOST=127.0.0.1\nDB_PORT=3306\nDB_DATABASE=testdb\nDB_USERNAME=root\nDB_PASSWORD=\n",
        )
        .unwrap();
        std::fs::write(
            root.join("config/database.agi"),
            r#"return {
    "default": "sqlite",
    "connections": {
        "sqlite": {
            "driver": "sqlite",
            "database": "storage/database/main.sqlite"
        }
    },
    "strict_driver_selection": true
}"#,
        )
        .unwrap();

        let cfg = ProjectSeedConfig::from_project_root(&root).unwrap();
        assert_eq!(cfg.database.driver, FrameworkDriver::Sqlite);
        assert!(cfg.database.database.ends_with("storage\\database\\main.sqlite") || cfg.database.database.ends_with("storage/database/main.sqlite"));

        let _ = std::fs::remove_dir_all(root);
    }
}
