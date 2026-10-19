use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DapRequest {
    pub seq: u64,
    #[serde(rename="type")]
    pub kind: String,
    pub command: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DapResponse {
    pub seq: u64,
    #[serde(rename="type")]
    pub kind: String,
    pub request_seq: u64,
    pub success: bool,
    pub command: String,
    pub body: serde_json::Value,
}
pub fn parse_message(input: &str) -> Result<DapRequest> {
    serde_json::from_str(input).context("invalid Debug Adapter Protocol request")
}
pub fn initialize_response(request: &DapRequest) -> DapResponse {
    DapResponse{seq:request.seq+1,kind:"response".into(),request_seq:request.seq,success:true,command:request.command.clone(),body:serde_json::json!({
        "supportsConfigurationDoneRequest": true,
        "supportsEvaluateForHovers": true,
        "supportsSetVariable": true,
        "supportsTerminateRequest": true
    })}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_initialize(){ let r=parse_message(r#"{"seq":1,"type":"request","command":"initialize","arguments":{}}"#).unwrap(); assert_eq!(r.command,"initialize"); assert!(initialize_response(&r).success); }
}
