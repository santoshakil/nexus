use async_trait::async_trait;
use nexus_domain::*;
use nexus_error::AgentError;
use serde_json::{json, Value};
use tracing::debug;

use crate::client::TdClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const CHAT_CACHE_TTL: Duration = Duration::from_secs(300);

pub struct TdlibAdapter {
    client: Arc<TdClient>,
    chat_cache: Arc<RwLock<HashMap<String, (i64, Instant)>>>,
}

impl TdlibAdapter {
    pub fn new(client: Arc<TdClient>) -> Self {
        Self {
            client,
            chat_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn cache_key(chat: &str) -> String {
        chat.to_lowercase()
    }

    async fn cache_get(&self, key: &str) -> Option<i64> {
        let cache = self.chat_cache.read().await;
        cache
            .get(key)
            .filter(|(_, ts)| ts.elapsed() < CHAT_CACHE_TTL)
            .map(|(id, _)| *id)
    }

    async fn cache_put(&self, key: String, id: i64) {
        let mut cache = self.chat_cache.write().await;
        cache.insert(key, (id, Instant::now()));
    }

    async fn resolve_chat_id(&self, chat: &str) -> Result<i64, AgentError> {
        if let Ok(id) = chat.parse::<i64>() {
            return Ok(id);
        }

        let key = Self::cache_key(chat);
        if let Some(id) = self.cache_get(&key).await {
            debug!(chat, id, "chat cache hit");
            return Ok(id);
        }

        let username = chat.strip_prefix('@').unwrap_or(chat);
        let resp = self
            .client
            .send(json!({
                "@type": "searchPublicChat",
                "username": username,
            }))
            .await;

        if let Ok(val) = resp {
            if let Some(id) = val.get("id").and_then(|v| v.as_i64()) {
                self.cache_put(key, id).await;
                return Ok(id);
            }
        }

        let resp = self
            .client
            .send(json!({
                "@type": "searchChatsOnServer",
                "query": chat,
                "limit": 5,
            }))
            .await?;

        let ids = resp
            .get("chat_ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::not_found(format!("chat '{chat}' not found")))?;

        if ids.is_empty() {
            return Err(AgentError::not_found(format!("chat '{chat}' not found")));
        }

        let id = ids[0]
            .as_i64()
            .ok_or_else(|| AgentError::internal("invalid chat id from TDLib"))?;

        self.cache_put(key, id).await;
        Ok(id)
    }

    fn parse_chat(val: &Value) -> Option<Channel> {
        let id = val.get("id")?.as_i64()?;
        let title = val
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let unread = val
            .get("unread_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        let chat_type =
            match val
                .get("type")
                .and_then(|t| t.get("@type"))
                .and_then(|t| t.as_str())
            {
                Some("chatTypePrivate") => ChannelType::Private,
                Some("chatTypeSupergroup") => {
                    let is_channel = val
                        .get("type")
                        .and_then(|t| t.get("is_channel"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if is_channel {
                        ChannelType::Broadcast
                    } else {
                        ChannelType::Group
                    }
                }
                Some("chatTypeBasicGroup") => ChannelType::Group,
                Some("chatTypeSecret") => ChannelType::Private,
                other => ChannelType::Other(other.unwrap_or("unknown").to_string()),
            };

        let last_msg_date = val
            .get("last_message")
            .and_then(|m| m.get("date"))
            .and_then(|d| d.as_i64());

        Some(Channel {
            id: id.to_string(),
            platform: Platform::Telegram,
            name: title,
            channel_type: chat_type,
            unread_count: unread,
            description: None,
            member_count: None,
            last_message_date: last_msg_date,
        })
    }

    async fn resolve_senders(&self, messages: &mut [Message]) {
        let mut user_ids: Vec<i64> = Vec::new();
        for msg in messages.iter() {
            if let Some(id_str) = msg.sender.strip_prefix("user:") {
                if let Ok(id) = id_str.parse::<i64>() {
                    if !user_ids.contains(&id) {
                        user_ids.push(id);
                    }
                }
            }
        }

        let mut names: HashMap<i64, String> = HashMap::new();
        for uid in user_ids {
            if let Ok(user) = self
                .client
                .send(json!({"@type": "getUser", "user_id": uid}))
                .await
            {
                let first = user
                    .get("first_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let last = user
                    .get("last_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let username = user
                    .get("usernames")
                    .and_then(|u| u.get("active_usernames"))
                    .and_then(|a| a.as_array())
                    .and_then(|a| a.first())
                    .and_then(|u| u.as_str());

                let display = if !last.is_empty() {
                    format!("{first} {last}")
                } else {
                    first.to_string()
                };

                let name = match username {
                    Some(u) => format!("{display} (@{u})"),
                    None => display,
                };
                names.insert(uid, name);
            }
        }

        for msg in messages.iter_mut() {
            if let Some(id_str) = msg.sender.strip_prefix("user:") {
                if let Ok(id) = id_str.parse::<i64>() {
                    if let Some(name) = names.get(&id) {
                        msg.sender = name.clone();
                    }
                }
            }
        }
    }

    fn parse_message(val: &Value) -> Option<Message> {
        let id = val.get("id")?.as_i64()?;
        let chat_id = val.get("chat_id")?.as_i64()?;
        let date = val.get("date").and_then(|v| v.as_i64()).unwrap_or(0);

        let sender = extract_sender(val);

        let content = val.get("content")?;
        let content_type = content.get("@type").and_then(|v| v.as_str()).unwrap_or("");

        let text = match content_type {
            "messageText" => content
                .get("text")
                .and_then(|t| t.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            "messagePhoto" => content
                .get("caption")
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| format!("[Photo] {s}"))
                .unwrap_or_else(|| "[Photo]".to_string()),
            "messageVideo" => {
                let caption = content
                    .get("caption")
                    .and_then(|c| c.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                if caption.is_empty() {
                    "[Video]".to_string()
                } else {
                    format!("[Video] {caption}")
                }
            }
            "messageDocument" => {
                let name = content
                    .get("document")
                    .and_then(|d| d.get("file_name"))
                    .and_then(|f| f.as_str())
                    .unwrap_or("file");
                format!("[Document: {name}]")
            }
            "messageVoiceNote" => "[Voice message]".to_string(),
            "messageAnimation" => "[GIF]".to_string(),
            "messageAudio" => {
                let title = content
                    .get("audio")
                    .and_then(|a| a.get("title"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("audio");
                format!("[Audio: {title}]")
            }
            "messageSticker" => {
                let emoji = content
                    .get("sticker")
                    .and_then(|s| s.get("emoji"))
                    .and_then(|e| e.as_str())
                    .unwrap_or("");
                format!("[Sticker {emoji}]")
            }
            "messageVideoNote" => "[Video message]".to_string(),
            "messagePoll" => {
                let question = content
                    .get("poll")
                    .and_then(|p| p.get("question"))
                    .and_then(|q| q.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("poll");
                format!("[Poll: {question}]")
            }
            "messageLocation" => "[Location]".to_string(),
            "messageContact" => "[Contact]".to_string(),
            _ => format!("[{content_type}]"),
        };

        let has_attachment = !matches!(content_type, "messageText");
        let media_type = if has_attachment {
            Some(content_type.replace("message", "").to_lowercase())
        } else {
            None
        };

        let reply_to = val
            .get("reply_to")
            .and_then(|r| r.get("message_id"))
            .and_then(|m| m.as_i64())
            .map(|id| id.to_string());

        let forward_from = val.get("forward_info").map(|_| "forwarded".to_string());

        let is_pinned = val.get("is_pinned").and_then(|v| v.as_bool());
        let edit_date = val
            .get("edit_date")
            .and_then(|v| v.as_i64())
            .filter(|&d| d > 0);

        let views = val
            .get("interaction_info")
            .and_then(|i| i.get("view_count"))
            .and_then(|v| v.as_i64())
            .filter(|&v| v > 0);

        let reactions = val
            .get("interaction_info")
            .and_then(|i| i.get("reactions"))
            .and_then(|r| r.get("reactions"))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        let emoji = r
                            .get("type")
                            .and_then(|t| t.get("emoji"))
                            .and_then(|e| e.as_str())
                            .unwrap_or("")
                            .to_string();
                        let count = r
                            .get("total_count")
                            .and_then(|c| c.as_i64())
                            .unwrap_or(0) as i32;
                        if emoji.is_empty() {
                            None
                        } else {
                            Some(Reaction { emoji, count })
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|r| !r.is_empty());

        Some(Message {
            id: id.to_string(),
            platform: Platform::Telegram,
            channel_id: chat_id.to_string(),
            sender,
            text,
            timestamp: date,
            has_attachment,
            reply_to,
            meta: MessageMeta {
                media_type,
                forward_from,
                reactions,
                views,
                edit_date,
                is_pinned,
                ..Default::default()
            },
        })
    }
}

fn extract_sender(msg: &Value) -> String {
    let sender = match msg.get("sender_id") {
        Some(s) => s,
        None => return "Unknown".to_string(),
    };

    match sender.get("@type").and_then(|t| t.as_str()) {
        Some("messageSenderUser") => {
            let user_id = sender
                .get("user_id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            format!("user:{user_id}")
        }
        Some("messageSenderChat") => {
            let chat_id = sender
                .get("chat_id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            format!("chat:{chat_id}")
        }
        _ => "Unknown".to_string(),
    }
}

#[async_trait]
impl MessagingPort for TdlibAdapter {
    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    async fn get_profile(&self) -> Result<Profile, AgentError> {
        let resp = self.client.send(json!({"@type": "getMe"})).await?;
        let first = resp
            .get("first_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let last = resp.get("last_name").and_then(|v| v.as_str());
        let name = match last {
            Some(l) if !l.is_empty() => format!("{first} {l}"),
            _ => first.to_string(),
        };
        let username = resp
            .get("usernames")
            .and_then(|u| u.get("active_usernames"))
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|u| u.as_str())
            .map(String::from);
        let phone = resp
            .get("phone_number")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);
        let id = resp.get("id").and_then(|v| v.as_i64()).unwrap_or(0);

        Ok(Profile {
            platform: Platform::Telegram,
            id: id.to_string(),
            name,
            username,
            email: None,
            phone,
        })
    }

    async fn list_channels(&self, limit: usize) -> Result<Vec<Channel>, AgentError> {
        let resp = self
            .client
            .send(json!({
                "@type": "getChats",
                "chat_list": {"@type": "chatListMain"},
                "limit": limit,
            }))
            .await?;

        let chat_ids = resp
            .get("chat_ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::api("unexpected getChats response"))?;

        let mut channels = Vec::with_capacity(chat_ids.len());
        for id_val in chat_ids {
            let chat_id = match id_val.as_i64() {
                Some(id) => id,
                None => continue,
            };
            let chat = self
                .client
                .send(json!({
                    "@type": "getChat",
                    "chat_id": chat_id,
                }))
                .await?;
            if let Some(ch) = Self::parse_chat(&chat) {
                self.cache_put(Self::cache_key(&ch.name), chat_id).await;
                channels.push(ch);
            }
        }

        debug!(count = channels.len(), "listed telegram chats");
        Ok(channels)
    }

    async fn read_messages(
        &self,
        channel: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        let chat_id = self.resolve_chat_id(channel).await?;

        let from_msg_id = cursor
            .and_then(|c| c.strip_prefix("tg:"))
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        let resp = self
            .client
            .send(json!({
                "@type": "getChatHistory",
                "chat_id": chat_id,
                "from_message_id": from_msg_id,
                "offset": 0,
                "limit": limit,
                "only_local": false,
            }))
            .await?;

        let raw_msgs = resp
            .get("messages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::api("unexpected getChatHistory response"))?;

        let raw_count = raw_msgs.len();
        let mut messages: Vec<Message> = raw_msgs.iter().filter_map(Self::parse_message).collect();
        self.resolve_senders(&mut messages).await;

        let has_more = raw_count == limit;
        let next_cursor = messages
            .last()
            .map(|m| format!("tg:{}", m.id));

        debug!(chat_id, count = messages.len(), has_more, "read telegram messages");
        Ok(Paginated {
            items: messages,
            has_more,
            next_cursor: if has_more { next_cursor } else { None },
        })
    }

    async fn send_message(
        &self,
        channel: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<Message, AgentError> {
        let chat_id = self.resolve_chat_id(channel).await?;

        let mut req = json!({
            "@type": "sendMessage",
            "chat_id": chat_id,
            "input_message_content": {
                "@type": "inputMessageText",
                "text": {
                    "@type": "formattedText",
                    "text": text,
                }
            }
        });

        if let Some(reply_str) = reply_to {
            let reply_id: i64 = reply_str
                .parse()
                .map_err(|_| AgentError::invalid_input(format!("invalid reply_to: {reply_str} (expected numeric message ID)")))?;
            req["reply_to"] = json!({
                "@type": "inputMessageReplyToMessage",
                "message_id": reply_id,
            });
        }

        let resp = self.client.send(req).await?;

        Self::parse_message(&resp)
            .ok_or_else(|| AgentError::internal("failed to parse sent message"))
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        let offset = cursor
            .and_then(|c| c.strip_prefix("tg:"))
            .unwrap_or("");

        let resp = self
            .client
            .send(json!({
                "@type": "searchMessages",
                "chat_list": {"@type": "chatListMain"},
                "query": query,
                "offset": offset,
                "limit": limit,
                "filter": null,
                "min_date": 0,
                "max_date": 0,
            }))
            .await?;

        let raw_msgs = resp
            .get("messages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::api("unexpected searchMessages response"))?;

        let next_offset = resp
            .get("next_offset")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| format!("tg:{s}"));

        let mut messages: Vec<Message> = raw_msgs.iter().filter_map(Self::parse_message).collect();
        self.resolve_senders(&mut messages).await;

        let has_more = next_offset.is_some();
        debug!(query, count = messages.len(), has_more, "searched telegram messages");
        Ok(Paginated {
            items: messages,
            has_more,
            next_cursor: next_offset,
        })
    }
}

#[async_trait]
impl TelegramExt for TdlibAdapter {
    async fn download_media(
        &self,
        chat: &str,
        msg_id: i64,
        path: &str,
    ) -> Result<String, AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        let resp = self
            .client
            .send(json!({
                "@type": "getMessage",
                "chat_id": chat_id,
                "message_id": msg_id,
            }))
            .await?;

        let file_id = extract_file_id(&resp)
            .ok_or_else(|| AgentError::not_found("no downloadable media in this message"))?;

        let dl = self
            .client
            .send(json!({
                "@type": "downloadFile",
                "file_id": file_id,
                "priority": 32,
                "synchronous": true,
            }))
            .await?;

        let local_path = dl
            .get("local")
            .and_then(|l| l.get("path"))
            .and_then(|p| p.as_str())
            .ok_or_else(|| AgentError::internal("no local path in download response"))?;

        if !path.is_empty() && local_path != path {
            if std::path::Path::new(path)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(AgentError::invalid_input(
                    "save_path must not contain '..' components",
                ));
            }
            tokio::fs::copy(local_path, path)
                .await
                .map_err(|e| AgentError::internal(format!("copy failed: {e}")))?;
            Ok(path.to_string())
        } else {
            Ok(local_path.to_string())
        }
    }

    async fn forward_message(
        &self,
        from_chat: &str,
        to_chat: &str,
        msg_id: i64,
    ) -> Result<Message, AgentError> {
        let from_id = self.resolve_chat_id(from_chat).await?;
        let to_id = self.resolve_chat_id(to_chat).await?;

        let resp = self
            .client
            .send(json!({
                "@type": "forwardMessages",
                "chat_id": to_id,
                "from_chat_id": from_id,
                "message_ids": [msg_id],
            }))
            .await?;

        let msgs = resp
            .get("messages")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first());

        match msgs {
            Some(m) => Self::parse_message(m)
                .ok_or_else(|| AgentError::internal("failed to parse forwarded message")),
            None => Err(AgentError::api("forward returned no messages")),
        }
    }

    async fn edit_message(
        &self,
        chat: &str,
        msg_id: i64,
        text: &str,
    ) -> Result<Message, AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        let resp = self
            .client
            .send(json!({
                "@type": "editMessageText",
                "chat_id": chat_id,
                "message_id": msg_id,
                "input_message_content": {
                    "@type": "inputMessageText",
                    "text": {
                        "@type": "formattedText",
                        "text": text,
                    }
                }
            }))
            .await?;

        Self::parse_message(&resp)
            .ok_or_else(|| AgentError::internal("failed to parse edited message"))
    }

    async fn delete_messages(
        &self,
        chat: &str,
        msg_ids: &[i64],
    ) -> Result<(), AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        self.client
            .send(json!({
                "@type": "deleteMessages",
                "chat_id": chat_id,
                "message_ids": msg_ids,
                "revoke": true,
            }))
            .await?;

        Ok(())
    }

    async fn pin_message(&self, chat: &str, msg_id: i64) -> Result<(), AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        self.client
            .send(json!({
                "@type": "pinChatMessage",
                "chat_id": chat_id,
                "message_id": msg_id,
                "disable_notification": false,
                "only_for_self": false,
            }))
            .await?;

        Ok(())
    }

    async fn unpin_message(&self, chat: &str, msg_id: i64) -> Result<(), AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        self.client
            .send(json!({
                "@type": "unpinChatMessage",
                "chat_id": chat_id,
                "message_id": msg_id,
            }))
            .await?;

        Ok(())
    }

    async fn get_chat_info(&self, chat: &str) -> Result<ChatInfo, AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        let chat_data = self
            .client
            .send(json!({
                "@type": "getChat",
                "chat_id": chat_id,
            }))
            .await?;

        let title = chat_data
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let unread = chat_data
            .get("unread_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        let chat_type_obj = chat_data.get("type");
        let type_str = chat_type_obj
            .and_then(|t| t.get("@type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let channel_type = match type_str {
            "chatTypePrivate" => ChannelType::Private,
            "chatTypeSupergroup" => {
                let is_ch = chat_type_obj
                    .and_then(|t| t.get("is_channel"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if is_ch {
                    ChannelType::Broadcast
                } else {
                    ChannelType::Group
                }
            }
            "chatTypeBasicGroup" => ChannelType::Group,
            _ => ChannelType::Other(type_str.to_string()),
        };

        let (description, member_count, invite_link, is_verified, is_scam) =
            match type_str {
                "chatTypeSupergroup" => {
                    let sg_id = chat_type_obj
                        .and_then(|t| t.get("supergroup_id"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let full = self
                        .client
                        .send(json!({
                            "@type": "getSupergroupFullInfo",
                            "supergroup_id": sg_id,
                        }))
                        .await
                        .ok();
                    let sg = self
                        .client
                        .send(json!({
                            "@type": "getSupergroup",
                            "supergroup_id": sg_id,
                        }))
                        .await
                        .ok();

                    let desc = full
                        .as_ref()
                        .and_then(|f| f.get("description"))
                        .and_then(|d| d.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());
                    let members = full
                        .as_ref()
                        .and_then(|f| f.get("member_count"))
                        .and_then(|m| m.as_i64())
                        .unwrap_or(0) as i32;
                    let link = full
                        .as_ref()
                        .and_then(|f| f.get("invite_link"))
                        .and_then(|l| l.get("invite_link"))
                        .and_then(|l| l.as_str())
                        .map(|s| s.to_string());
                    let verified = sg
                        .as_ref()
                        .and_then(|s| s.get("is_verified"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let scam = sg
                        .as_ref()
                        .and_then(|s| s.get("is_scam"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    (desc, members, link, verified, scam)
                }
                "chatTypeBasicGroup" => {
                    let bg_id = chat_type_obj
                        .and_then(|t| t.get("basic_group_id"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let full = self
                        .client
                        .send(json!({
                            "@type": "getBasicGroupFullInfo",
                            "basic_group_id": bg_id,
                        }))
                        .await
                        .ok();

                    let desc = full
                        .as_ref()
                        .and_then(|f| f.get("description"))
                        .and_then(|d| d.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());
                    let members = full
                        .as_ref()
                        .and_then(|f| f.get("members"))
                        .and_then(|m| m.as_array())
                        .map(|a| a.len() as i32)
                        .unwrap_or(0);

                    (desc, members, None, false, false)
                }
                _ => (None, 0, None, false, false),
            };

        Ok(ChatInfo {
            id: chat_id.to_string(),
            platform: Platform::Telegram,
            name: title,
            channel_type,
            description,
            member_count,
            unread_count: unread,
            invite_link,
            is_verified,
            is_scam,
        })
    }

    async fn mark_read(&self, chat: &str, msg_id: i64) -> Result<(), AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        self.client
            .send(json!({
                "@type": "viewMessages",
                "chat_id": chat_id,
                "message_ids": [msg_id],
                "force_read": true,
            }))
            .await?;

        Ok(())
    }

    async fn get_message(&self, chat: &str, msg_id: i64) -> Result<Message, AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        let resp = self
            .client
            .send(json!({
                "@type": "getMessage",
                "chat_id": chat_id,
                "message_id": msg_id,
            }))
            .await?;

        let msg = Self::parse_message(&resp)
            .ok_or_else(|| AgentError::not_found(format!("message {msg_id} not found")))?;

        let mut msgs = vec![msg];
        self.resolve_senders(&mut msgs).await;

        msgs.into_iter()
            .next()
            .ok_or_else(|| AgentError::internal("failed to resolve message sender"))
    }

    async fn send_media(
        &self,
        chat: &str,
        file_path: &str,
        caption: Option<&str>,
        media_type: Option<&str>,
    ) -> Result<Message, AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        let input_type = match media_type {
            Some("photo") => "inputMessagePhoto",
            Some("video") => "inputMessageVideo",
            Some("document") => "inputMessageDocument",
            Some(other) => {
                return Err(AgentError::invalid_input(format!(
                    "unsupported media_type '{other}', use photo/video/document"
                )));
            }
            None => {
                let lower = file_path.to_lowercase();
                if lower.ends_with(".jpg")
                    || lower.ends_with(".jpeg")
                    || lower.ends_with(".png")
                    || lower.ends_with(".gif")
                    || lower.ends_with(".webp")
                {
                    "inputMessagePhoto"
                } else if lower.ends_with(".mp4")
                    || lower.ends_with(".mov")
                    || lower.ends_with(".avi")
                {
                    "inputMessageVideo"
                } else {
                    "inputMessageDocument"
                }
            }
        };

        let file_key = match input_type {
            "inputMessagePhoto" => "photo",
            "inputMessageVideo" => "video",
            _ => "document",
        };

        let caption_obj = json!({
            "@type": "formattedText",
            "text": caption.unwrap_or(""),
        });

        let mut content = json!({
            "@type": input_type,
            "caption": caption_obj,
        });
        content[file_key] = json!({
            "@type": "inputFileLocal",
            "path": file_path,
        });

        let resp = self
            .client
            .send(json!({
                "@type": "sendMessage",
                "chat_id": chat_id,
                "input_message_content": content,
            }))
            .await?;

        Self::parse_message(&resp)
            .ok_or_else(|| AgentError::internal("failed to parse sent media message"))
    }

    async fn react_message(
        &self,
        chat: &str,
        msg_id: i64,
        emoji: &str,
    ) -> Result<(), AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        self.client
            .send(json!({
                "@type": "addMessageReaction",
                "chat_id": chat_id,
                "message_id": msg_id,
                "reaction_type": {
                    "@type": "reactionTypeEmoji",
                    "emoji": emoji,
                },
                "is_big": false,
                "update_recent_reactions": true,
            }))
            .await?;

        Ok(())
    }

    async fn search_chat(
        &self,
        chat: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Message>, AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        let resp = self
            .client
            .send(json!({
                "@type": "searchChatMessages",
                "chat_id": chat_id,
                "query": query,
                "sender_id": null,
                "from_message_id": 0,
                "offset": 0,
                "limit": limit,
                "filter": null,
                "message_thread_id": 0,
            }))
            .await?;

        let raw_msgs = resp
            .get("messages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::api("unexpected searchChatMessages response"))?;

        let mut messages: Vec<Message> = raw_msgs.iter().filter_map(Self::parse_message).collect();
        self.resolve_senders(&mut messages).await;

        debug!(chat_id, query, count = messages.len(), "searched chat messages");
        Ok(messages)
    }

    async fn get_chat_members(
        &self,
        chat: &str,
        limit: usize,
    ) -> Result<Vec<ChatMember>, AgentError> {
        let chat_id = self.resolve_chat_id(chat).await?;

        let chat_data = self
            .client
            .send(json!({
                "@type": "getChat",
                "chat_id": chat_id,
            }))
            .await?;

        let type_obj = chat_data.get("type");
        let type_str = type_obj
            .and_then(|t| t.get("@type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        match type_str {
            "chatTypeSupergroup" => {
                let sg_id = type_obj
                    .and_then(|t| t.get("supergroup_id"))
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| AgentError::internal("missing supergroup_id"))?;

                let resp = self
                    .client
                    .send(json!({
                        "@type": "getSupergroupMembers",
                        "supergroup_id": sg_id,
                        "filter": {
                            "@type": "supergroupMembersFilterRecent",
                        },
                        "offset": 0,
                        "limit": limit,
                    }))
                    .await?;

                let raw = resp
                    .get("members")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| AgentError::api("unexpected getSupergroupMembers response"))?;

                let mut members = Vec::with_capacity(raw.len());
                for m in raw {
                    if let Some(member) = parse_chat_member(m) {
                        members.push(member);
                    }
                }

                resolve_member_names(&self.client, &mut members).await;
                debug!(chat_id, count = members.len(), "got supergroup members");
                Ok(members)
            }
            "chatTypeBasicGroup" => {
                let bg_id = type_obj
                    .and_then(|t| t.get("basic_group_id"))
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| AgentError::internal("missing basic_group_id"))?;

                let resp = self
                    .client
                    .send(json!({
                        "@type": "getBasicGroupFullInfo",
                        "basic_group_id": bg_id,
                    }))
                    .await?;

                let raw = resp
                    .get("members")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| AgentError::api("unexpected getBasicGroupFullInfo response"))?;

                let mut members = Vec::with_capacity(raw.len());
                for m in raw {
                    if let Some(member) = parse_chat_member(m) {
                        members.push(member);
                    }
                }

                members.truncate(limit);
                resolve_member_names(&self.client, &mut members).await;
                debug!(chat_id, count = members.len(), "got basic group members");
                Ok(members)
            }
            _ => Err(AgentError::invalid_input(
                "get_chat_members only works on groups and supergroups",
            )),
        }
    }
}

fn extract_file_id(msg: &Value) -> Option<i64> {
    let content = msg.get("content")?;
    let content_type = content.get("@type")?.as_str()?;

    let file = match content_type {
        "messagePhoto" => content
            .get("photo")
            .and_then(|p| p.get("sizes"))
            .and_then(|s| s.as_array())
            .and_then(|a| a.last())
            .and_then(|s| s.get("photo")),
        "messageVideo" => content.get("video").and_then(|v| v.get("video")),
        "messageDocument" => content.get("document").and_then(|d| d.get("document")),
        "messageVoiceNote" => content.get("voice_note").and_then(|v| v.get("voice")),
        "messageAudio" => content.get("audio").and_then(|a| a.get("audio")),
        "messageAnimation" => content.get("animation").and_then(|a| a.get("animation")),
        "messageVideoNote" => content.get("video_note").and_then(|v| v.get("video")),
        _ => None,
    };

    file.and_then(|f| f.get("id")).and_then(|id| id.as_i64())
}

fn parse_chat_member(val: &Value) -> Option<ChatMember> {
    let user_id = val
        .get("member_id")
        .and_then(|m| m.get("user_id"))
        .and_then(|v| v.as_i64())?;

    let status = val.get("status")?;
    let status_type = status.get("@type").and_then(|t| t.as_str()).unwrap_or("");

    let role = match status_type {
        "chatMemberStatusCreator" => MemberRole::Owner,
        "chatMemberStatusAdministrator" => MemberRole::Admin,
        "chatMemberStatusMember" => MemberRole::Member,
        "chatMemberStatusRestricted" => MemberRole::Restricted,
        "chatMemberStatusBanned" => MemberRole::Banned,
        _ => MemberRole::Member,
    };

    let joined = val
        .get("joined_chat_date")
        .and_then(|v| v.as_i64())
        .filter(|&d| d > 0);

    Some(ChatMember {
        user_id: user_id.to_string(),
        name: format!("user:{user_id}"),
        username: None,
        role,
        joined_date: joined,
    })
}

async fn resolve_member_names(client: &TdClient, members: &mut [ChatMember]) {
    for member in members.iter_mut() {
        let uid_str = member
            .name
            .strip_prefix("user:")
            .unwrap_or(&member.user_id);
        let uid: i64 = match uid_str.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };

        if let Ok(user) = client
            .send(json!({"@type": "getUser", "user_id": uid}))
            .await
        {
            let first = user
                .get("first_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let last = user
                .get("last_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            member.name = if !last.is_empty() {
                format!("{first} {last}")
            } else {
                first.to_string()
            };

            member.username = user
                .get("usernames")
                .and_then(|u| u.get("active_usernames"))
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|u| u.as_str())
                .map(String::from);
        }
    }
}
