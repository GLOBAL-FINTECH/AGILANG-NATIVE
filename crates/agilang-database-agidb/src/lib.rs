use agilang_database_driver::{
    DatabaseCapability, DatabaseConnection, DatabaseDriverKind, DatabaseHealth, DatabaseRow,
    DatabaseValue, ExecutionResult,
};
use agilang_database_storage::{PageManager, MAGIC_BYTES};
use agilang_database_wal::WriteAheadLog;
use anyhow::{bail, Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static DB_PROCESS_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
static DB_STORE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug)]
pub struct AgiDbEngine {
    pub path: String,
    pub storage: PageManager,
    pub wal: WriteAheadLog,
    pub air_gapped: bool,
}

impl AgiDbEngine {
    pub fn open(path: impl Into<String>, air_gapped: bool) -> Result<Self> {
        let mut guard = DB_PROCESS_LOCK.lock().unwrap();
        if *guard {
            bail!("E6301 Database already opened for writing: single writer lock active");
        }
        *guard = true;

        let path = path.into();
        AgiDbStore::open(Path::new(&path))?;

        Ok(Self {
            path,
            storage: PageManager::new(),
            wal: WriteAheadLog::new(),
            air_gapped,
        })
    }

    pub fn close(&mut self) {
        let mut guard = DB_PROCESS_LOCK.lock().unwrap();
        *guard = false;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgiDbStore {
    format: String,
    version: u32,
    next_row_id: i64,
    tables: BTreeMap<String, Table>,
}

impl Default for AgiDbStore {
    fn default() -> Self {
        Self {
            format: "AGIDB001".to_string(),
            version: 1,
            next_row_id: 1,
            tables: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Table {
    columns: Vec<String>,
    rows: Vec<BTreeMap<String, DatabaseValue>>,
}

impl AgiDbStore {
    fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            let store = Self::default();
            store.persist(path)?;
            return Ok(store);
        }

        let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        if bytes.is_empty() {
            let store = Self::default();
            store.persist(path)?;
            return Ok(store);
        }
        if bytes.len() < MAGIC_BYTES.len() || &bytes[..MAGIC_BYTES.len()] != MAGIC_BYTES {
            bail!("E6302 Invalid database format: magic header does not match AGIDB001");
        }

        let payload = std::str::from_utf8(&bytes[MAGIC_BYTES.len()..])
            .context("AGIDB payload is not valid UTF-8")?;
        if payload.trim().is_empty() {
            return Ok(Self::default());
        }

        let store: Self =
            serde_json::from_str(payload).context("AGIDB payload is not valid JSON")?;
        if store.format != "AGIDB001" || store.version != 1 {
            bail!("E6303 Unsupported database version: {}", store.version);
        }
        Ok(store)
    }

    fn persist(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let mut bytes = MAGIC_BYTES.to_vec();
        bytes.extend_from_slice(
            serde_json::to_string_pretty(self)
                .context("failed to serialize AGIDB store")?
                .as_bytes(),
        );
        let tmp_name = format!(
            ".{}.tmp-{}-{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("agidb"),
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let tmp_path = path.with_file_name(tmp_name);
        {
            let mut file = File::create(&tmp_path)
                .with_context(|| format!("failed to create {}", tmp_path.display()))?;
            file.write_all(&bytes)
                .with_context(|| format!("failed to write {}", tmp_path.display()))?;
            file.sync_all()
                .with_context(|| format!("failed to flush {}", tmp_path.display()))?;
        }
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("failed to replace {}", path.display()))?;
        }
        fs::rename(&tmp_path, path)
            .with_context(|| format!("failed to move {} into place", tmp_path.display()))?;
        Ok(())
    }
}

pub struct AgiDbConnection {
    pub engine_path: String,
    pub in_transaction: bool,
    transaction_snapshot: Option<AgiDbStore>,
}

impl AgiDbConnection {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            engine_path: path.into(),
            in_transaction: false,
            transaction_snapshot: None,
        }
    }

    fn path(&self) -> PathBuf {
        PathBuf::from(&self.engine_path)
    }

    fn read_store(&self) -> Result<AgiDbStore> {
        AgiDbStore::open(&self.path())
    }

    fn write_store(&self, store: &AgiDbStore) -> Result<()> {
        store.persist(&self.path())
    }
}

impl DatabaseConnection for AgiDbConnection {
    fn execute(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<ExecutionResult> {
        let _guard = DB_STORE_LOCK.lock().unwrap();
        let mut store = self.read_store()?;
        let statement = sql.trim().trim_end_matches(';').trim();
        let upper = statement.to_ascii_uppercase();

        let result = if upper.starts_with("CREATE TABLE") {
            execute_create_table(&mut store, statement)?
        } else if upper.starts_with("DROP TABLE") {
            execute_drop_table(&mut store, statement)?
        } else if upper.starts_with("INSERT INTO") {
            execute_insert(&mut store, statement, params)?
        } else if upper.starts_with("UPDATE") {
            execute_update(&mut store, statement, params)?
        } else if upper.starts_with("DELETE FROM") {
            execute_delete(&mut store, statement, params)?
        } else {
            bail!("E6310 Unsupported AGIDB statement: {}", sql);
        };

        self.write_store(&store)?;
        Ok(result)
    }

    fn query(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<Vec<DatabaseRow>> {
        let store = self.read_store()?;
        execute_select(&store, sql.trim().trim_end_matches(';').trim(), params)
    }

    fn begin_transaction(&mut self) -> Result<()> {
        if self.in_transaction {
            bail!("E6311 Transaction already active");
        }
        let _guard = DB_STORE_LOCK.lock().unwrap();
        self.transaction_snapshot = Some(self.read_store()?);
        self.in_transaction = true;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        self.transaction_snapshot = None;
        self.in_transaction = false;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        if let Some(snapshot) = self.transaction_snapshot.take() {
            let _guard = DB_STORE_LOCK.lock().unwrap();
            self.write_store(&snapshot)?;
        }
        self.in_transaction = false;
        Ok(())
    }

    fn health_check(&mut self) -> Result<DatabaseHealth> {
        let _ = self.read_store()?;
        Ok(DatabaseHealth {
            healthy: true,
            message: "ok".to_string(),
        })
    }

    fn inspect_tables(&mut self) -> Result<Vec<String>> {
        let store = self.read_store()?;
        Ok(store.tables.keys().cloned().collect())
    }

    fn acquire_migration_lock(&mut self, lock_name: &str) -> Result<()> {
        let _ = self.execute(
            "CREATE TABLE agilang_migration_locks (id INTEGER, name TEXT, acquired_at TEXT)",
            &[],
        );
        let existing = self.query(
            "SELECT * FROM agilang_migration_locks WHERE name = ? LIMIT 1",
            &[DatabaseValue::Text(lock_name.to_string())],
        )?;
        if !existing.is_empty() {
            bail!("E6110 Another migration process is currently active");
        }
        self.execute(
            "INSERT INTO agilang_migration_locks (name, acquired_at) VALUES (?, ?)",
            &[
                DatabaseValue::Text(lock_name.to_string()),
                DatabaseValue::Text(format!("{:?}", std::time::SystemTime::now())),
            ],
        )?;
        Ok(())
    }

    fn release_migration_lock(&mut self, lock_name: &str) -> Result<()> {
        let _ = self.execute(
            "DELETE FROM agilang_migration_locks WHERE name = ?",
            &[DatabaseValue::Text(lock_name.to_string())],
        )?;
        Ok(())
    }

    fn capabilities(&self) -> Vec<DatabaseCapability> {
        vec![
            DatabaseCapability::Transactions,
            DatabaseCapability::PreparedStatements,
            DatabaseCapability::SchemaInspection,
            DatabaseCapability::MigrationLocking,
        ]
    }

    fn driver_kind(&self) -> DatabaseDriverKind {
        DatabaseDriverKind::AgiDb
    }
}

fn execute_create_table(store: &mut AgiDbStore, sql: &str) -> Result<ExecutionResult> {
    let after = sql
        .split_once('(')
        .map(|(head, _)| head)
        .unwrap_or(sql)
        .trim();
    let table_name = after
        .split_whitespace()
        .last()
        .context("CREATE TABLE requires a table name")?
        .trim_matches('"')
        .to_string();

    let columns = sql
        .split_once('(')
        .and_then(|(_, rest)| rest.rsplit_once(')').map(|(cols, _)| cols))
        .map(|cols| {
            cols.split(',')
                .filter_map(|col| col.split_whitespace().next())
                .filter(|col| {
                    !col.eq_ignore_ascii_case("PRIMARY") && !col.eq_ignore_ascii_case("UNIQUE")
                })
                .map(|col| col.trim_matches('"').to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    store.tables.entry(table_name).or_insert(Table {
        columns,
        rows: Vec::new(),
    });
    Ok(ExecutionResult {
        rows_affected: 0,
        last_insert_id: None,
    })
}

fn execute_insert(
    store: &mut AgiDbStore,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<ExecutionResult> {
    let upper = sql.to_ascii_uppercase();
    let into_pos = upper.find("INTO").context("INSERT requires INTO")? + 4;
    let values_pos = upper.find("VALUES").context("INSERT requires VALUES")?;
    let target = sql[into_pos..values_pos].trim();
    let (table_name, columns) = if let Some((name, cols)) = target.split_once('(') {
        (
            name.trim().trim_matches('"').to_string(),
            cols.trim_end_matches(')')
                .split(',')
                .map(|c| c.trim().trim_matches('"').to_string())
                .collect::<Vec<_>>(),
        )
    } else {
        let name = target.trim().trim_matches('"').to_string();
        let columns = store
            .tables
            .get(&name)
            .map(|table| table.columns.clone())
            .unwrap_or_default();
        (name, columns)
    };

    let table = store.tables.entry(table_name).or_default();
    for column in &columns {
        if !table.columns.contains(column) {
            table.columns.push(column.clone());
        }
    }

    let values = parse_insert_values(&sql[values_pos + 6..], params)?;
    if !columns.is_empty() && values.len() != columns.len() {
        bail!("E6312 INSERT column count does not match value count");
    }

    let mut row = BTreeMap::new();
    for (idx, value) in values.into_iter().enumerate() {
        let Some(column) = columns.get(idx).or_else(|| table.columns.get(idx)) else {
            bail!("E6313 INSERT value has no target column");
        };
        row.insert(column.clone(), value);
    }
    if !row.contains_key("id") {
        let id = store.next_row_id;
        store.next_row_id += 1;
        row.insert("id".to_string(), DatabaseValue::Integer(id));
    }
    let last_insert_id = match row.get("id") {
        Some(DatabaseValue::Integer(id)) => Some(*id),
        _ => None,
    };
    table.rows.push(row);

    Ok(ExecutionResult {
        rows_affected: 1,
        last_insert_id,
    })
}

fn execute_drop_table(store: &mut AgiDbStore, sql: &str) -> Result<ExecutionResult> {
    let parts = sql.split_whitespace().collect::<Vec<_>>();
    let table_name = match parts.as_slice() {
        [_, _, name] => *name,
        [_, _, if_kw, not_kw, exists_kw, name]
            if if_kw.eq_ignore_ascii_case("IF")
                && not_kw.eq_ignore_ascii_case("NOT")
                && exists_kw.eq_ignore_ascii_case("EXISTS") =>
        {
            *name
        }
        [_, _, if_kw, exists_kw, name]
            if if_kw.eq_ignore_ascii_case("IF") && exists_kw.eq_ignore_ascii_case("EXISTS") =>
        {
            *name
        }
        _ => bail!("E6315 DROP TABLE requires a table name"),
    }
    .trim_matches('"');

    let removed = store.tables.remove(table_name).is_some();
    Ok(ExecutionResult {
        rows_affected: u64::from(removed),
        last_insert_id: None,
    })
}

fn execute_update(
    store: &mut AgiDbStore,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<ExecutionResult> {
    let upper = sql.to_ascii_uppercase();
    let set_pos = upper.find(" SET ").context("UPDATE requires SET")?;
    let where_pos = upper.find(" WHERE ");
    let table_name = sql[6..set_pos].trim().trim_matches('"');
    let assignments = match where_pos {
        Some(pos) => &sql[set_pos + 5..pos],
        None => &sql[set_pos + 5..],
    };
    let conditions = where_pos.map(|pos| &sql[pos + 7..]);
    let mut param_index = 0;
    let updates = assignments
        .split(',')
        .map(|assignment| {
            let (column, value) = parse_assignment(assignment, params, &mut param_index)?;
            Ok((column, value))
        })
        .collect::<Result<Vec<_>>>()?;

    let table = store
        .tables
        .get_mut(table_name)
        .with_context(|| format!("E6314 Table `{}` does not exist", table_name))?;

    let mut affected = 0;
    for row in &mut table.rows {
        if row_matches(row, conditions, params, param_index)? {
            for (column, value) in &updates {
                row.insert(column.clone(), value.clone());
            }
            affected += 1;
        }
    }
    Ok(ExecutionResult {
        rows_affected: affected,
        last_insert_id: None,
    })
}

fn execute_delete(
    store: &mut AgiDbStore,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<ExecutionResult> {
    let upper = sql.to_ascii_uppercase();
    let where_pos = upper.find(" WHERE ");
    let table_name = match where_pos {
        Some(pos) => sql[11..pos].trim().trim_matches('"'),
        None => sql[11..].trim().trim_matches('"'),
    };
    let conditions = where_pos.map(|pos| &sql[pos + 7..]);

    let table = store
        .tables
        .get_mut(table_name)
        .with_context(|| format!("E6314 Table `{}` does not exist", table_name))?;
    let initial_len = table.rows.len();
    table
        .rows
        .retain(|row| !row_matches(row, conditions, params, 0).unwrap_or(false));

    Ok(ExecutionResult {
        rows_affected: (initial_len - table.rows.len()) as u64,
        last_insert_id: None,
    })
}

fn execute_select(
    store: &AgiDbStore,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<Vec<DatabaseRow>> {
    let upper = sql.to_ascii_uppercase();
    if !upper.starts_with("SELECT ") {
        bail!("E6310 Unsupported AGIDB query: {}", sql);
    }
    let from_pos = upper.find(" FROM ").context("SELECT requires FROM")?;
    let where_pos = upper.find(" WHERE ");
    let limit_pos = upper.find(" LIMIT ");
    let end_table = where_pos.or(limit_pos).unwrap_or(sql.len());
    let table_name = sql[from_pos + 6..end_table].trim().trim_matches('"');
    let conditions = where_pos.map(|pos| {
        let end = limit_pos.unwrap_or(sql.len());
        &sql[pos + 7..end]
    });
    let limit = limit_pos.and_then(|pos| sql[pos + 7..].trim().parse::<usize>().ok());

    let table = match store.tables.get(table_name) {
        Some(table) => table,
        None => return Ok(Vec::new()),
    };

    let mut rows = Vec::new();
    for row in &table.rows {
        if row_matches(row, conditions, params, 0)? {
            rows.push(row.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        }
        if limit.is_some_and(|count| rows.len() >= count) {
            break;
        }
    }
    Ok(rows)
}

fn parse_insert_values(sql: &str, params: &[DatabaseValue]) -> Result<Vec<DatabaseValue>> {
    let values = sql
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    let mut param_index = 0;
    values
        .into_iter()
        .map(|value| parse_value(value, params, &mut param_index))
        .collect()
}

fn parse_assignment(
    assignment: &str,
    params: &[DatabaseValue],
    param_index: &mut usize,
) -> Result<(String, DatabaseValue)> {
    let (column, value) = assignment
        .split_once('=')
        .context("assignment requires `=`")?;
    Ok((
        column.trim().trim_matches('"').to_string(),
        parse_value(value.trim(), params, param_index)?,
    ))
}

fn parse_value(
    raw: &str,
    params: &[DatabaseValue],
    param_index: &mut usize,
) -> Result<DatabaseValue> {
    if raw == "?" {
        let value = params
            .get(*param_index)
            .with_context(|| format!("missing SQL parameter {}", *param_index + 1))?
            .clone();
        *param_index += 1;
        return Ok(value);
    }
    if raw.eq_ignore_ascii_case("NULL") {
        return Ok(DatabaseValue::Null);
    }
    if raw.eq_ignore_ascii_case("TRUE") {
        return Ok(DatabaseValue::Boolean(true));
    }
    if raw.eq_ignore_ascii_case("FALSE") {
        return Ok(DatabaseValue::Boolean(false));
    }
    if let Ok(number) = raw.parse::<i64>() {
        return Ok(DatabaseValue::Integer(number));
    }
    if let Ok(number) = raw.parse::<f64>() {
        return Ok(DatabaseValue::Float(number));
    }
    Ok(DatabaseValue::Text(
        raw.trim_matches('\'').trim_matches('"').to_string(),
    ))
}

fn row_matches(
    row: &BTreeMap<String, DatabaseValue>,
    conditions: Option<&str>,
    params: &[DatabaseValue],
    param_offset: usize,
) -> Result<bool> {
    let Some(conditions) = conditions else {
        return Ok(true);
    };

    let mut param_index = param_offset;
    for condition in conditions.split(" AND ") {
        let (column, raw_value) = condition
            .split_once('=')
            .context("only equality WHERE conditions are supported")?;
        let expected = parse_value(raw_value.trim(), params, &mut param_index)?;
        let actual = row.get(column.trim().trim_matches('"'));
        if actual != Some(&expected) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("agidb-{}-{}.agidb", name, nonce))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn test_single_writer_process_lock() {
        let path = temp_db_path("lock");
        let mut eng1 = AgiDbEngine::open(&path, true).unwrap();
        let eng2_err = AgiDbEngine::open(&path, true);

        assert!(eng2_err.is_err());
        assert!(eng2_err.unwrap_err().to_string().contains("E6301"));

        eng1.close();
        let eng3 = AgiDbEngine::open(&path, true);
        assert!(eng3.is_ok());
    }

    #[test]
    fn persists_inserted_rows() {
        let path = temp_db_path("persist");
        let mut conn = AgiDbConnection::new(&path);
        conn.execute(
            "CREATE TABLE users (id INTEGER, name TEXT, email TEXT)",
            &[],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (name, email) VALUES (?, ?)",
            &[
                DatabaseValue::Text("Ada".into()),
                DatabaseValue::Text("ada@example.com".into()),
            ],
        )
        .unwrap();

        let mut reopened = AgiDbConnection::new(&path);
        let rows = reopened
            .query(
                "SELECT * FROM users WHERE email = ? LIMIT 1",
                &[DatabaseValue::Text("ada@example.com".into())],
            )
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name"),
            Some(&DatabaseValue::Text("Ada".into()))
        );
    }

    #[test]
    fn rolls_back_transaction_snapshot() {
        let path = temp_db_path("rollback");
        let mut conn = AgiDbConnection::new(&path);
        conn.execute("CREATE TABLE users (id INTEGER, email TEXT)", &[])
            .unwrap();
        conn.begin_transaction().unwrap();
        conn.execute(
            "INSERT INTO users (email) VALUES (?)",
            &[DatabaseValue::Text("rollback@example.com".into())],
        )
        .unwrap();
        conn.rollback().unwrap();

        let rows = conn.query("SELECT * FROM users", &[]).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn concurrent_writes_keep_valid_json_and_preserve_rows() {
        let path = Arc::new(temp_db_path("concurrent"));
        let mut conn = AgiDbConnection::new(path.as_ref().clone());
        conn.execute("CREATE TABLE users (id INTEGER, email TEXT)", &[])
            .unwrap();

        let mut handles = Vec::new();
        for index in 0..8 {
            let path = Arc::clone(&path);
            handles.push(thread::spawn(move || {
                let mut conn = AgiDbConnection::new(path.as_ref().clone());
                conn.execute(
                    "INSERT INTO users (email) VALUES (?)",
                    &[DatabaseValue::Text(format!("user{index}@example.com"))],
                )
                .unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let bytes = fs::read(path.as_ref()).unwrap();
        assert_eq!(&bytes[..MAGIC_BYTES.len()], MAGIC_BYTES);
        let payload = std::str::from_utf8(&bytes[MAGIC_BYTES.len()..]).unwrap();
        let store: AgiDbStore = serde_json::from_str(payload).unwrap();
        let users = store.tables.get("users").unwrap();
        assert_eq!(users.rows.len(), 8);
    }
}
