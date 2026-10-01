use serde::{Deserialize, Serialize};

const FEED_URL: &str =
    "https://api.github.com/repos/Hakkaiz1/obsy-launcher-hakkaiz/contents/public/announcements.json";

fn announcement_feed_url(cache_buster: u128) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(FEED_URL)
        .map_err(|error| format!("Announcements feed URL is invalid: {error}"))?;
    url.query_pairs_mut()
        .append_pair("ref", "main")
        .append_pair("refresh", &cache_buster.to_string());
    Ok(url)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementFeed {
    schema_version: u32,
    generated_at: String,
    pub updates: AnnouncementChannel,
    pub patch_notes: AnnouncementChannel,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnnouncementChannel {
    cursor: String,
    pub messages: Vec<Announcement>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Announcement {
    pub id: String,
    pub content: String,
    pub author: String,
    pub timestamp: String,
    pub attachments: Vec<String>,
}

fn is_snowflake(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn validate_channel(channel: &AnnouncementChannel) -> Result<(), String> {
    if (!channel.cursor.is_empty() && !is_snowflake(&channel.cursor)) || channel.messages.len() > 30
    {
        return Err("Announcements feed contains an invalid channel".to_string());
    }

    for message in &channel.messages {
        if !is_snowflake(&message.id)
            || message.author.trim().is_empty()
            || message.timestamp.trim().is_empty()
        {
            return Err("Announcements feed contains an invalid message".to_string());
        }
        for attachment in &message.attachments {
            let url = reqwest::Url::parse(attachment)
                .map_err(|_| "Announcements feed contains an invalid attachment URL")?;
            if url.scheme() != "https" || url.host_str().is_none() {
                return Err("Announcements feed attachments must use HTTPS".to_string());
            }
        }
    }
    Ok(())
}

pub fn parse_feed(json: &str) -> Result<AnnouncementFeed, String> {
    let feed: AnnouncementFeed = serde_json::from_str(json)
        .map_err(|_| "Announcements feed has an invalid format".to_string())?;
    if feed.schema_version != 1 || feed.generated_at.trim().is_empty() {
        return Err("Announcements feed has an unsupported schema".to_string());
    }
    validate_channel(&feed.updates)?;
    validate_channel(&feed.patch_notes)?;
    Ok(feed)
}

#[tauri::command]
pub async fn fetch_announcements() -> Result<AnnouncementFeed, String> {
    let cache_buster = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("Could not determine current time: {error}"))?
        .as_millis();
    let response = reqwest::Client::new()
        .get(announcement_feed_url(cache_buster)?)
        .header(reqwest::header::ACCEPT, "application/vnd.github.raw+json")
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .await
        .map_err(|_| "Could not fetch announcements feed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Announcements feed request failed with HTTP {}",
            response.status()
        ));
    }
    let body = response
        .text()
        .await
        .map_err(|_| "Could not read announcements feed".to_string())?;
    parse_feed(&body)
}

#[cfg(test)]
mod tests {
    use super::{announcement_feed_url, parse_feed};

    const VALID_FEED: &str = r#"{
        "schemaVersion": 1,
        "generatedAt": "2026-10-01T12:00:00.000Z",
        "updates": {
            "cursor": "123456789012345678",
            "messages": [{
                "id": "123456789012345678",
                "content": "Server update",
                "author": "Admin",
                "timestamp": "2026-10-01T11:00:00.000Z",
                "attachments": ["https://cdn.example.com/update.zip"]
            }]
        },
        "patchNotes": { "cursor": "", "messages": [] }
    }"#;

    #[test]
    fn announcement_feed_url_bypasses_cdn_cache() {
        let first = announcement_feed_url(1).unwrap();
        let second = announcement_feed_url(2).unwrap();

        assert_eq!(first.host_str(), Some("api.github.com"));
        assert_eq!(
            first.path(),
            "/repos/Hakkaiz1/obsy-launcher-hakkaiz/contents/public/announcements.json"
        );
        assert_ne!(first, second);
        assert_eq!(
            first
                .query_pairs()
                .find(|(key, _)| key == "refresh")
                .map(|(_, value)| value.into_owned()),
            Some("1".to_string())
        );
    }

    #[test]
    fn parse_feed_accepts_valid_feed() {
        let feed = parse_feed(VALID_FEED).unwrap();
        assert_eq!(feed.schema_version, 1);
        assert_eq!(feed.updates.messages[0].id, "123456789012345678");
        assert_eq!(feed.patch_notes.messages.len(), 0);
    }

    #[test]
    fn parse_feed_rejects_unsupported_schema_and_missing_fields() {
        let unsupported = VALID_FEED.replace("\"schemaVersion\": 1", "\"schemaVersion\": 2");
        let mut missing_channel: serde_json::Value = serde_json::from_str(VALID_FEED).unwrap();
        missing_channel
            .as_object_mut()
            .unwrap()
            .remove("patchNotes");
        assert!(parse_feed(&unsupported).is_err());
        assert!(parse_feed(&missing_channel.to_string()).is_err());
    }

    #[test]
    fn parse_feed_rejects_non_https_attachments() {
        let invalid_url = VALID_FEED.replace(
            "https://cdn.example.com/update.zip",
            "http://cdn.example.com/update.zip",
        );
        assert!(parse_feed(&invalid_url).is_err());
    }

    #[test]
    fn parse_feed_rejects_more_than_thirty_messages_per_channel() {
        let mut oversized: serde_json::Value = serde_json::from_str(VALID_FEED).unwrap();
        let message = oversized["updates"]["messages"][0].clone();
        let messages = oversized["updates"]["messages"].as_array_mut().unwrap();
        messages.resize(31, message);
        assert!(parse_feed(&oversized.to_string()).is_err());
    }
}
