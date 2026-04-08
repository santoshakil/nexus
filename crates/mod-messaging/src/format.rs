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

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_domain::{ChatMember, MemberRole, MessageMeta, Reaction};

    fn make_msg(id: &str, sender: &str, text: &str, ts: i64) -> Message {
        Message {
            id: id.to_string(),
            platform: Platform::Telegram,
            channel_id: "ch1".to_string(),
            sender: sender.to_string(),
            text: text.to_string(),
            timestamp: ts,
            has_attachment: false,
            reply_to: None,
            meta: MessageMeta::default(),
        }
    }

    fn make_channel(id: &str, name: &str, unread: i32) -> Channel {
        Channel {
            id: id.to_string(),
            platform: Platform::Telegram,
            name: name.to_string(),
            channel_type: ChannelType::Group,
            unread_count: unread,
            description: None,
            member_count: Some(42),
            last_message_date: None,
        }
    }

    #[test]
    fn format_parse() {
        assert_eq!(Format::parse(None), Format::Compact);
        assert_eq!(Format::parse(Some("compact")), Format::Compact);
        assert_eq!(Format::parse(Some("expanded")), Format::Expanded);
        assert_eq!(Format::parse(Some("full")), Format::Full);
        assert_eq!(Format::parse(Some("unknown")), Format::Compact);
    }

    #[test]
    fn format_profile_compact() {
        let p = Profile {
            platform: Platform::Telegram,
            id: "123".to_string(),
            name: "Alice".to_string(),
            username: Some("alice".to_string()),
            email: None,
            phone: Some("1234567890".to_string()),
        };
        let out = format_profile(&p, Format::Compact);
        assert!(out.contains("Alice"));
        assert!(out.contains("(@alice)"));
        assert!(out.contains("id:123"));
        assert!(out.contains("+1234567890"));
        assert!(out.contains("[telegram]"));
    }

    #[test]
    fn format_profile_full_is_json() {
        let p = Profile {
            platform: Platform::Gmail,
            id: "test@mail.com".to_string(),
            name: "Test".to_string(),
            username: None,
            email: Some("test@mail.com".to_string()),
            phone: None,
        };
        let out = format_profile(&p, Format::Full);
        assert!(out.starts_with('{'));
        assert!(out.contains("\"name\":\"Test\""));
    }

    #[test]
    fn format_channels_compact() {
        let chs = vec![
            make_channel("1", "General", 3),
            make_channel("2", "Random", 0),
        ];
        let out = format_channels(&chs, Format::Compact);
        assert!(out.contains("2 channels:"));
        assert!(out.contains("General"));
        assert!(out.contains("3 unread"));
        assert!(out.contains("Random"));
    }

    #[test]
    fn format_messages_compact_truncates() {
        let long_text = "a".repeat(300);
        let msgs = vec![make_msg("1", "Alice", &long_text, 1700000000)];
        let out = format_messages(&msgs, Format::Compact);
        assert!(out.contains("..."));
        assert!(out.len() < long_text.len() + 200);
    }

    #[test]
    fn format_messages_expanded_no_truncate() {
        let long_text = "b".repeat(300);
        let msgs = vec![make_msg("1", "Bob", &long_text, 1700000000)];
        let out = format_messages(&msgs, Format::Expanded);
        assert!(!out.contains("..."));
        assert!(out.contains(&"b".repeat(300)));
    }

    #[test]
    fn format_paginated_with_cursor() {
        let result = Paginated {
            items: vec![make_msg("1", "A", "hello", 1700000000)],
            has_more: true,
            next_cursor: Some("tg:12345".to_string()),
        };
        let out = format_paginated(&result, Format::Compact);
        assert!(out.contains("1 messages:"));
        assert!(out.contains("cursor: tg:12345"));
    }

    #[test]
    fn format_paginated_no_more() {
        let result = Paginated {
            items: vec![make_msg("1", "A", "hi", 1700000000)],
            has_more: false,
            next_cursor: None,
        };
        let out = format_paginated(&result, Format::Compact);
        assert!(!out.contains("cursor"));
    }

    #[test]
    fn format_message_with_attachment() {
        let mut msg = make_msg("1", "Alice", "[Photo]", 1700000000);
        msg.has_attachment = true;
        let out = format_message(&msg, Format::Compact);
        assert!(out.contains("+attach"));
    }

    #[test]
    fn format_message_with_reply() {
        let mut msg = make_msg("1", "Alice", "reply text", 1700000000);
        msg.reply_to = Some("99".to_string());
        let out = format_message(&msg, Format::Compact);
        assert!(out.contains("reply:99"));
    }

    #[test]
    fn format_gmail_message_with_subject() {
        let mut msg = make_msg("1", "alice@test.com", "body", 1700000000);
        msg.platform = Platform::Gmail;
        msg.meta.subject = Some("Important Email".to_string());
        let out = format_message(&msg, Format::Compact);
        assert!(out.contains("subj:Important Email"));
    }

    #[test]
    fn format_message_with_reactions() {
        let mut msg = make_msg("1", "Alice", "cool", 1700000000);
        msg.meta.reactions = Some(vec![
            Reaction { emoji: "👍".to_string(), count: 3 },
            Reaction { emoji: "❤️".to_string(), count: 1 },
        ]);
        let out = format_message(&msg, Format::Compact);
        assert!(out.contains("👍x3"));
        assert!(out.contains("❤\u{fe0f}"));
    }

    #[test]
    fn format_message_pinned_and_edited() {
        let mut msg = make_msg("1", "Alice", "pinned msg", 1700000000);
        msg.meta.is_pinned = Some(true);
        msg.meta.edit_date = Some(1700000100);
        let out = format_message(&msg, Format::Compact);
        assert!(out.contains("pinned"));
        assert!(out.contains("edited"));
    }

    #[test]
    fn format_timestamp_valid() {
        let ts = format_timestamp(1700000000);
        assert!(ts.contains("Nov"));
        assert!(ts.contains("14"));
    }

    #[test]
    fn format_timestamp_zero() {
        let ts = format_timestamp(0);
        assert!(ts.contains("Jan") || ts.contains("1970") || ts == "0");
    }

    #[test]
    fn format_chat_info_compact() {
        let info = ChatInfo {
            id: "123".to_string(),
            platform: Platform::Telegram,
            name: "Dev Group".to_string(),
            channel_type: ChannelType::Group,
            description: Some("A dev group".to_string()),
            member_count: 150,
            unread_count: 5,
            invite_link: Some("https://t.me/+abc".to_string()),
            is_verified: true,
            is_scam: false,
        };
        let out = format_chat_info(&info, Format::Compact);
        assert!(out.contains("Dev Group"));
        assert!(out.contains("members:150"));
        assert!(out.contains("unread:5"));
        assert!(out.contains("verified"));
        assert!(out.contains("link:"));
        assert!(out.contains("id:123"));
    }

    #[test]
    fn format_members_compact() {
        let members = vec![
            ChatMember {
                user_id: "1".to_string(),
                name: "Alice".to_string(),
                username: Some("alice".to_string()),
                role: MemberRole::Admin,
                joined_date: None,
            },
            ChatMember {
                user_id: "2".to_string(),
                name: "Bob".to_string(),
                username: None,
                role: MemberRole::Member,
                joined_date: None,
            },
        ];
        let out = format_members(&members, Format::Compact);
        assert!(out.contains("2 members:"));
        assert!(out.contains("Alice (@alice) [admin]"));
        assert!(out.contains("Bob [member]"));
    }

    #[test]
    fn format_labels_compact() {
        let labels = vec!["INBOX".to_string(), "[Gmail]/Sent".to_string()];
        let out = format_labels(&labels, Format::Compact);
        assert!(out.contains("2 labels:"));
        assert!(out.contains("INBOX"));
        assert!(out.contains("[Gmail]/Sent"));
    }
}
