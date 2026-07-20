use agilang_database_parser::SqlParser;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MySqlCommandPacket {
    ComQuery(String),
    ComStmtPrepare(String),
    ComStmtExecute {
        statement_id: u32,
        params_count: usize,
    },
    ComStmtClose(u32),
}

pub struct MySqlGateway {
    pub port: u16,
    pub active_connections: usize,
}

impl MySqlGateway {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            active_connections: 0,
        }
    }

    pub fn handle_packet(&mut self, packet: MySqlCommandPacket) -> Result<String> {
        match packet {
            MySqlCommandPacket::ComQuery(sql) => {
                let ast = SqlParser::parse(&sql)?;
                Ok(format!(
                    "OK: Query executed via typed AST ({:?})",
                    ast.statement
                ))
            }
            MySqlCommandPacket::ComStmtPrepare(sql) => {
                let _ast = SqlParser::parse(&sql)?;
                Ok("OK: Statement prepared ID 1".to_string())
            }
            MySqlCommandPacket::ComStmtExecute {
                statement_id,
                params_count,
            } => {
                if statement_id == 0 {
                    bail!("E6608 MySQL gateway policy denial: invalid statement ID");
                }
                Ok(format!(
                    "OK: Statement {} executed with {} params",
                    statement_id, params_count
                ))
            }
            MySqlCommandPacket::ComStmtClose(id) => Ok(format!("OK: Statement {} closed", id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_gateway_command_packet_handling() {
        let mut gateway = MySqlGateway::new(3306);
        let res = gateway.handle_packet(MySqlCommandPacket::ComQuery(
            "SELECT id, email FROM users".into(),
        ));
        assert!(res.is_ok());

        let err = gateway.handle_packet(MySqlCommandPacket::ComStmtExecute {
            statement_id: 0,
            params_count: 1,
        });
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6608"));
    }
}
