use agilang_database_sql::{SqlAst, SqlStatement};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const MAXIMUM_JOIN_COUNT: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogicalPlanNode {
    TableScan { table: String },
    IndexScan { table: String, index_name: String },
    Filter { predicate: String },
    Projection { columns: Vec<String> },
    Insert { table: String, row_count: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicalPlan {
    pub nodes: Vec<LogicalPlanNode>,
    pub estimated_cost: u64,
    pub join_count: usize,
}

pub struct LogicalPlanner;

impl LogicalPlanner {
    pub fn build_plan(ast: &SqlAst) -> Result<LogicalPlan> {
        let mut nodes = Vec::new();
        let mut estimated_cost = 10;
        let join_count = 0;

        if join_count > MAXIMUM_JOIN_COUNT {
            bail!(
                "E6503 Join count limit exceeded: {} > {}",
                join_count,
                MAXIMUM_JOIN_COUNT
            );
        }

        match &ast.statement {
            SqlStatement::Select { table, columns, .. } => {
                nodes.push(LogicalPlanNode::IndexScan {
                    table: table.clone(),
                    index_name: format!("{}_pk", table),
                });
                nodes.push(LogicalPlanNode::Projection {
                    columns: columns.clone(),
                });
                estimated_cost = 15;
            }
            SqlStatement::Insert {
                table,
                values_count,
                ..
            } => {
                nodes.push(LogicalPlanNode::Insert {
                    table: table.clone(),
                    row_count: *values_count,
                });
                estimated_cost = 5;
            }
            _ => {
                nodes.push(LogicalPlanNode::TableScan {
                    table: "table".to_string(),
                });
            }
        }

        Ok(LogicalPlan {
            nodes,
            estimated_cost,
            join_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logical_planner_select_plan() {
        let ast = SqlAst::new(SqlStatement::Select {
            table: "users".into(),
            columns: vec!["id".into()],
            where_clause: None,
        })
        .unwrap();

        let plan = LogicalPlanner::build_plan(&ast).unwrap();
        assert_eq!(plan.nodes.len(), 2);
        assert!(plan.estimated_cost <= 100);
    }
}
