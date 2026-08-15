//! Media hosting helpers.
//!
//! Currently supports anonymous Imgur uploads. The user supplies an Imgur
//! Client ID in Settings; the returned URL is inserted into the post as a
//! Markdown image/video link.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

pub async fn upload_media(data_base64: &str, filename: &str, client_id: &str) -> Result<String, String> {
    let b64 = data_base64
        .trim()
        .strip_prefix("data:")
        .and_then(|s| s.split_once(',').map(|(_, b)| b))
        .unwrap_or(data_base64.trim());
    let bytes = BASE64
        .decode(b64)
        .map_err(|e| format!("invalid base64: {e}"))?;

    let client = reqwest::Client::new();
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")
        .map_err(|e| format!("invalid mime: {e}"))?;
    let form = reqwest::multipart::Form::new()
        .text("type", "file")
        .part("image", part);

    let resp = client
        .post("https://api.imgur.com/3/image")
        .header("Authorization", format!("Client-ID {client_id}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("upload request failed: {e}"))?;

    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("invalid upload response: {e}"))?;
    if !status.is_success() {
        return Err(format!("imgur upload failed ({}): {}", status, json));
    }
    json["data"]["link"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("imgur response missing link: {json}"))
}
