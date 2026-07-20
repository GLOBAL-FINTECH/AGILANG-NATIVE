use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableConstraint {
    pub table: String,
    pub allow_select: bool,
    pub allow_insert: bool,
    pub allow_update: bool,
    pub allow_delete: bool,
    pub blocked_columns: Vec<String>,
}

pub struct PolicyEngine {
    pub constraints: Vec<TableConstraint>,
}

impl PolicyEngine {
    pub fn new(constraints: Vec<TableConstraint>) -> Self {
        Self { constraints }
    }

    pub fn authorize_update(&self, table: &str, columns: &[String]) -> Result<()> {
        if let Some(rule) = self.constraints.iter().find(|c| c.table == table) {
            if !rule.allow_update {
                bail!(
                    "E6507 Access denied: updates disabled for table `{}`",
                    table
                );
            }
            for col in columns {
                if rule.blocked_columns.contains(col) {
                    bail!(
                        "E6507 Access denied: column `{}` modification restricted",
                        col
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_engine_column_mutation_restriction() {
        let engine = PolicyEngine::new(vec![TableConstraint {
            table: "users".into(),
            allow_select: true,
            allow_insert: true,
            allow_update: true,
            allow_delete: false,
            blocked_columns: vec!["role".into(), "password_hash".into()],
        }]);

        assert!(engine.authorize_update("users", &["name".into()]).is_ok());

        let err = engine.authorize_update("users", &["role".into()]);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6507"));
    }
}
