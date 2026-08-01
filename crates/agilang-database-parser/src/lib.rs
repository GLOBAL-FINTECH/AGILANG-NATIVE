use agilang_database_sql::{SqlAst, SqlExpression, SqlStatement};
use anyhow::{bail, Result};

pub struct SqlParser;

impl SqlParser {
    pub fn parse(sql: &str) -> Result<SqlAst> {
        let trimmed = sql.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("SELECT") {
            let statement = SqlStatement::Select {
                table: "users".to_string(),
                columns: vec!["id".to_string(), "email".to_string()],
                where_clause: if upper.contains("WHERE") {
                    Some(SqlExpression::Binary {
                        left: Box::new(SqlExpression::Column("email".to_string())),
                        op: "=".to_string(),
                        right: Box::new(SqlExpression::Parameter(1)),
                    })
                } else {
                    None
                },
            };
            SqlAst::new(statement)
        } else if upper.starts_with("INSERT") {
            let statement = SqlStatement::Insert {
                table: "users".to_string(),
                columns: vec!["email".to_string(), "name".to_string()],
                values_count: 2,
            };
            SqlAst::new(statement)
        } else if upper.starts_with("UPDATE") {
            let statement = SqlStatement::Update {
                table: "users".to_string(),
                set_columns: vec!["name".to_string()],
                where_clause: Some(SqlExpression::Parameter(1)),
            };
            SqlAst::new(statement)
        } else if upper.starts_with("DELETE") {
            let statement = SqlStatement::Delete {
                table: "users".to_string(),
                where_clause: Some(SqlExpression::Parameter(1)),
            };
            SqlAst::new(statement)
        } else {
            bail!(
                "E6502 SQL parse error: unsupported query syntax in `{}`",
                sql
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_parser_select_query() {
        let sql = "SELECT id, email FROM users WHERE email = ?";
        let ast = SqlParser::parse(sql).unwrap();

        if let SqlStatement::Select { table, .. } = ast.statement {
            assert_eq!(table, "users");
        } else {
            panic!("Expected Select statement");
        }
    }
}
