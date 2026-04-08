use std::time::Duration;

use async_trait::async_trait;
use nexus_domain::*;
use nexus_error::AgentError;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, warn};

const BASE_URL: &str = "https://slack.com/api";

pub struct SlackConfig {
    pub bot_token: String,
}

pub struct SlackAdapter {
    config: SlackConfig,
    client: Client,
}

impl SlackAdapter {
    pub fn new(config: SlackConfig) -> Self {
        let client = Client::new();
        Self { config, client }
    }

    async fn api_post(&self, method: &str, body: &Value) -> Result<Value, AgentError> {
        let url = format!("{BASE_URL}/{method}");
        debug!(url, "slack POST");

        for attempt in 0..3 {
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.config.bot_token)
                .json(body)
                .send()
                .await
                .map_err(|e| AgentError::network(format!("slack request failed: {e}")))?;

            if resp.status().as_u16() == 429 {
                let delay = parse_slack_retry_after(&resp);
                warn!(attempt, delay_ms = delay.as_millis() as u64, "slack rate limited, retrying");
                tokio::time::sleep(delay).await;
                continue;
            }

            return parse_slack_response(resp).await;
        }

        Err(AgentError::api("slack rate limited after 3 retries"))
    }

    async fn api_get(&self, method: &str, params: &[(&str, &str)]) -> Result<Value, AgentError> {
        let url = format!("{BASE_URL}/{method}");
        debug!(url, "slack GET");

        for attempt in 0..3 {
            let resp = self
                .client
                .get(&url)
                .bearer_auth(&self.config.bot_token)
                .query(params)
                .send()
                .await
                .map_err(|e| AgentError::network(format!("slack request failed: {e}")))?;

            if resp.status().as_u16() == 429 {
                let delay = parse_slack_retry_after(&resp);
                warn!(attempt, delay_ms = delay.as_millis() as u64, "slack rate limited, retrying");
                tokio::time::sleep(delay).await;
                continue;
            }

            return parse_slack_response(resp).await;
        }

        Err(AgentError::api("slack rate limited after 3 retries"))
    }
}

#[async_trait]
impl MessagingPort for SlackAdapter {
    fn platform(&self) -> Platform {
        Platform::Slack
    }

    async fn get_profile(&self) -> Result<Profile, AgentError> {
        let resp = self.api_post("auth.test", &serde_json::json!({})).await?;

        Ok(Profile {
            platform: Platform::Slack,
            id: resp["user_id"].as_str().unwrap_or("").to_string(),
            name: resp["user"].as_str().unwrap_or("Unknown").to_string(),
            username: None,
            email: None,
            phone: None,
        })
    }

    async fn list_channels(&self, limit: usize) -> Result<Vec<Channel>, AgentError> {
        let limit_s = limit.to_string();
        let resp = self
            .api_get(
                "conversations.list",
                &[
                    ("types", "public_channel,private_channel,im,mpim"),
                    ("limit", &limit_s),
                    ("exclude_archived", "true"),
                ],
            )
            .await?;

        let channels = resp["channels"]
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .map(parse_slack_channel)
            .collect();

        Ok(channels)
    }

    async fn read_messages(
        &self,
        channel: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        let limit_s = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![("channel", channel), ("limit", &limit_s)];
        let cursor_stripped;
        if let Some(c) = cursor.and_then(|c| c.strip_prefix("sl:")) {
            cursor_stripped = c.to_string();
            params.push(("cursor", &cursor_stripped));
        }

        let resp = self.api_get("conversations.history", &params).await?;

        let messages: Vec<Message> = resp["messages"]
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .map(|m| parse_slack_message(m, channel))
            .collect();

        let has_more = resp["has_more"].as_bool().unwrap_or(false);
        let next_cursor = resp["response_metadata"]["next_cursor"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| format!("sl:{s}"));

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
        let mut body = serde_json::json!({
            "channel": channel,
            "text": text,
        });

        if let Some(thread_ts) = reply_to {
            body["thread_ts"] = Value::String(thread_ts.to_string());
        }

        let resp = self.api_post("chat.postMessage", &body).await?;
        Ok(parse_slack_message(&resp["message"], channel))
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        let limit_s = limit.to_string();
        let page = cursor
            .and_then(|c| c.strip_prefix("sl:"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);
        let page_s = page.to_string();

        let resp = self
            .api_get(
                "search.messages",
                &[("query", query), ("count", &limit_s), ("page", &page_s)],
            )
            .await?;

        let messages: Vec<Message> = resp["messages"]["matches"]
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .map(|m| {
                let ch = m["channel"]["id"].as_str().unwrap_or("");
                parse_slack_message(m, ch)
            })
            .collect();

        let total = resp["messages"]["total"].as_u64().unwrap_or(0) as usize;
        let seen = (page - 1) * limit + messages.len();
        let has_more = seen < total;
        let next_cursor = if has_more {
            Some(format!("sl:{}", page + 1))
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
impl SlackExt for SlackAdapter {
    async fn set_status(&self, text: &str, emoji: &str) -> Result<(), AgentError> {
        self.api_post(
            "users.profile.set",
            &serde_json::json!({
                "profile": {
                    "status_text": text,
                    "status_emoji": emoji,
                }
            }),
        )
        .await?;
        Ok(())
    }

    async fn create_channel(
        &self,
        name: &str,
        is_private: bool,
    ) -> Result<Channel, AgentError> {
        let resp = self
            .api_post(
                "conversations.create",
                &serde_json::json!({
                    "name": name,
                    "is_private": is_private,
                }),
            )
            .await?;
        Ok(parse_slack_channel(&resp["channel"]))
    }

    async fn invite_to_channel(
        &self,
        channel: &str,
        user_id: &str,
    ) -> Result<(), AgentError> {
        self.api_post(
            "conversations.invite",
            &serde_json::json!({
                "channel": channel,
                "users": user_id,
            }),
        )
        .await?;
        Ok(())
    }

    async fn set_topic(&self, channel: &str, topic: &str) -> Result<(), AgentError> {
        self.api_post(
            "conversations.setTopic",
            &serde_json::json!({
                "channel": channel,
                "topic": topic,
            }),
        )
        .await?;
        Ok(())
    }

    async fn add_reaction(
        &self,
        channel: &str,
        msg_ts: &str,
        emoji: &str,
    ) -> Result<(), AgentError> {
        self.api_post(
            "reactions.add",
            &serde_json::json!({
                "channel": channel,
                "timestamp": msg_ts,
                "name": emoji,
            }),
        )
        .await?;
        Ok(())
    }

    async fn remove_reaction(
        &self,
        channel: &str,
        msg_ts: &str,
        emoji: &str,
    ) -> Result<(), AgentError> {
        self.api_post(
            "reactions.remove",
            &serde_json::json!({
                "channel": channel,
                "timestamp": msg_ts,
                "name": emoji,
            }),
        )
        .await?;
        Ok(())
    }

    async fn upload_file(
        &self,
        channels: &[String],
        file_path: &str,
        title: Option<&str>,
    ) -> Result<String, AgentError> {
        for component in std::path::Path::new(file_path).components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(AgentError::invalid_input(
                    "file_path must not contain '..' components",
                ));
            }
        }

        let file_content = tokio::fs::read(file_path)
            .await
            .map_err(|e| AgentError::invalid_input(format!("cannot read file {file_path}: {e}")))?;

        let filename = std::path::Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let file_len = file_content.len();

        let url_resp = self
            .api_get(
                "files.getUploadURLExternal",
                &[
                    ("filename", &filename),
                    ("length", &file_len.to_string()),
                ],
            )
            .await?;

        let upload_url = url_resp["upload_url"]
            .as_str()
            .ok_or_else(|| AgentError::api("slack: no upload_url in response"))?;
        let file_id = url_resp["file_id"]
            .as_str()
            .ok_or_else(|| AgentError::api("slack: no file_id in response"))?
            .to_string();

        debug!(upload_url, file_id, "slack uploading to presigned URL");
        let upload_resp = self
            .client
            .post(upload_url)
            .body(file_content)
            .send()
            .await
            .map_err(|e| AgentError::network(format!("slack upload failed: {e}")))?;

        if !upload_resp.status().is_success() {
            let body = upload_resp.text().await.unwrap_or_default();
            return Err(AgentError::api(format!("slack upload failed: {body}")));
        }

        let mut files_arr = vec![serde_json::json!({"id": file_id})];
        if let Some(t) = title {
            files_arr[0]["title"] = serde_json::Value::String(t.to_string());
        }

        let channel_id = channels.first().map(|s| s.as_str()).unwrap_or("");
        self.api_post(
            "files.completeUploadExternal",
            &serde_json::json!({
                "files": files_arr,
                "channel_id": channel_id,
            }),
        )
        .await?;

        Ok(format!("Uploaded file {filename} (id: {file_id})"))
    }

    async fn list_users(&self, limit: usize) -> Result<Vec<ChatMember>, AgentError> {
        let limit_s = limit.to_string();
        let resp = self
            .api_get("users.list", &[("limit", limit_s.as_str())])
            .await?;

        let members: Vec<ChatMember> = resp["members"]
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .filter(|u| !u["deleted"].as_bool().unwrap_or(false))
            .map(|u| {
                let role = if u["is_owner"].as_bool().unwrap_or(false) {
                    MemberRole::Owner
                } else if u["is_admin"].as_bool().unwrap_or(false) {
                    MemberRole::Admin
                } else {
                    MemberRole::Member
                };
                ChatMember {
                    user_id: u["id"].as_str().unwrap_or("").to_string(),
                    name: u["real_name"]
                        .as_str()
                        .unwrap_or(u["name"].as_str().unwrap_or(""))
                        .to_string(),
                    username: u["name"].as_str().map(|s| s.to_string()),
                    role,
                    joined_date: None,
                }
            })
            .collect();

        Ok(members)
    }

    async fn get_user_info(&self, user: &str) -> Result<Profile, AgentError> {
        let resp = self
            .api_get("users.info", &[("user", user)])
            .await?;

        let u = &resp["user"];
        Ok(Profile {
            platform: Platform::Slack,
            id: u["id"].as_str().unwrap_or("").to_string(),
            name: u["real_name"]
                .as_str()
                .unwrap_or(u["name"].as_str().unwrap_or("Unknown"))
                .to_string(),
            username: u["name"].as_str().map(|s| s.to_string()),
            email: u["profile"]["email"].as_str().map(|s| s.to_string()),
            phone: None,
        })
    }

    async fn edit_message(
        &self,
        channel: &str,
        msg_ts: &str,
        text: &str,
    ) -> Result<Message, AgentError> {
        let resp = self
            .api_post(
                "chat.update",
                &serde_json::json!({
                    "channel": channel,
                    "ts": msg_ts,
                    "text": text,
                }),
            )
            .await?;
        Ok(parse_slack_message(&resp["message"], channel))
    }

    async fn delete_message(&self, channel: &str, msg_ts: &str) -> Result<(), AgentError> {
        self.api_post(
            "chat.delete",
            &serde_json::json!({
                "channel": channel,
                "ts": msg_ts,
            }),
        )
        .await?;
        Ok(())
    }

    async fn read_thread(
        &self,
        channel: &str,
        thread_ts: &str,
        limit: usize,
    ) -> Result<Vec<Message>, AgentError> {
        let limit_s = limit.to_string();
        let resp = self
            .api_get(
                "conversations.replies",
                &[
                    ("channel", channel),
                    ("ts", thread_ts),
                    ("limit", &limit_s),
                ],
            )
            .await?;

        let messages: Vec<Message> = resp["messages"]
            .as_array()
            .map_or(&[] as &[Value], |v| v)
            .iter()
            .map(|m| parse_slack_message(m, channel))
            .collect();

        Ok(messages)
    }
}

fn parse_slack_retry_after(resp: &reqwest::Response) -> Duration {
    let secs: u64 = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    Duration::from_secs(secs.clamp(1, 30))
}

async fn parse_slack_response(resp: reqwest::Response) -> Result<Value, AgentError> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AgentError::auth(format!(
                "slack auth failed ({status}): {body}"
            )));
        }
        return Err(AgentError::api(format!(
            "slack http error ({status}): {body}"
        )));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| AgentError::api(format!("slack response parse failed: {e}")))?;
    check_slack_ok(&body)?;
    Ok(body)
}

fn check_slack_ok(resp: &Value) -> Result<(), AgentError> {
    if resp["ok"].as_bool() != Some(true) {
        let error = resp["error"].as_str().unwrap_or("unknown_error");
        if error == "invalid_auth" || error == "not_authed" || error == "token_revoked" {
            return Err(AgentError::auth(format!("slack auth error: {error}")));
        }
        return Err(AgentError::api(format!("slack api error: {error}")));
    }
    Ok(())
}

fn parse_slack_channel(ch: &Value) -> Channel {
    let is_im = ch["is_im"].as_bool().unwrap_or(false);
    let is_mpim = ch["is_mpim"].as_bool().unwrap_or(false);
    let is_private = ch["is_private"].as_bool().unwrap_or(false);

    let channel_type = if is_im {
        ChannelType::Private
    } else if is_mpim || is_private {
        ChannelType::Group
    } else {
        ChannelType::Broadcast
    };

    let name = ch["name"]
        .as_str()
        .or_else(|| ch["user"].as_str())
        .unwrap_or("unknown")
        .to_string();

    Channel {
        id: ch["id"].as_str().unwrap_or("").to_string(),
        platform: Platform::Slack,
        name,
        channel_type,
        unread_count: ch["unread_count"].as_i64().unwrap_or(0) as i32,
        description: ch["topic"]["value"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        member_count: ch["num_members"].as_i64().map(|n| n as i32),
        last_message_date: None,
    }
}

fn parse_slack_message(m: &Value, channel: &str) -> Message {
    let ts_str = m["ts"].as_str().unwrap_or("0");
    let timestamp = ts_str
        .split('.')
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let has_files = m["files"].as_array().is_some_and(|a| !a.is_empty());

    Message {
        id: ts_str.to_string(),
        platform: Platform::Slack,
        channel_id: channel.to_string(),
        sender: m["user"]
            .as_str()
            .or_else(|| m["username"].as_str())
            .unwrap_or("unknown")
            .to_string(),
        text: m["text"].as_str().unwrap_or("").to_string(),
        timestamp,
        has_attachment: has_files,
        reply_to: m["thread_ts"]
            .as_str()
            .filter(|ts| *ts != ts_str)
            .map(|s| s.to_string()),
        meta: MessageMeta::default(),
    }
}
