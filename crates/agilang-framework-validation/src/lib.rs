use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

pub struct Validator;

impl Validator {
    pub fn validate(
        data: &HashMap<String, Value>,
        rules: &HashMap<String, Vec<String>>,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        for (field, field_rules) in rules {
            let val_opt = data.get(field);

            for rule in field_rules {
                if rule == "required" {
                    if val_opt.is_none()
                        || val_opt.unwrap().is_null()
                        || (val_opt.unwrap().is_string()
                            && val_opt.unwrap().as_str().unwrap().trim().is_empty())
                    {
                        errors.push(ValidationError {
                            field: field.clone(),
                            message: format!("The {} field is required.", field),
                        });
                        break;
                    }
                } else if let Some(val) = val_opt {
                    if rule == "email" {
                        if let Some(s) = val.as_str() {
                            if !s.contains('@') || !s.contains('.') {
                                errors.push(ValidationError {
                                    field: field.clone(),
                                    message: format!(
                                        "The {} field must be a valid email address.",
                                        field
                                    ),
                                });
                            }
                        }
                    } else if rule.starts_with("min:") {
                        let min_val: usize = rule[4..].parse().unwrap_or(0);
                        if let Some(s) = val.as_str() {
                            if s.len() < min_val {
                                errors.push(ValidationError {
                                    field: field.clone(),
                                    message: format!(
                                        "The {} field must be at least {} characters.",
                                        field, min_val
                                    ),
                                });
                            }
                        }
                    } else if rule.starts_with("max:") {
                        let max_val: usize = rule[4..].parse().unwrap_or(usize::MAX);
                        if let Some(s) = val.as_str() {
                            if s.len() > max_val {
                                errors.push(ValidationError {
                                    field: field.clone(),
                                    message: format!(
                                        "The {} field must not exceed {} characters.",
                                        field, max_val
                                    ),
                                });
                            }
                        }
                    } else if rule == "confirmed" {
                        let confirm_field = format!("{}_confirmation", field);
                        let confirm_val = data.get(&confirm_field).and_then(|v| v.as_str());
                        if val.as_str() != confirm_val {
                            errors.push(ValidationError {
                                field: field.clone(),
                                message: format!("The {} confirmation does not match.", field),
                            });
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validation_success() {
        let mut data = HashMap::new();
        data.insert("email".into(), json!("user@example.com"));
        data.insert("password".into(), json!("password123"));
        data.insert("password_confirmation".into(), json!("password123"));

        let mut rules = HashMap::new();
        rules.insert("email".into(), vec!["required".into(), "email".into()]);
        rules.insert(
            "password".into(),
            vec!["required".into(), "min:8".into(), "confirmed".into()],
        );

        assert!(Validator::validate(&data, &rules).is_ok());
    }

    #[test]
    fn test_validation_failures() {
        let mut data = HashMap::new();
        data.insert("email".into(), json!("invalid_email"));
        data.insert("password".into(), json!("short"));

        let mut rules = HashMap::new();
        rules.insert("email".into(), vec!["required".into(), "email".into()]);
        rules.insert("password".into(), vec!["required".into(), "min:8".into()]);

        let res = Validator::validate(&data, &rules);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert_eq!(errs.len(), 2);
    }
}
