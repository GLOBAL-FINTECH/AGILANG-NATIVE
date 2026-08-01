//! Opt-in integration boundary for the live SIBAQ RPC.

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use std::time::Instant;

    async fn rpc(client: &reqwest::Client, method: &str, params: Value) -> Value {
        client
            .post("https://rpc.sibaq.us")
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .send()
            .await
            .expect("SIBAQ RPC must be reachable")
            .error_for_status()
            .expect("SIBAQ RPC must return success")
            .json::<Value>()
            .await
            .expect("SIBAQ RPC must return JSON")
    }

    #[tokio::test]
    #[ignore = "requires live SIBAQ RPC access"]
    async fn verifies_sibaq_chain_identity() {
        if std::env::var("SIBAQ_LIVE_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped: set SIBAQ_LIVE_TESTS=1 to enable live RPC verification");
            return;
        }
        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("TLS client configuration must be valid");
        let started = Instant::now();
        assert_eq!(
            rpc(&client, "eth_chainId", json!([])).await["result"],
            "0x7c6"
        );
        let height = rpc(&client, "eth_blockNumber", json!([])).await;
        let quantity = height["result"]
            .as_str()
            .expect("block number must be a quantity");
        assert!(u64::from_str_radix(quantity.trim_start_matches("0x"), 16).is_ok());
        let block = rpc(&client, "eth_getBlockByNumber", json!(["latest", false])).await;
        assert!(block["result"]["hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("0x")));
        eprintln!(
            "SIBAQ verification latency: {} ms",
            started.elapsed().as_millis()
        );
    }
}
