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
