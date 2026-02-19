use nexus_domain::{Channel, ChannelType, ChatInfo, ChatMember, Message, Paginated, Platform, Profile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Compact,
    Expanded,
    Full,
}

impl Format {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("full") => Self::Full,
            Some("expanded") => Self::Expanded,
            _ => Self::Compact,
        }
    }
}

pub fn format_profile(profile: &Profile, fmt: Format) -> String {
    match fmt {
        Format::Compact | Format::Expanded => {
            let mut parts = vec![profile.name.clone()];
            if let Some(ref u) = profile.username {
                parts.push(format!("(@{u})"));
            }
            parts.push(format!("id:{}", profile.id));
            if let Some(ref e) = profile.email {
                parts.push(e.clone());
            }
            if let Some(ref p) = profile.phone {
                parts.push(format!("+{p}"));
            }
            parts.push(format!("[{}]", profile.platform));
            parts.join(" ")
        }
        Format::Full => to_json(profile),
    }
}

pub fn format_channels(channels: &[Channel], fmt: Format) -> String {
    match fmt {
        Format::Compact | Format::Expanded => {
            let mut lines: Vec<String> = Vec::with_capacity(channels.len() + 1);
            lines.push(format!("{} channels:", channels.len()));
            for ch in channels {
                lines.push(format_channel_line(ch));
            }
            lines.join("\n")
        }
        Format::Full => to_json(channels),
    }
}

pub fn format_messages(messages: &[Message], fmt: Format) -> String {
    match fmt {
        Format::Compact | Format::Expanded => {
            let max_text = if fmt == Format::Expanded { 0 } else { 200 };
            let mut lines: Vec<String> = Vec::with_capacity(messages.len() + 1);
            lines.push(format!("{} messages:", messages.len()));
            for msg in messages {
                lines.push(format_message_line(msg, max_text));
            }
            lines.join("\n")
        }
        Format::Full => to_json(messages),
    }
}

pub fn format_paginated(result: &Paginated<Message>, fmt: Format) -> String {
    match fmt {
        Format::Compact | Format::Expanded => {
            let max_text = if fmt == Format::Expanded { 0 } else { 200 };
            let mut lines: Vec<String> = Vec::with_capacity(result.items.len() + 2);
            lines.push(format!("{} messages:", result.items.len()));
            for msg in &result.items {
                lines.push(format_message_line(msg, max_text));
            }
            if result.has_more {
                if let Some(ref cursor) = result.next_cursor {
                    lines.push(format!("  ... more available (cursor: {cursor})"));
                }
            }
            lines.join("\n")
        }
        Format::Full => to_json(result),
    }
}

pub fn format_message(msg: &Message, fmt: Format) -> String {
    match fmt {
        Format::Compact => format_message_line(msg, 200),
        Format::Expanded => format_message_line(msg, 0),
        Format::Full => to_json(msg),
    }
}

pub fn format_chat_info(info: &ChatInfo, fmt: Format) -> String {
    match fmt {
        Format::Compact | Format::Expanded => {
            let typ = format_channel_type(&info.channel_type);
            let mut parts = vec![format!("{} [{}]", info.name, typ)];
            parts.push(format!("members:{}", info.member_count));
            if info.unread_count > 0 {
                parts.push(format!("unread:{}", info.unread_count));
            }
            if let Some(ref desc) = info.description {
                if !desc.is_empty() {
                    let d = if fmt == Format::Expanded { clean_text(desc) } else { truncate(desc, 100) };
                    parts.push(format!("desc:{d}"));
                }
            }
            if info.is_verified {
                parts.push("verified".to_string());
            }
            if info.is_scam {
                parts.push("SCAM".to_string());
            }
            if let Some(ref link) = info.invite_link {
                parts.push(format!("link:{link}"));
            }
            parts.push(format!("id:{}", info.id));
            parts.join(" | ")
        }
        Format::Full => to_json(info),
    }
}

pub fn format_labels(labels: &[String], fmt: Format) -> String {
    match fmt {
        Format::Compact | Format::Expanded => {
            let mut lines: Vec<String> = Vec::with_capacity(labels.len() + 1);
            lines.push(format!("{} labels:", labels.len()));
            for l in labels {
                lines.push(format!("  {l}"));
            }
            lines.join("\n")
        }
        Format::Full => to_json(labels),
    }
}

pub fn format_members(members: &[ChatMember], fmt: Format) -> String {
    match fmt {
        Format::Compact | Format::Expanded => {
            let mut lines: Vec<String> = Vec::with_capacity(members.len() + 1);
            lines.push(format!("{} members:", members.len()));
            for m in members {
                let username = m
                    .username
                    .as_ref()
                    .map(|u| format!(" (@{u})"))
                    .unwrap_or_default();
                lines.push(format!(
                    "  {}{} [{}] id:{}",
                    m.name, username, m.role, m.user_id
                ));
            }
            lines.join("\n")
        }
        Format::Full => to_json(members),
    }
}

fn format_channel_type(ct: &ChannelType) -> &str {
    match ct {
        ChannelType::Private => "private",
        ChannelType::Group => "group",
        ChannelType::Broadcast => "broadcast",
        ChannelType::Thread => "thread",
        ChannelType::Other(s) => s.as_str(),
    }
}

fn format_channel_line(ch: &Channel) -> String {
    let typ = format_channel_type(&ch.channel_type);
    let unread = if ch.unread_count > 0 {
        format!(" {} unread", ch.unread_count)
    } else {
        String::new()
    };
    let members = ch
        .member_count
        .map(|n| format!(" {n}m"))
        .unwrap_or_default();
    format!("  {} [{}]{}{} id:{}", ch.name, typ, unread, members, ch.id)
}

fn format_message_line(msg: &Message, max_text: usize) -> String {
    let ts = format_timestamp(msg.timestamp);
    let text = if max_text > 0 { truncate(&msg.text, max_text) } else { clean_text(&msg.text) };
    let attach = if msg.has_attachment { " +attach" } else { "" };
    let reply = msg
        .reply_to
        .as_ref()
        .map(|r| format!(" reply:{r}"))
        .unwrap_or_default();

    let mut extras = String::new();

    match msg.platform {
        Platform::Gmail => {
            if let Some(ref s) = msg.meta.subject {
                let subj = if max_text > 0 { truncate(s, 60) } else { clean_text(s) };
                extras.push_str(&format!(" subj:{subj}"));
            }
        }
        _ => {
            if let Some(true) = msg.meta.is_pinned {
                extras.push_str(" pinned");
            }
            if let Some(views) = msg.meta.views {
                extras.push_str(&format!(" {views}views"));
            }
            if msg.meta.edit_date.is_some() {
                extras.push_str(" edited");
            }
            if let Some(ref reactions) = msg.meta.reactions {
                let rxn: Vec<String> = reactions
                    .iter()
                    .map(|r| {
                        if r.count > 1 {
                            format!("{}x{}", r.emoji, r.count)
                        } else {
                            r.emoji.clone()
                        }
                    })
                    .collect();
                if !rxn.is_empty() {
                    extras.push_str(&format!(" [{}]", rxn.join("")));
                }
            }
        }
    }

    format!(
        "  [{ts}] {sender}: {text}{attach}{reply}{extras} (id:{id})",
        sender = msg.sender,
        id = msg.id,
    )
}

pub fn format_timestamp(ts: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(ts, 0);
    match dt {
        Some(dt) => {
            let utc = dt.with_timezone(&chrono::Utc);
            utc.format("%b %d %H:%M").to_string()
        }
        None => format!("{ts}"),
    }
}

fn clean_text(s: &str) -> String {
    let clean: String = s.chars().filter(|c| *c != '\r').collect();
    clean.replace('\n', " ")
}

fn truncate(s: &str, max: usize) -> String {
    let clean: String = s.chars().filter(|c| *c != '\r').collect();
    let oneline = clean.replace('\n', " ");
    if oneline.chars().count() > max {
        let truncated: String = oneline.chars().take(max).collect();
        format!("{truncated}...")
    } else {
        oneline
    }
}

fn to_json<T: serde::Serialize + ?Sized>(val: &T) -> String {
    serde_json::to_string(val).unwrap_or_else(|e| format!("serialization error: {e}"))
}
