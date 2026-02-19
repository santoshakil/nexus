use std::collections::HashMap;
use std::sync::Arc;

use nexus_domain::{Channel, Message, MessagingPort, Paginated, Platform, Profile};
use nexus_error::AgentError;
use tracing::info;

pub struct AgentService {
    adapters: HashMap<Platform, Arc<dyn MessagingPort>>,
}

impl Default for AgentService {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentService {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    pub fn register(&mut self, adapter: Arc<dyn MessagingPort>) {
        let platform = adapter.platform();
        info!(%platform, "registered adapter");
        self.adapters.insert(platform, adapter);
    }

    pub fn get(&self, platform: Platform) -> Result<&Arc<dyn MessagingPort>, AgentError> {
        self.adapters
            .get(&platform)
            .ok_or_else(|| AgentError::platform_not_available(format!("{platform}")))
    }

    pub fn available_platforms(&self) -> Vec<Platform> {
        self.adapters.keys().copied().collect()
    }

    pub async fn get_profile(&self, platform: Platform) -> Result<Profile, AgentError> {
        self.get(platform)?.get_profile().await
    }

    pub async fn list_channels(
        &self,
        platform: Platform,
        limit: usize,
    ) -> Result<Vec<Channel>, AgentError> {
        let limit = clamp(limit, 1, 100, 20);
        let channels = self.get(platform)?.list_channels(limit).await?;
        info!(%platform, count = channels.len(), "listed channels");
        Ok(channels)
    }

    pub async fn read_messages(
        &self,
        platform: Platform,
        channel: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        validate_not_empty(channel, "channel")?;
        let limit = clamp(limit, 1, 200, 20);
        let result = self.get(platform)?.read_messages(channel, limit, cursor).await?;
        info!(%platform, channel, count = result.items.len(), has_more = result.has_more, "read messages");
        Ok(result)
    }

    pub async fn send_message(
        &self,
        platform: Platform,
        channel: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<Message, AgentError> {
        validate_not_empty(channel, "channel")?;
        validate_not_empty(text, "text")?;
        let msg = self
            .get(platform)?
            .send_message(channel, text, reply_to)
            .await?;
        info!(%platform, channel, msg_id = %msg.id, "sent message");
        Ok(msg)
    }

    pub async fn search(
        &self,
        platform: Platform,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        validate_not_empty(query, "query")?;
        let limit = clamp(limit, 1, 100, 20);
        let result = self.get(platform)?.search(query, limit, cursor).await?;
        info!(%platform, query, count = result.items.len(), has_more = result.has_more, "searched");
        Ok(result)
    }
}

fn validate_not_empty(val: &str, name: &str) -> Result<(), AgentError> {
    if val.trim().is_empty() {
        return Err(AgentError::invalid_input(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn clamp(val: usize, min: usize, max: usize, default: usize) -> usize {
    if val == 0 {
        default
    } else {
        val.max(min).min(max)
    }
}
