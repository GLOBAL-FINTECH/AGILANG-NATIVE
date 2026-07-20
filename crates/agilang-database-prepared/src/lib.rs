use agilang_database_driver::DatabaseValue;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDescriptor {
    pub position: u16,
    pub expected_type: String,
    pub nullable: bool,
    pub maximum_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedStatement {
    pub statement_id: String,
    pub sql: String,
    pub descriptors: Vec<ParameterDescriptor>,
}

impl PreparedStatement {
    pub fn new(
        id: impl Into<String>,
        sql: impl Into<String>,
        descriptors: Vec<ParameterDescriptor>,
    ) -> Self {
        Self {
            statement_id: id.into(),
            sql: sql.into(),
            descriptors,
        }
    }

    pub fn validate_parameters(&self, params: &[DatabaseValue]) -> Result<()> {
        if params.len() != self.descriptors.len() {
            bail!(
                "E6505 Prepared parameter mismatch: expected {} arguments, got {}",
                self.descriptors.len(),
                params.len()
            );
        }

        for (desc, param) in self.descriptors.iter().zip(params.iter()) {
            if let DatabaseValue::Text(val) = param {
                if let Some(max_len) = desc.maximum_length {
                    if val.len() as u64 > max_len {
                        bail!(
                            "E6506 Parameter length exceeded for position {}: {} > {}",
                            desc.position,
                            val.len(),
                            max_len
                        );
                    }
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
    fn test_prepared_statement_parameter_length_and_count_validation() {
        let stmt = PreparedStatement::new(
            "stmt_1",
            "SELECT * FROM users WHERE email = ?",
            vec![ParameterDescriptor {
                position: 1,
                expected_type: "Text".into(),
                nullable: false,
                maximum_length: Some(10),
            }],
        );

        let valid_params = vec![DatabaseValue::Text("a@b.com".into())];
        assert!(stmt.validate_parameters(&valid_params).is_ok());

        let invalid_len = vec![DatabaseValue::Text(
            "very_long_email_address@example.com".into(),
        )];
        let err = stmt.validate_parameters(&invalid_len);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6506"));
    }
}
