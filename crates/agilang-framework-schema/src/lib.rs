use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    Id,
    Uuid,
    String(usize),
    Text,
    Integer,
    BigInteger,
    Decimal(u32, u32),
    Boolean,
    Date,
    DateTime,
    Timestamp,
    Json,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyRef {
    pub ref_table: String,
    pub ref_column: String,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub name: String,
    pub col_type: ColumnType,
    pub nullable: bool,
    pub default_val: Option<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub is_unsigned: bool,
    pub auto_increment: bool,
    pub foreign_key: Option<ForeignKeyRef>,
}

impl ColumnDefinition {
    pub fn new(name: impl Into<String>, col_type: ColumnType) -> Self {
        Self {
            name: name.into(),
            col_type,
            nullable: false,
            default_val: None,
            is_unique: false,
            is_primary: false,
            is_unsigned: false,
            auto_increment: false,
            foreign_key: None,
        }
    }

    pub fn nullable(&mut self) -> &mut Self {
        self.nullable = true;
        self
    }

    pub fn default(&mut self, val: impl Into<String>) -> &mut Self {
        self.default_val = Some(val.into());
        self
    }

    pub fn unique(&mut self) -> &mut Self {
        self.is_unique = true;
        self
    }

    pub fn primary(&mut self) -> &mut Self {
        self.is_primary = true;
        self
    }

    pub fn references(&mut self, ref_column: &str, ref_table: &str) -> &mut Self {
        self.foreign_key = Some(ForeignKeyRef {
            ref_table: ref_table.to_string(),
            ref_column: ref_column.to_string(),
            on_delete: Some("cascade".to_string()),
            on_update: Some("cascade".to_string()),
        });
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
}

pub struct Blueprint {
    pub table_name: String,
    pub columns: Vec<ColumnDefinition>,
}

impl Blueprint {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            table_name: name.into(),
            columns: Vec::new(),
        }
    }

    pub fn id(&mut self) -> &mut ColumnDefinition {
        let mut col = ColumnDefinition::new("id", ColumnType::Id);
        col.is_primary = true;
        col.auto_increment = true;
        self.columns.push(col);
        self.columns.last_mut().unwrap()
    }

    pub fn string(&mut self, name: &str, length: usize) -> &mut ColumnDefinition {
        self.columns
            .push(ColumnDefinition::new(name, ColumnType::String(length)));
        self.columns.last_mut().unwrap()
    }

    pub fn text(&mut self, name: &str) -> &mut ColumnDefinition {
        self.columns
            .push(ColumnDefinition::new(name, ColumnType::Text));
        self.columns.last_mut().unwrap()
    }

    pub fn integer(&mut self, name: &str) -> &mut ColumnDefinition {
        self.columns
            .push(ColumnDefinition::new(name, ColumnType::Integer));
        self.columns.last_mut().unwrap()
    }

    pub fn boolean(&mut self, name: &str) -> &mut ColumnDefinition {
        self.columns
            .push(ColumnDefinition::new(name, ColumnType::Boolean));
        self.columns.last_mut().unwrap()
    }

    pub fn timestamps(&mut self) {
        let mut created = ColumnDefinition::new("created_at", ColumnType::Timestamp);
        created.nullable = true;
        let mut updated = ColumnDefinition::new("updated_at", ColumnType::Timestamp);
        updated.nullable = true;
        self.columns.push(created);
        self.columns.push(updated);
    }

    pub fn soft_deletes(&mut self) {
        let mut deleted = ColumnDefinition::new("deleted_at", ColumnType::Timestamp);
        deleted.nullable = true;
        self.columns.push(deleted);
    }
}

pub trait SchemaDialect {
    fn compile_create_table(&self, table: &TableDefinition) -> Result<Vec<String>>;
    fn compile_drop_table(&self, table: &str, if_exists: bool) -> String;
    fn quote_identifier(&self, identifier: &str) -> Result<String>;
}

fn validate_identifier(id: &str) -> Result<()> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("invalid identifier token: `{}`", id);
    }
    Ok(())
}

pub struct SqliteDialect;

impl SchemaDialect for SqliteDialect {
    fn quote_identifier(&self, identifier: &str) -> Result<String> {
        validate_identifier(identifier)?;
        Ok(format!("\"{}\"", identifier))
    }

    fn compile_create_table(&self, table: &TableDefinition) -> Result<Vec<String>> {
        let table_quoted = self.quote_identifier(&table.name)?;
        let mut col_defs = Vec::new();

        for col in &table.columns {
            let col_quoted = self.quote_identifier(&col.name)?;
            let type_str = match col.col_type {
                ColumnType::Id => "INTEGER PRIMARY KEY AUTOINCREMENT",
                ColumnType::Uuid => "TEXT",
                ColumnType::String(_) => "TEXT",
                ColumnType::Text => "TEXT",
                ColumnType::Integer | ColumnType::BigInteger => "INTEGER",
                ColumnType::Decimal(_, _) => "NUMERIC",
                ColumnType::Boolean => "INTEGER",
                ColumnType::Date | ColumnType::DateTime | ColumnType::Timestamp => "TEXT",
                ColumnType::Json => "TEXT",
                ColumnType::Binary => "BLOB",
            };

            let mut stmt = format!("{} {}", col_quoted, type_str);
            if !col.nullable && col.col_type != ColumnType::Id {
                stmt.push_str(" NOT NULL");
            }
            if col.is_unique && col.col_type != ColumnType::Id {
                stmt.push_str(" UNIQUE");
            }
            if let Some(def) = &col.default_val {
                stmt.push_str(&format!(" DEFAULT {}", def));
            }
            col_defs.push(stmt);
        }

        let create_sql = format!(
            "CREATE TABLE {} (\n  {}\n);",
            table_quoted,
            col_defs.join(",\n  ")
        );
        Ok(vec![create_sql])
    }

    fn compile_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists {
            format!("DROP TABLE IF EXISTS \"{}\";", table)
        } else {
            format!("DROP TABLE \"{}\";", table)
        }
    }
}

pub struct PostgresDialect;

impl SchemaDialect for PostgresDialect {
    fn quote_identifier(&self, identifier: &str) -> Result<String> {
        validate_identifier(identifier)?;
        Ok(format!("\"{}\"", identifier))
    }

    fn compile_create_table(&self, table: &TableDefinition) -> Result<Vec<String>> {
        let table_quoted = self.quote_identifier(&table.name)?;
        let mut col_defs = Vec::new();

        for col in &table.columns {
            let col_quoted = self.quote_identifier(&col.name)?;
            let type_str = match col.col_type {
                ColumnType::Id => "BIGSERIAL PRIMARY KEY",
                ColumnType::Uuid => "UUID",
                ColumnType::String(len) => {
                    return Ok(vec![format!("{} VARCHAR({})", col_quoted, len)])
                }
                ColumnType::Text => "TEXT",
                ColumnType::Integer => "INTEGER",
                ColumnType::BigInteger => "BIGINT",
                ColumnType::Decimal(p, s) => {
                    return Ok(vec![format!("{} NUMERIC({}, {})", col_quoted, p, s)])
                }
                ColumnType::Boolean => "BOOLEAN",
                ColumnType::Date => "DATE",
                ColumnType::DateTime | ColumnType::Timestamp => "TIMESTAMP WITH TIME ZONE",
                ColumnType::Json => "JSONB",
                ColumnType::Binary => "BYTEA",
            };

            let mut stmt = format!("{} {}", col_quoted, type_str);
            if !col.nullable && col.col_type != ColumnType::Id {
                stmt.push_str(" NOT NULL");
            }
            if col.is_unique && col.col_type != ColumnType::Id {
                stmt.push_str(" UNIQUE");
            }
            col_defs.push(stmt);
        }

        let create_sql = format!(
            "CREATE TABLE {} (\n  {}\n);",
            table_quoted,
            col_defs.join(",\n  ")
        );
        Ok(vec![create_sql])
    }

    fn compile_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists {
            format!("DROP TABLE IF EXISTS \"{}\" CASCADE;", table)
        } else {
            format!("DROP TABLE \"{}\" CASCADE;", table)
        }
    }
}

pub struct MySqlDialect;

impl SchemaDialect for MySqlDialect {
    fn quote_identifier(&self, identifier: &str) -> Result<String> {
        validate_identifier(identifier)?;
        Ok(format!("`{}`", identifier))
    }

    fn compile_create_table(&self, table: &TableDefinition) -> Result<Vec<String>> {
        let table_quoted = self.quote_identifier(&table.name)?;
        let mut col_defs = Vec::new();

        for col in &table.columns {
            let col_quoted = self.quote_identifier(&col.name)?;
            let type_str = match col.col_type {
                ColumnType::Id => "BIGINT AUTO_INCREMENT PRIMARY KEY",
                ColumnType::Uuid => "CHAR(36)",
                ColumnType::String(len) => {
                    return Ok(vec![format!("{} VARCHAR({})", col_quoted, len)])
                }
                ColumnType::Text => "TEXT",
                ColumnType::Integer => "INT",
                ColumnType::BigInteger => "BIGINT",
                ColumnType::Decimal(p, s) => {
                    return Ok(vec![format!("{} DECIMAL({}, {})", col_quoted, p, s)])
                }
                ColumnType::Boolean => "TINYINT(1)",
                ColumnType::Date => "DATE",
                ColumnType::DateTime | ColumnType::Timestamp => "DATETIME",
                ColumnType::Json => "JSON",
                ColumnType::Binary => "LONGBLOB",
            };

            let mut stmt = format!("{} {}", col_quoted, type_str);
            if !col.nullable && col.col_type != ColumnType::Id {
                stmt.push_str(" NOT NULL");
            }
            if col.is_unique && col.col_type != ColumnType::Id {
                stmt.push_str(" UNIQUE");
            }
            col_defs.push(stmt);
        }

        let create_sql = format!(
            "CREATE TABLE {} (\n  {}\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;",
            table_quoted,
            col_defs.join(",\n  ")
        );
        Ok(vec![create_sql])
    }

    fn compile_drop_table(&self, table: &str, if_exists: bool) -> String {
        if if_exists {
            format!("DROP TABLE IF EXISTS `{}`;", table)
        } else {
            format!("DROP TABLE `{}`;", table)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_create_table_sql() {
        let mut bp = Blueprint::new("users");
        bp.id();
        bp.string("name", 150);
        bp.string("email", 254).unique();
        bp.timestamps();

        let table_def = TableDefinition {
            name: bp.table_name,
            columns: bp.columns,
        };

        let dialect = SqliteDialect;
        let sql = dialect.compile_create_table(&table_def).unwrap();
        assert!(sql[0].contains("CREATE TABLE \"users\""));
        assert!(sql[0].contains("\"id\" INTEGER PRIMARY KEY AUTOINCREMENT"));
        assert!(sql[0].contains("\"email\" TEXT NOT NULL UNIQUE"));
    }

    #[test]
    fn test_rejects_invalid_identifier() {
        let dialect = SqliteDialect;
        assert!(dialect.quote_identifier("users; DROP TABLE users").is_err());
    }
}
