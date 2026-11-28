use axum::http::StatusCode;
use serde_json::{json, Value};
use std::fmt;

#[derive(Debug, Clone)]
pub struct ServiceError {
    pub code: &'static str,
    pub message: String,
    pub status: StatusCode,
    pub details: Value,
}
impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ServiceError {}
impl ServiceError {
    pub fn new(code: &'static str, message: impl Into<String>, status: StatusCode) -> Self {
        Self {
            code,
            message: message.into(),
            status,
            details: json!({}),
        }
    }
    pub fn details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
    pub fn value(&self) -> Value {
        json!({"error":self.message,"code":self.code,"details":self.details})
    }
}
pub fn describe(error: &anyhow::Error) -> ServiceError {
    if let Some(value) = error.downcast_ref::<ServiceError>() {
        return value.clone();
    }
    ServiceError::new(
        "operation_failed",
        crate::storage::error_message(error),
        StatusCode::BAD_REQUEST,
    )
}
