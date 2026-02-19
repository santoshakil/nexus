use async_trait::async_trait;
use nexus_error::AgentError;

use crate::entities::{Channel, ChatInfo, ChatMember, Message, Paginated, Platform, Profile};

#[async_trait]
pub trait MessagingPort: Send + Sync {
    fn platform(&self) -> Platform;

    async fn get_profile(&self) -> Result<Profile, AgentError>;

    async fn list_channels(&self, limit: usize) -> Result<Vec<Channel>, AgentError>;

    async fn read_messages(
        &self,
        channel: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError>;

    async fn send_message(
        &self,
        channel: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<Message, AgentError>;

    async fn search(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError>;
}

#[async_trait]
pub trait TelegramExt: MessagingPort {
    async fn download_media(
        &self,
        chat: &str,
        msg_id: i64,
        path: &str,
    ) -> Result<String, AgentError>;

    async fn forward_message(
        &self,
        from_chat: &str,
        to_chat: &str,
        msg_id: i64,
    ) -> Result<Message, AgentError>;

    async fn edit_message(
        &self,
        chat: &str,
        msg_id: i64,
        text: &str,
    ) -> Result<Message, AgentError>;

    async fn delete_messages(
        &self,
        chat: &str,
        msg_ids: &[i64],
    ) -> Result<(), AgentError>;

    async fn pin_message(&self, chat: &str, msg_id: i64) -> Result<(), AgentError>;

    async fn unpin_message(&self, chat: &str, msg_id: i64) -> Result<(), AgentError>;

    async fn get_chat_info(&self, chat: &str) -> Result<ChatInfo, AgentError>;

    async fn mark_read(&self, chat: &str, msg_id: i64) -> Result<(), AgentError>;

    async fn get_message(&self, chat: &str, msg_id: i64) -> Result<Message, AgentError>;

    async fn send_media(
        &self,
        chat: &str,
        file_path: &str,
        caption: Option<&str>,
        media_type: Option<&str>,
    ) -> Result<Message, AgentError>;

    async fn react_message(
        &self,
        chat: &str,
        msg_id: i64,
        emoji: &str,
    ) -> Result<(), AgentError>;

    async fn search_chat(
        &self,
        chat: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Message>, AgentError>;

    async fn get_chat_members(
        &self,
        chat: &str,
        limit: usize,
    ) -> Result<Vec<ChatMember>, AgentError>;
}

#[async_trait]
pub trait GmailExt: MessagingPort {
    #[allow(clippy::too_many_arguments)]
    async fn send_email(
        &self,
        to: &[String],
        cc: &[String],
        bcc: &[String],
        subject: &str,
        body: &str,
        reply_to: Option<&str>,
        attachments: &[String],
    ) -> Result<Message, AgentError>;

    async fn archive(&self, thread_id: &str) -> Result<(), AgentError>;

    async fn list_labels(&self) -> Result<Vec<String>, AgentError>;

    async fn add_label(&self, thread_id: &str, label: &str) -> Result<(), AgentError>;

    async fn mark_read(&self, message_id: &str) -> Result<(), AgentError>;

    async fn mark_unread(&self, message_id: &str) -> Result<(), AgentError>;

    async fn star(&self, message_id: &str) -> Result<(), AgentError>;

    async fn unstar(&self, message_id: &str) -> Result<(), AgentError>;

    async fn move_to(&self, message_id: &str, folder: &str) -> Result<(), AgentError>;

    async fn trash(&self, message_id: &str) -> Result<(), AgentError>;

    async fn remove_label(&self, message_id: &str, label: &str) -> Result<(), AgentError>;

    async fn get_attachment(
        &self,
        message_id: &str,
        filename: &str,
        save_path: &str,
    ) -> Result<String, AgentError>;

    async fn create_draft(
        &self,
        to: &[String],
        subject: &str,
        body: &str,
    ) -> Result<Message, AgentError>;
}

#[async_trait]
pub trait WhatsAppExt: MessagingPort {
    async fn send_media(
        &self,
        chat: &str,
        file_path: &str,
        caption: &str,
    ) -> Result<Message, AgentError>;
}

#[async_trait]
pub trait SlackExt: MessagingPort {
    async fn set_status(&self, text: &str, emoji: &str) -> Result<(), AgentError>;

    async fn create_channel(
        &self,
        name: &str,
        is_private: bool,
    ) -> Result<Channel, AgentError>;

    async fn invite_to_channel(
        &self,
        channel: &str,
        user_id: &str,
    ) -> Result<(), AgentError>;

    async fn set_topic(&self, channel: &str, topic: &str) -> Result<(), AgentError>;

    async fn add_reaction(
        &self,
        channel: &str,
        msg_ts: &str,
        emoji: &str,
    ) -> Result<(), AgentError>;

    async fn remove_reaction(
        &self,
        channel: &str,
        msg_ts: &str,
        emoji: &str,
    ) -> Result<(), AgentError>;

    async fn upload_file(
        &self,
        channels: &[String],
        file_path: &str,
        title: Option<&str>,
    ) -> Result<String, AgentError>;

    async fn list_users(&self, limit: usize) -> Result<Vec<ChatMember>, AgentError>;

    async fn get_user_info(&self, user: &str) -> Result<Profile, AgentError>;
}

#[async_trait]
pub trait DiscordExt: MessagingPort {
    async fn list_guilds(&self) -> Result<Vec<Channel>, AgentError>;

    async fn list_guild_channels(
        &self,
        guild_id: &str,
    ) -> Result<Vec<Channel>, AgentError>;

    async fn create_thread(
        &self,
        channel: &str,
        name: &str,
        msg_id: Option<&str>,
    ) -> Result<Channel, AgentError>;

    async fn add_reaction(
        &self,
        channel: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<(), AgentError>;

    async fn remove_reaction(
        &self,
        channel: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<(), AgentError>;

    async fn pin_message(&self, channel: &str, msg_id: &str) -> Result<(), AgentError>;
}
