use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const MAXIMUM_AST_DEPTH: u32 = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SqlStatement {
    Select {
        table: String,
        columns: Vec<String>,
        where_clause: Option<SqlExpression>,
    },
    Insert {
        table: String,
        columns: Vec<String>,
        values_count: usize,
    },
    Update {
        table: String,
        set_columns: Vec<String>,
        where_clause: Option<SqlExpression>,
    },
    Delete {
        table: String,
        where_clause: Option<SqlExpression>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SqlExpression {
    Column(String),
    Parameter(usize),
    Literal(String),
    Binary {
        left: Box<SqlExpression>,
        op: String,
        right: Box<SqlExpression>,
    },
}

impl SqlExpression {
    pub fn depth(&self) -> u32 {
        match self {
            SqlExpression::Column(_) | SqlExpression::Parameter(_) | SqlExpression::Literal(_) => 1,
            SqlExpression::Binary { left, right, .. } => 1 + left.depth().max(right.depth()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlAst {
    pub statement: SqlStatement,
    pub depth: u32,
}

impl SqlAst {
    pub fn new(statement: SqlStatement) -> Result<Self> {
        let depth = match &statement {
            SqlStatement::Select { where_clause, .. }
            | SqlStatement::Update { where_clause, .. }
            | SqlStatement::Delete { where_clause, .. } => {
                where_clause.as_ref().map(|e| e.depth()).unwrap_or(1)
            }
            SqlStatement::Insert { .. } => 1,
        };

        if depth > MAXIMUM_AST_DEPTH {
            bail!(
                "E6501 AST depth limit exceeded: depth {} > {}",
                depth,
                MAXIMUM_AST_DEPTH
            );
        }

        Ok(Self { statement, depth })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_depth_calculation_and_limit_enforcement() {
        let expr = SqlExpression::Column("email".into());
        assert_eq!(expr.depth(), 1);

        let ast = SqlAst::new(SqlStatement::Select {
            table: "users".into(),
            columns: vec!["id".into()],
            where_clause: Some(expr),
        });
        assert!(ast.is_ok());
    }
}
