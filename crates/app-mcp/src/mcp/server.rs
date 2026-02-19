use std::sync::Arc;

use nexus_domain::*;
use nexus_error::ErrorResponse;
use nexus_discord::DiscordAdapter;
use nexus_google::GmailAdapter;
use nexus_messaging::{AgentService, Format};
use nexus_messaging::format;
use nexus_slack::SlackAdapter;
use nexus_tdlib::TdlibAdapter;
use nexus_whatsapp::WhatsAppAdapter;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, warn};

use super::tools;
use super::types::*;

pub struct McpServer {
    agent: AgentService,
    telegram: Option<Arc<TdlibAdapter>>,
    gmail: Option<Arc<GmailAdapter>>,
    whatsapp: Option<Arc<WhatsAppAdapter>>,
    slack: Option<Arc<SlackAdapter>>,
    discord: Option<Arc<DiscordAdapter>>,
}

impl McpServer {
    pub fn new(
        agent: AgentService,
        telegram: Option<Arc<TdlibAdapter>>,
        gmail: Option<Arc<GmailAdapter>>,
        whatsapp: Option<Arc<WhatsAppAdapter>>,
        slack: Option<Arc<SlackAdapter>>,
        discord: Option<Arc<DiscordAdapter>>,
    ) -> Self {
        Self {
            agent,
            telegram,
            gmail,
            whatsapp,
            slack,
            discord,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let stdin = BufReader::new(tokio::io::stdin());
        let mut stdout = tokio::io::stdout();
        let mut lines = stdin.lines();

        debug!("MCP server started, waiting for requests on stdin");

        while let Some(line) = lines.next_line().await? {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let msg: RpcMessage = match serde_json::from_str(&line) {
                Ok(m) => m,
                Err(e) => {
                    let resp =
                        RpcResponse::err(Value::Null, PARSE_ERROR, format!("parse error: {e}"));
                    write_response(&mut stdout, &resp).await?;
                    continue;
                }
            };

            if msg.id.is_none() {
                if msg.method.as_deref() == Some("notifications/initialized") {
                    debug!("client initialized");
                }
                continue;
            }

            let id = msg.id.clone().unwrap_or(Value::Null);

            if !msg.is_valid_jsonrpc() {
                let resp = RpcResponse::err(id, -32600, "invalid jsonrpc version (expected \"2.0\")");
                write_response(&mut stdout, &resp).await?;
                continue;
            }

            let method = msg.method.as_deref().unwrap_or("");

            let resp = if method.is_empty() {
                RpcResponse::err(id, -32600, "missing method")
            } else {
                match method {
                    "initialize" => self.handle_initialize(id),
                    "tools/list" => self.handle_tools_list(id),
                    "tools/call" => self.handle_tools_call(id, msg.params).await,
                    "ping" => RpcResponse::ok(id, json!({})),
                    _ => RpcResponse::err(
                        id,
                        METHOD_NOT_FOUND,
                        format!("unknown method: {method}"),
                    ),
                }
            };

            write_response(&mut stdout, &resp).await?;
        }

        debug!("stdin closed, MCP server shutting down");
        Ok(())
    }

    fn handle_initialize(&self, id: Value) -> RpcResponse {
        let platforms: Vec<String> = self
            .agent
            .available_platforms()
            .iter()
            .map(|p| p.to_string())
            .collect();
        debug!(?platforms, "initialized with platforms");

        RpcResponse::ok(
            id,
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "nexus",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
    }

    fn handle_tools_list(&self, id: Value) -> RpcResponse {
        let has_tg = self.telegram.is_some();
        let has_gm = self.gmail.is_some();
        let has_wa = self.whatsapp.is_some();
        let has_sl = self.slack.is_some();
        let has_dc = self.discord.is_some();
        let tool_defs = tools::available_tools(has_tg, has_gm, has_wa, has_sl, has_dc);
        RpcResponse::ok(id, json!({ "tools": tool_defs }))
    }

    async fn handle_tools_call(&self, id: Value, params: Option<Value>) -> RpcResponse {
        let params: CallToolParams = match params.and_then(|v| serde_json::from_value(v).ok()) {
            Some(p) => p,
            None => return RpcResponse::err(id, INVALID_PARAMS, "missing or invalid params"),
        };

        let args = params.arguments.unwrap_or(json!({}));
        let result = self.dispatch_tool(&params.name, &args).await;

        let tool_result = match result {
            Ok(text) => ToolResult::success(text),
            Err(e) => ToolResult::failure(e),
        };

        match serde_json::to_value(tool_result) {
            Ok(val) => RpcResponse::ok(id, val),
            Err(e) => RpcResponse::err(id, -32603, format!("serialization error: {e}")),
        }
    }

    async fn dispatch_tool(&self, name: &str, args: &Value) -> Result<String, String> {
        let fmt = Format::parse(args.get("format").and_then(|v| v.as_str()));

        match name {
            "get_profile" => {
                let p = parse_platform(args)?;
                let profile = self.agent.get_profile(p).await.map_err(fmt_err)?;
                Ok(format::format_profile(&profile, fmt))
            }
            "list_channels" => {
                let p = parse_platform(args)?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let channels = self
                    .agent
                    .list_channels(p, limit)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_channels(&channels, fmt))
            }
            "read_messages" => {
                let p = parse_platform(args)?;
                let channel = get_str(args, "channel")?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let cursor = args.get("cursor").and_then(|v| v.as_str());
                let result = self
                    .agent
                    .read_messages(p, channel, limit, cursor)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_paginated(&result, fmt))
            }
            "send_message" => {
                let p = parse_platform(args)?;
                let channel = get_str(args, "channel")?;
                let text = get_str(args, "text")?;
                let reply_to_owned: Option<String> = match args.get("reply_to") {
                    Some(v) if v.is_string() => v.as_str().map(|s| s.to_string()),
                    Some(v) if v.is_number() => v.as_i64().map(|n| n.to_string()),
                    _ => None,
                };
                let msg = self
                    .agent
                    .send_message(p, channel, text, reply_to_owned.as_deref())
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }
            "search" => {
                let p = parse_platform(args)?;
                let query = get_str(args, "query")?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let cursor = args.get("cursor").and_then(|v| v.as_str());
                let result = self
                    .agent
                    .search(p, query, limit, cursor)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_paginated(&result, fmt))
            }
            "list_platforms" => {
                let platforms = self.agent.available_platforms();
                let lines: Vec<String> = platforms.iter().map(|p| p.to_string()).collect();
                Ok(format!("Connected platforms: {}", lines.join(", ")))
            }

            // --- Telegram tools ---
            "telegram_download_media" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_id = get_i64(args, "message_id")?;
                let path = get_str(args, "save_path")?;
                tg.download_media(chat, msg_id, path)
                    .await
                    .map_err(fmt_err)
            }
            "telegram_forward_message" => {
                let tg = self.require_telegram()?;
                let from = get_str(args, "from_chat")?;
                let to = get_str(args, "to_chat")?;
                let msg_id = get_i64(args, "message_id")?;
                let msg = tg
                    .forward_message(from, to, msg_id)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }
            "telegram_edit_message" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_id = get_i64(args, "message_id")?;
                let text = get_str(args, "text")?;
                let msg = tg
                    .edit_message(chat, msg_id, text)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }
            "telegram_delete_messages" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_ids = get_i64_array(args, "message_ids")?;
                tg.delete_messages(chat, &msg_ids)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Deleted {} message(s) from {chat}", msg_ids.len()))
            }
            "telegram_pin_message" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_id = get_i64(args, "message_id")?;
                tg.pin_message(chat, msg_id).await.map_err(fmt_err)?;
                Ok(format!("Pinned message {msg_id} in {chat}"))
            }
            "telegram_unpin_message" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_id = get_i64(args, "message_id")?;
                tg.unpin_message(chat, msg_id).await.map_err(fmt_err)?;
                Ok(format!("Unpinned message {msg_id} in {chat}"))
            }
            "telegram_get_chat_info" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let info = tg.get_chat_info(chat).await.map_err(fmt_err)?;
                Ok(format::format_chat_info(&info, fmt))
            }
            "telegram_mark_read" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_id = get_i64(args, "message_id")?;
                tg.mark_read(chat, msg_id).await.map_err(fmt_err)?;
                Ok(format!("Marked read up to message {msg_id} in {chat}"))
            }
            "telegram_get_message" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_id = get_i64(args, "message_id")?;
                let msg = tg.get_message(chat, msg_id).await.map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }
            "telegram_send_media" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let file_path = get_str(args, "file_path")?;
                let caption = args.get("caption").and_then(|v| v.as_str());
                let media_type = args.get("media_type").and_then(|v| v.as_str());
                let msg = tg
                    .send_media(chat, file_path, caption, media_type)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }
            "telegram_react" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let msg_id = get_i64(args, "message_id")?;
                let emoji = get_str(args, "emoji")?;
                tg.react_message(chat, msg_id, emoji)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Reacted with {emoji} to message {msg_id} in {chat}"))
            }
            "telegram_search_chat" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let query = get_str(args, "query")?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let messages = tg
                    .search_chat(chat, query, limit)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_messages(&messages, fmt))
            }
            "telegram_get_chat_members" => {
                let tg = self.require_telegram()?;
                let chat = get_str(args, "chat")?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
                let members = tg
                    .get_chat_members(chat, limit)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_members(&members, fmt))
            }

            // --- Gmail tools ---
            "gmail_send_email" => {
                let gm = self.require_gmail()?;
                let to = get_str_array(args, "to")?;
                let cc = get_str_array_opt(args, "cc");
                let bcc = get_str_array_opt(args, "bcc");
                let subject = get_str(args, "subject")?;
                let body = get_str(args, "body")?;
                let reply_to = args.get("reply_to").and_then(|v| v.as_str());
                let attachments = get_str_array_opt(args, "attachments");
                let msg = gm
                    .send_email(&to, &cc, &bcc, subject, body, reply_to, &attachments)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }
            "gmail_archive" => {
                let gm = self.require_gmail()?;
                let thread_id = get_str(args, "thread_id")?;
                gm.archive(thread_id).await.map_err(fmt_err)?;
                Ok(format!("Archived thread {thread_id}"))
            }
            "gmail_list_labels" => {
                let gm = self.require_gmail()?;
                let labels = gm.list_labels().await.map_err(fmt_err)?;
                Ok(format::format_labels(&labels, fmt))
            }
            "gmail_add_label" => {
                let gm = self.require_gmail()?;
                let thread_id = get_str(args, "thread_id")?;
                let label = get_str(args, "label")?;
                gm.add_label(thread_id, label).await.map_err(fmt_err)?;
                Ok(format!("Added label '{label}' to thread {thread_id}"))
            }
            "gmail_mark_read" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                gm.mark_read(message_id).await.map_err(fmt_err)?;
                Ok(format!("Marked message {message_id} as read"))
            }
            "gmail_mark_unread" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                gm.mark_unread(message_id).await.map_err(fmt_err)?;
                Ok(format!("Marked message {message_id} as unread"))
            }
            "gmail_star" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                gm.star(message_id).await.map_err(fmt_err)?;
                Ok(format!("Starred message {message_id}"))
            }
            "gmail_unstar" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                gm.unstar(message_id).await.map_err(fmt_err)?;
                Ok(format!("Unstarred message {message_id}"))
            }
            "gmail_move_to" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                let folder = get_str(args, "folder")?;
                gm.move_to(message_id, folder).await.map_err(fmt_err)?;
                Ok(format!("Moved message {message_id} to {folder}"))
            }
            "gmail_trash" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                gm.trash(message_id).await.map_err(fmt_err)?;
                Ok(format!("Trashed message {message_id}"))
            }
            "gmail_remove_label" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                let label = get_str(args, "label")?;
                gm.remove_label(message_id, label)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Removed label '{label}' from message {message_id}"))
            }
            "gmail_get_attachment" => {
                let gm = self.require_gmail()?;
                let message_id = get_str(args, "message_id")?;
                let filename = get_str(args, "filename")?;
                let save_path = get_str(args, "save_path")?;
                let result = gm
                    .get_attachment(message_id, filename, save_path)
                    .await
                    .map_err(fmt_err)?;
                Ok(result)
            }
            "gmail_create_draft" => {
                let gm = self.require_gmail()?;
                let to = get_str_array(args, "to")?;
                let subject = get_str(args, "subject")?;
                let body = get_str(args, "body")?;
                let msg = gm
                    .create_draft(&to, subject, body)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }

            // --- WhatsApp tools ---
            "whatsapp_send_media" => {
                let wa = self.require_whatsapp()?;
                let chat = get_str(args, "chat")?;
                let file_path = get_str(args, "file_path")?;
                let caption = get_str(args, "caption")?;
                let msg = wa
                    .send_media(chat, file_path, caption)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_message(&msg, fmt))
            }

            // --- Slack tools ---
            "slack_set_status" => {
                let sl = self.require_slack()?;
                let text = get_str(args, "text")?;
                let emoji = get_str(args, "emoji")?;
                sl.set_status(text, emoji).await.map_err(fmt_err)?;
                Ok(format!("Status set to {emoji} {text}"))
            }
            "slack_create_channel" => {
                let sl = self.require_slack()?;
                let name = get_str(args, "name")?;
                let is_private = args
                    .get("is_private")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let ch = sl
                    .create_channel(name, is_private)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_channels(&[ch], fmt))
            }
            "slack_invite_to_channel" => {
                let sl = self.require_slack()?;
                let channel = get_str(args, "channel")?;
                let user_id = get_str(args, "user_id")?;
                sl.invite_to_channel(channel, user_id)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Invited {user_id} to {channel}"))
            }
            "slack_set_topic" => {
                let sl = self.require_slack()?;
                let channel = get_str(args, "channel")?;
                let topic = get_str(args, "topic")?;
                sl.set_topic(channel, topic).await.map_err(fmt_err)?;
                Ok(format!("Set topic of {channel}"))
            }
            "slack_add_reaction" => {
                let sl = self.require_slack()?;
                let channel = get_str(args, "channel")?;
                let msg_ts = get_str(args, "message_ts")?;
                let emoji = get_str(args, "emoji")?;
                sl.add_reaction(channel, msg_ts, emoji)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Added :{emoji}: reaction"))
            }
            "slack_remove_reaction" => {
                let sl = self.require_slack()?;
                let channel = get_str(args, "channel")?;
                let msg_ts = get_str(args, "message_ts")?;
                let emoji = get_str(args, "emoji")?;
                sl.remove_reaction(channel, msg_ts, emoji)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Removed :{emoji}: reaction"))
            }
            "slack_upload_file" => {
                let sl = self.require_slack()?;
                let channels = get_str_array(args, "channels")?;
                let file_path = get_str(args, "file_path")?;
                let title = args.get("title").and_then(|v| v.as_str());
                let result = sl
                    .upload_file(&channels, file_path, title)
                    .await
                    .map_err(fmt_err)?;
                Ok(result)
            }
            "slack_list_users" => {
                let sl = self.require_slack()?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
                let members = sl.list_users(limit).await.map_err(fmt_err)?;
                Ok(format::format_members(&members, fmt))
            }
            "slack_get_user_info" => {
                let sl = self.require_slack()?;
                let user = get_str(args, "user")?;
                let profile = sl.get_user_info(user).await.map_err(fmt_err)?;
                Ok(format::format_profile(&profile, fmt))
            }

            // --- Discord tools ---
            "discord_list_guilds" => {
                let dc = self.require_discord()?;
                let guilds = dc.list_guilds().await.map_err(fmt_err)?;
                Ok(format::format_channels(&guilds, fmt))
            }
            "discord_list_guild_channels" => {
                let dc = self.require_discord()?;
                let guild_id = get_str(args, "guild_id")?;
                let channels = dc
                    .list_guild_channels(guild_id)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_channels(&channels, fmt))
            }
            "discord_create_thread" => {
                let dc = self.require_discord()?;
                let channel = get_str(args, "channel")?;
                let name = get_str(args, "name")?;
                let msg_id = args.get("message_id").and_then(|v| v.as_str());
                let thread = dc
                    .create_thread(channel, name, msg_id)
                    .await
                    .map_err(fmt_err)?;
                Ok(format::format_channels(&[thread], fmt))
            }
            "discord_add_reaction" => {
                let dc = self.require_discord()?;
                let channel = get_str(args, "channel")?;
                let msg_id = get_str(args, "message_id")?;
                let emoji = get_str(args, "emoji")?;
                dc.add_reaction(channel, msg_id, emoji)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Added {emoji} reaction to message {msg_id}"))
            }
            "discord_remove_reaction" => {
                let dc = self.require_discord()?;
                let channel = get_str(args, "channel")?;
                let msg_id = get_str(args, "message_id")?;
                let emoji = get_str(args, "emoji")?;
                dc.remove_reaction(channel, msg_id, emoji)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Removed {emoji} reaction from message {msg_id}"))
            }
            "discord_pin_message" => {
                let dc = self.require_discord()?;
                let channel = get_str(args, "channel")?;
                let msg_id = get_str(args, "message_id")?;
                dc.pin_message(channel, msg_id)
                    .await
                    .map_err(fmt_err)?;
                Ok(format!("Pinned message {msg_id} in channel {channel}"))
            }

            unknown => {
                warn!(tool = unknown, "unknown tool called");
                Err(format!("unknown tool: {unknown}"))
            }
        }
    }

    fn require_telegram(&self) -> Result<&Arc<TdlibAdapter>, String> {
        self.telegram
            .as_ref()
            .ok_or_else(|| "telegram not configured. Set TELEGRAM_API_ID and TELEGRAM_API_HASH env vars".to_string())
    }

    fn require_gmail(&self) -> Result<&Arc<GmailAdapter>, String> {
        self.gmail
            .as_ref()
            .ok_or_else(|| "gmail not configured. Set GMAIL_ADDRESS and GMAIL_APP_PASSWORD env vars".to_string())
    }

    fn require_whatsapp(&self) -> Result<&Arc<WhatsAppAdapter>, String> {
        self.whatsapp
            .as_ref()
            .ok_or_else(|| "whatsapp not configured. Set WHATSAPP_ACCESS_TOKEN and WHATSAPP_PHONE_NUMBER_ID env vars".to_string())
    }

    fn require_slack(&self) -> Result<&Arc<SlackAdapter>, String> {
        self.slack
            .as_ref()
            .ok_or_else(|| "slack not configured. Set SLACK_BOT_TOKEN env var".to_string())
    }

    fn require_discord(&self) -> Result<&Arc<DiscordAdapter>, String> {
        self.discord
            .as_ref()
            .ok_or_else(|| "discord not configured. Set DISCORD_BOT_TOKEN env var".to_string())
    }
}

fn fmt_err(e: nexus_error::AgentError) -> String {
    let resp = ErrorResponse::from(&e);
    resp.to_compact()
}

fn parse_platform(args: &Value) -> Result<Platform, String> {
    let name = args
        .get("platform")
        .and_then(|v| v.as_str())
        .ok_or("missing 'platform' parameter")?;
    name.parse::<Platform>().map_err(|e| e.to_string())
}

fn get_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or(format!("missing '{key}' parameter"))
}

fn get_i64(args: &Value, key: &str) -> Result<i64, String> {
    args.get(key)
        .and_then(|v| v.as_i64())
        .ok_or(format!("missing '{key}' parameter"))
}

fn get_i64_array(args: &Value, key: &str) -> Result<Vec<i64>, String> {
    let arr = args
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or(format!("missing '{key}' parameter"))?;
    arr.iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_i64()
                .ok_or(format!("'{key}[{i}]' is not an integer"))
        })
        .collect()
}

fn get_str_array(args: &Value, key: &str) -> Result<Vec<String>, String> {
    let arr = args
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or(format!("missing '{key}' parameter"))?;
    arr.iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_str()
                .map(|s| s.to_string())
                .ok_or(format!("'{key}[{i}]' is not a string"))
        })
        .collect()
}

fn get_str_array_opt(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

async fn write_response(
    stdout: &mut tokio::io::Stdout,
    resp: &RpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(resp)?;
    stdout.write_all(json.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}
