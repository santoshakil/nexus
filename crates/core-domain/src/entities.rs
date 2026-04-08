use serde::Serialize;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Telegram,
    Gmail,
    WhatsApp,
    Slack,
    Discord,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Telegram => write!(f, "telegram"),
            Self::Gmail => write!(f, "gmail"),
            Self::WhatsApp => write!(f, "whatsapp"),
            Self::Slack => write!(f, "slack"),
            Self::Discord => write!(f, "discord"),
        }
    }
}

impl FromStr for Platform {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "telegram" | "tg" => Ok(Self::Telegram),
            "gmail" | "email" | "mail" => Ok(Self::Gmail),
            "whatsapp" | "wa" => Ok(Self::WhatsApp),
            "slack" | "sl" => Ok(Self::Slack),
            "discord" | "dc" => Ok(Self::Discord),
            _ => Err(format!("unknown platform: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Private,
    Group,
    Broadcast,
    Thread,
    Other(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub channel_type: ChannelType,
    pub unread_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_date: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub platform: Platform,
    pub channel_id: String,
    pub sender: String,
    pub text: String,
    pub timestamp: i64,
    pub has_attachment: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "MessageMeta::is_empty")]
    pub meta: MessageMeta,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MessageMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactions: Option<Vec<Reaction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub views: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_pinned: Option<bool>,
}

impl MessageMeta {
    pub fn is_empty(&self) -> bool {
        self.subject.is_none()
            && self.cc.is_none()
            && self.bcc.is_none()
            && self.labels.is_none()
            && self.media_type.is_none()
            && self.forward_from.is_none()
            && self.reactions.is_none()
            && self.views.is_none()
            && self.edit_date.is_none()
            && self.is_pinned.is_none()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Reaction {
    pub emoji: String,
    pub count: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub platform: Platform,
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatInfo {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub channel_type: ChannelType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub member_count: i32,
    pub unread_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_link: Option<String>,
    pub is_verified: bool,
    pub is_scam: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Paginated<T: serde::Serialize> {
    pub items: Vec<T>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
    Restricted,
    Banned,
}

impl fmt::Display for MemberRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner => write!(f, "owner"),
            Self::Admin => write!(f, "admin"),
            Self::Member => write!(f, "member"),
            Self::Restricted => write!(f, "restricted"),
            Self::Banned => write!(f, "banned"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMember {
    pub user_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub role: MemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_date: Option<i64>,
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({:?}, unread: {})",
            self.platform, self.name, self.channel_type, self.unread_count
        )
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let preview: String = self.text.chars().take(80).collect();
        let suffix = if self.text.chars().count() > 80 {
            "..."
        } else {
            ""
        };
        write!(
            f,
            "[{}:{}] {}: {preview}{suffix}",
            self.platform, self.id, self.sender
        )
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn platform_from_str() {
        assert_eq!("telegram".parse::<Platform>().ok(), Some(Platform::Telegram));
        assert_eq!("tg".parse::<Platform>().ok(), Some(Platform::Telegram));
        assert_eq!("gmail".parse::<Platform>().ok(), Some(Platform::Gmail));
        assert_eq!("email".parse::<Platform>().ok(), Some(Platform::Gmail));
        assert_eq!("mail".parse::<Platform>().ok(), Some(Platform::Gmail));
        assert_eq!("whatsapp".parse::<Platform>().ok(), Some(Platform::WhatsApp));
        assert_eq!("wa".parse::<Platform>().ok(), Some(Platform::WhatsApp));
        assert_eq!("slack".parse::<Platform>().ok(), Some(Platform::Slack));
        assert_eq!("sl".parse::<Platform>().ok(), Some(Platform::Slack));
        assert_eq!("discord".parse::<Platform>().ok(), Some(Platform::Discord));
        assert_eq!("dc".parse::<Platform>().ok(), Some(Platform::Discord));
    }

    #[test]
    fn platform_from_str_case_insensitive() {
        assert_eq!("TELEGRAM".parse::<Platform>().ok(), Some(Platform::Telegram));
        assert_eq!("Gmail".parse::<Platform>().ok(), Some(Platform::Gmail));
        assert_eq!("WhatsApp".parse::<Platform>().ok(), Some(Platform::WhatsApp));
    }

    #[test]
    fn platform_from_str_unknown() {
        assert!("twitter".parse::<Platform>().is_err());
        assert!("".parse::<Platform>().is_err());
        assert!("signal".parse::<Platform>().is_err());
    }

    #[test]
    fn platform_display() {
        assert_eq!(Platform::Telegram.to_string(), "telegram");
        assert_eq!(Platform::Gmail.to_string(), "gmail");
        assert_eq!(Platform::WhatsApp.to_string(), "whatsapp");
        assert_eq!(Platform::Slack.to_string(), "slack");
        assert_eq!(Platform::Discord.to_string(), "discord");
    }

    #[test]
    fn platform_roundtrip() {
        for p in [Platform::Telegram, Platform::Gmail, Platform::WhatsApp, Platform::Slack, Platform::Discord] {
            let s = p.to_string();
            let parsed: Platform = s.parse().unwrap_or_else(|_| panic!("failed to parse {s}"));
            assert_eq!(p, parsed);
        }
    }

    #[test]
    fn message_meta_is_empty() {
        assert!(MessageMeta::default().is_empty());

        let meta = MessageMeta {
            subject: Some("test".to_string()),
            ..Default::default()
        };
        assert!(!meta.is_empty());
    }

    #[test]
    fn channel_display() {
        let ch = Channel {
            id: "123".to_string(),
            platform: Platform::Telegram,
            name: "Test Group".to_string(),
            channel_type: ChannelType::Group,
            unread_count: 5,
            description: None,
            member_count: None,
            last_message_date: None,
        };
        let display = ch.to_string();
        assert!(display.contains("Test Group"));
        assert!(display.contains("unread: 5"));
    }

    #[test]
    fn message_display_truncates() {
        let msg = Message {
            id: "1".to_string(),
            platform: Platform::Telegram,
            channel_id: "ch1".to_string(),
            sender: "Alice".to_string(),
            text: "x".repeat(100),
            timestamp: 0,
            has_attachment: false,
            reply_to: None,
            meta: MessageMeta::default(),
        };
        let display = msg.to_string();
        assert!(display.contains("..."));
        assert!(display.len() < 200);
    }

    #[test]
    fn message_display_short_text() {
        let msg = Message {
            id: "1".to_string(),
            platform: Platform::Gmail,
            channel_id: "INBOX".to_string(),
            sender: "Bob".to_string(),
            text: "Hello".to_string(),
            timestamp: 0,
            has_attachment: false,
            reply_to: None,
            meta: MessageMeta::default(),
        };
        let display = msg.to_string();
        assert!(!display.contains("..."));
        assert!(display.contains("Hello"));
    }

    #[test]
    fn member_role_display() {
        assert_eq!(MemberRole::Owner.to_string(), "owner");
        assert_eq!(MemberRole::Admin.to_string(), "admin");
        assert_eq!(MemberRole::Member.to_string(), "member");
        assert_eq!(MemberRole::Restricted.to_string(), "restricted");
        assert_eq!(MemberRole::Banned.to_string(), "banned");
    }
}
