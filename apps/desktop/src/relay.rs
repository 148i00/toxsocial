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

#[derive(Debug, Clone, Serialize)]
pub struct RelayChannel {
    pub name: String,
    pub desc: String,
    pub host_toxid: String,
    pub channel_id: String,
}

pub async fn list_channels(relay: &str) -> Result<Vec<RelayChannel>, String> {
    let url = format!("{}/api/channels", relay.trim_end_matches('/'));
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
            out.push(RelayChannel {
                name: item["name"].as_str().unwrap_or("").to_string(),
                desc: item["desc"].as_str().unwrap_or("").to_string(),
                host_toxid: item["hostToxid"].as_str().unwrap_or("").to_string(),
                channel_id: item["channelId"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

pub async fn register_channel(
    relay: &str,
    name: &str,
    desc: &str,
    host_toxid: &str,
    channel_id: &str,
) -> Result<(), String> {
    let url = format!("{}/api/channels", relay.trim_end_matches('/'));
    let body = serde_json::json!({
        "name": name,
        "desc": desc,
        "hostToxid": host_toxid,
        "channelId": channel_id,
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("relay register channel failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("relay register channel error {}: {}", status, text));
    }
    Ok(())
}

pub async fn register_profile(
    relay: &str,
    name: &str,
    pubkey: &str,
    toxid: &str,
    avatar: &str,
) -> Result<(), String> {
    let url = format!("{}/api/directory", relay.trim_end_matches('/'));
    let body = serde_json::json!({
        "name": name,
        "pubkey": pubkey,
        "toxid": toxid,
        "avatar": avatar,
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("relay register profile failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("relay register profile error {}: {}", status, text));
    }
    Ok(())
}

pub async fn delete_channel(relay: &str, channel_id: &str, host_toxid: &str) -> Result<(), String> {
    let url = format!("{}/api/channels/delete", relay.trim_end_matches('/'));
    let body = serde_json::json!({
        "channelId": channel_id,
        "hostToxid": host_toxid,
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("relay delete channel failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("relay delete channel error {}: {}", status, text));
    }
    Ok(())
}
