use async_trait::async_trait;
use nexus_domain::*;
use nexus_error::AgentError;
use reqwest::Client;
use serde_json::Value;
use tracing::debug;

const BASE_URL: &str = "https://discord.com/api/v10";

pub struct DiscordConfig {
    pub bot_token: String,
}

pub struct DiscordAdapter {
    auth: String,
    client: Client,
}

impl DiscordAdapter {
    pub fn new(config: DiscordConfig) -> Self {
        let auth = format!("Bot {}", config.bot_token);
        let client = Client::new();
        Self { auth, client }
    }

    fn validate_id(id: &str, label: &str) -> Result<(), AgentError> {
        if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains('\0') {
            return Err(AgentError::invalid_input(format!(
                "invalid {label}: must not be empty or contain path separators"
            )));
        }
        Ok(())
    }

    async fn api_get(&self, path: &str) -> Result<serde_json::Value, AgentError> {
        let url = format!("{BASE_URL}{path}");
        debug!(url, "discord GET");
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.auth)
            .send()
            .await
            .map_err(|e| AgentError::network(format!("discord request failed: {e}")))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AgentError::api(format!("discord response parse failed: {e}")))?;

        if !status.is_success() {
            let msg = body["message"].as_str().unwrap_or("unknown error");
            return Err(parse_discord_error(status.as_u16(), msg));
        }

        Ok(body)
    }

    async fn api_post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        let url = format!("{BASE_URL}{path}");
        debug!(url, "discord POST");
        let resp = self
            .client
            .post(&url)
            .header("Authorization", &self.auth)
            .json(body)
            .send()
            .await
            .map_err(|e| AgentError::network(format!("discord request failed: {e}")))?;

        let status = resp.status();
        let response_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AgentError::api(format!("discord response parse failed: {e}")))?;

        if !status.is_success() {
            let msg = response_body["message"].as_str().unwrap_or("unknown error");
            return Err(parse_discord_error(status.as_u16(), msg));
        }

        Ok(response_body)
    }

    async fn api_put_empty(&self, path: &str) -> Result<(), AgentError> {
        let url = format!("{BASE_URL}{path}");
        debug!(url, "discord PUT");
        let resp = self
            .client
            .put(&url)
            .header("Authorization", &self.auth)
            .header("Content-Length", "0")
            .send()
            .await
            .map_err(|e| AgentError::network(format!("discord request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or(serde_json::json!({"message": "unknown error"}));
            let msg = body["message"].as_str().unwrap_or("unknown error");
            return Err(parse_discord_error(status.as_u16(), msg));
        }

        Ok(())
    }

    async fn api_delete(&self, path: &str) -> Result<(), AgentError> {
        let url = format!("{BASE_URL}{path}");
        debug!(url, "discord DELETE");
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", &self.auth)
            .send()
            .await
            .map_err(|e| AgentError::network(format!("discord request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or(serde_json::json!({"message": "unknown error"}));
            let msg = body["message"].as_str().unwrap_or("unknown error");
            return Err(parse_discord_error(status.as_u16(), msg));
        }

        Ok(())
    }
}

#[async_trait]
impl MessagingPort for DiscordAdapter {
    fn platform(&self) -> Platform {
        Platform::Discord
    }

    async fn get_profile(&self) -> Result<Profile, AgentError> {
        let user = self.api_get("/users/@me").await?;

        Ok(Profile {
            platform: Platform::Discord,
            id: user["id"].as_str().unwrap_or("").to_string(),
            name: user["global_name"]
                .as_str()
                .or_else(|| user["username"].as_str())
                .unwrap_or("Unknown")
                .to_string(),
            username: user["username"].as_str().map(|s| s.to_string()),
            email: user["email"].as_str().map(|s| s.to_string()),
            phone: None,
        })
    }

    async fn list_channels(&self, limit: usize) -> Result<Vec<Channel>, AgentError> {
        let guilds = self.api_get("/users/@me/guilds").await?;
        let guild_arr = guilds.as_array().ok_or_else(|| {
            AgentError::api("discord: expected guilds array".to_string())
        })?;

        let mut channels = Vec::new();
        for guild in guild_arr.iter().take(5) {
            let guild_id = guild["id"].as_str().unwrap_or("");
            let guild_name = guild["name"].as_str().unwrap_or("Unknown");

            let guild_channels = self
                .api_get(&format!("/guilds/{guild_id}/channels"))
                .await?;

            if let Some(arr) = guild_channels.as_array() {
                for ch in arr {
                    let ch_type = ch["type"].as_u64().unwrap_or(0);
                    if ch_type == 0 || ch_type == 2 || ch_type == 5 {
                        channels.push(parse_discord_channel(ch, guild_name));
                    }
                }
            }

            if channels.len() >= limit {
                channels.truncate(limit);
                break;
            }
        }

        Ok(channels)
    }

    async fn read_messages(
        &self,
        channel: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        Self::validate_id(channel, "channel")?;
        let mut path = format!("/channels/{channel}/messages?limit={limit}");
        if let Some(c) = cursor {
            if let Some(before_id) = c.strip_prefix("dc:") {
                path.push_str(&format!("&before={before_id}"));
            }
        }

        let resp = self.api_get(&path).await?;
        let messages: Vec<Message> = resp
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .map(|m| parse_discord_message(m, channel))
            .collect();

        let has_more = messages.len() == limit;
        let next_cursor = messages
            .last()
            .map(|m| format!("dc:{}", m.id));

        Ok(Paginated {
            items: messages,
            has_more,
            next_cursor,
        })
    }

    async fn send_message(
        &self,
        channel: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<Message, AgentError> {
        Self::validate_id(channel, "channel")?;
        let mut body = serde_json::json!({
            "content": text,
        });

        if let Some(msg_id) = reply_to {
            body["message_reference"] = serde_json::json!({
                "message_id": msg_id,
            });
        }

        let resp = self
            .api_post(&format!("/channels/{channel}/messages"), &body)
            .await?;

        Ok(parse_discord_message(&resp, channel))
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        let guilds = self.api_get("/users/@me/guilds").await?;
        let first_guild_id = guilds
            .as_array()
            .and_then(|a| a.first())
            .and_then(|g| g["id"].as_str())
            .ok_or_else(|| AgentError::not_found("no guilds found for search (searches first guild only)"))?;

        let offset: usize = cursor
            .and_then(|c| c.strip_prefix("dc:"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let path = format!(
            "/guilds/{first_guild_id}/messages/search?content={}&limit={}&offset={}",
            urlencoding(query),
            limit,
            offset,
        );

        let resp = self.api_get(&path).await?;
        let messages: Vec<Message> = resp["messages"]
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .filter_map(|arr| arr.as_array().and_then(|a| a.first()))
            .map(|m| {
                let ch_id = m["channel_id"].as_str().unwrap_or("");
                parse_discord_message(m, ch_id)
            })
            .collect();

        let total = resp["total_results"].as_u64().unwrap_or(0) as usize;
        let next_offset = offset + messages.len();
        let has_more = next_offset < total;

        let next_cursor = if has_more {
            Some(format!("dc:{next_offset}"))
        } else {
            None
        };

        Ok(Paginated {
            items: messages,
            has_more,
            next_cursor,
        })
    }
}

#[async_trait]
impl DiscordExt for DiscordAdapter {
    async fn list_guilds(&self) -> Result<Vec<Channel>, AgentError> {
        let guilds = self.api_get("/users/@me/guilds").await?;
        let channels: Vec<Channel> = guilds
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .map(|g| Channel {
                id: g["id"].as_str().unwrap_or("").to_string(),
                platform: Platform::Discord,
                name: g["name"].as_str().unwrap_or("Unknown").to_string(),
                channel_type: ChannelType::Other("guild".to_string()),
                unread_count: 0,
                description: None,
                member_count: g["approximate_member_count"].as_i64().map(|n| n as i32),
                last_message_date: None,
            })
            .collect();

        Ok(channels)
    }

    async fn list_guild_channels(
        &self,
        guild_id: &str,
    ) -> Result<Vec<Channel>, AgentError> {
        Self::validate_id(guild_id, "guild_id")?;
        let resp = self
            .api_get(&format!("/guilds/{guild_id}/channels"))
            .await?;

        let channels: Vec<Channel> = resp
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .map(|ch| parse_discord_channel(ch, ""))
            .collect();

        Ok(channels)
    }

    async fn create_thread(
        &self,
        channel: &str,
        name: &str,
        msg_id: Option<&str>,
    ) -> Result<Channel, AgentError> {
        Self::validate_id(channel, "channel")?;
        if let Some(mid) = msg_id {
            Self::validate_id(mid, "message_id")?;
        }
        let resp = if let Some(mid) = msg_id {
            self.api_post(
                &format!("/channels/{channel}/messages/{mid}/threads"),
                &serde_json::json!({ "name": name }),
            )
            .await?
        } else {
            self.api_post(
                &format!("/channels/{channel}/threads"),
                &serde_json::json!({
                    "name": name,
                    "type": 11,
                    "auto_archive_duration": 1440,
                }),
            )
            .await?
        };

        Ok(parse_discord_channel(&resp, ""))
    }

    async fn add_reaction(
        &self,
        channel: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<(), AgentError> {
        Self::validate_id(channel, "channel")?;
        Self::validate_id(msg_id, "message_id")?;
        let encoded = urlencoding(emoji);
        self.api_put_empty(&format!(
            "/channels/{channel}/messages/{msg_id}/reactions/{encoded}/@me"
        ))
        .await
    }

    async fn remove_reaction(
        &self,
        channel: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<(), AgentError> {
        Self::validate_id(channel, "channel")?;
        Self::validate_id(msg_id, "message_id")?;
        let encoded = urlencoding(emoji);
        self.api_delete(&format!(
            "/channels/{channel}/messages/{msg_id}/reactions/{encoded}/@me"
        ))
        .await
    }

    async fn pin_message(&self, channel: &str, msg_id: &str) -> Result<(), AgentError> {
        Self::validate_id(channel, "channel")?;
        Self::validate_id(msg_id, "message_id")?;
        self.api_put_empty(&format!("/channels/{channel}/pins/{msg_id}"))
            .await
    }
}

fn parse_discord_channel(ch: &serde_json::Value, guild_name: &str) -> Channel {
    let ch_type = ch["type"].as_u64().unwrap_or(0);
    let channel_type = match ch_type {
        0 => ChannelType::Other("text".to_string()),
        1 => ChannelType::Private,
        2 => ChannelType::Other("voice".to_string()),
        3 => ChannelType::Group,
        4 => ChannelType::Other("category".to_string()),
        5 => ChannelType::Broadcast,
        10..=12 => ChannelType::Thread,
        13 => ChannelType::Other("stage".to_string()),
        15 => ChannelType::Other("forum".to_string()),
        _ => ChannelType::Other(format!("type:{ch_type}")),
    };

    let name = ch["name"].as_str().unwrap_or("unknown").to_string();
    let display_name = if guild_name.is_empty() {
        name
    } else {
        format!("{guild_name}/#{name}")
    };

    Channel {
        id: ch["id"].as_str().unwrap_or("").to_string(),
        platform: Platform::Discord,
        name: display_name,
        channel_type,
        unread_count: 0,
        description: ch["topic"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        member_count: ch["member_count"].as_i64().map(|n| n as i32),
        last_message_date: None,
    }
}

fn parse_discord_message(m: &serde_json::Value, channel: &str) -> Message {
    let timestamp = m["timestamp"]
        .as_str()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let sender = m["author"]["global_name"]
        .as_str()
        .or_else(|| m["author"]["username"].as_str())
        .unwrap_or("unknown")
        .to_string();

    let has_attachment = m["attachments"]
        .as_array()
        .is_some_and(|a| !a.is_empty());

    let reply_to = m["message_reference"]["message_id"]
        .as_str()
        .map(|s| s.to_string());

    let mut meta = MessageMeta::default();
    if let Some(true) = m["pinned"].as_bool() {
        meta.is_pinned = Some(true);
    }

    Message {
        id: m["id"].as_str().unwrap_or("").to_string(),
        platform: Platform::Discord,
        channel_id: channel.to_string(),
        sender,
        text: m["content"].as_str().unwrap_or("").to_string(),
        timestamp,
        has_attachment,
        reply_to,
        meta,
    }
}

fn parse_discord_error(status: u16, msg: &str) -> AgentError {
    if status == 401 || status == 403 {
        return AgentError::auth(format!("discord auth failed ({status}): {msg}"));
    }
    AgentError::api(format!("discord api error ({status}): {msg}"))
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
