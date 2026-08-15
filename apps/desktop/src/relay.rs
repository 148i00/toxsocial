//! Optional Relay integration for directory search and public outbox.
//! Default relay: https://toxsocial-relay.vcst.top

use serde::Serialize;

pub const DEFAULT_RELAY: &str = "https://toxsocial-relay.vcst.top";

#[derive(Debug, Clone, Serialize)]
pub struct RelayDirectoryEntry {
    pub name: String,
    pub pubkey: String,
    pub toxid: String,
    pub avatar: String,
    pub relay: String,
}

pub async fn search_directory(relay: &str, query: &str) -> Result<Vec<RelayDirectoryEntry>, String> {
    let url = url::Url::parse_with_params(
        &format!("{}/api/directory", relay.trim_end_matches('/')),
        &[("q", query)],
    )
    .map_err(|e| e.to_string())?;
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("relay request failed: {e}"))?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("relay response invalid: {e}"))?;
    if !status.is_success() {
        return Err(format!("relay error {}: {}", status, json));
    }
    let mut out = Vec::new();
    if let Some(items) = json["items"].as_array() {
        for item in items {
            out.push(RelayDirectoryEntry {
                name: item["name"].as_str().unwrap_or("").to_string(),
                pubkey: item["pubkey"].as_str().unwrap_or("").to_string(),
                toxid: item["toxid"].as_str().unwrap_or("").to_string(),
                avatar: item["avatar"].as_str().unwrap_or("").to_string(),
                relay: item["relay"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

pub async fn publish_post(relay: &str, pubkey: &str, id: &str, ts: i64, text: &str) -> Result<(), String> {
    let url = format!("{}/api/outbox", relay.trim_end_matches('/'));
    let body = serde_json::json!({
        "pubkey": pubkey,
        "id": id,
        "ts": ts,
        "text": text,
        "type": "post",
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("relay publish failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("relay publish error {}: {}", status, text));
    }
    Ok(())
}

pub async fn fetch_outbox(relay: &str, since: i64) -> Result<Vec<serde_json::Value>, String> {
    let url = url::Url::parse_with_params(
        &format!("{}/api/outbox", relay.trim_end_matches('/')),
        &[("since", since.to_string())],
    )
    .map_err(|e| e.to_string())?;
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("relay request failed: {e}"))?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("relay response invalid: {e}"))?;
    if !status.is_success() {
        return Err(format!("relay error {}: {}", status, json));
    }
    Ok(json["items"].as_array().cloned().unwrap_or_default())
}
