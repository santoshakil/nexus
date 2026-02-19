use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use imap_proto::types::Address as ImapAddress;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use mailparse::MailHeaderMap;
use nexus_domain::*;
use nexus_error::AgentError;
use tracing::{debug, info, warn};

pub struct GmailConfig {
    pub email: String,
    pub app_password: String,
}

pub struct GmailAdapter {
    config: Arc<GmailConfig>,
    session: Arc<std::sync::Mutex<Option<ImapSession>>>,
}

impl GmailAdapter {
    pub fn new(config: GmailConfig) -> Self {
        Self {
            config: Arc::new(config),
            session: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn take_session(&self) -> Option<ImapSession> {
        self.session
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    async fn with_session<F, R>(&self, f: F) -> Result<R, AgentError>
    where
        F: FnOnce(&mut ImapSession) -> Result<R, AgentError> + Send + 'static,
        R: Send + 'static,
    {
        let config = self.config.clone();
        let cached = self.take_session();
        let pool = self.session.clone();
        tokio::task::spawn_blocking(move || {
            let mut session = get_or_connect(cached, &config)?;
            let result = f(&mut session);
            if let Ok(mut guard) = pool.lock() {
                *guard = Some(session);
            }
            result
        })
        .await
        .map_err(|e| AgentError::internal(format!("spawn: {e}")))?
    }
}

fn get_or_connect(
    cached: Option<ImapSession>,
    config: &GmailConfig,
) -> Result<ImapSession, AgentError> {
    if let Some(mut s) = cached {
        if s.noop().is_ok() {
            return Ok(s);
        }
        debug!("cached IMAP session stale, reconnecting");
    }
    imap_connect(config)
}

type ImapSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

fn imap_connect(config: &GmailConfig) -> Result<ImapSession, AgentError> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| AgentError::network(format!("TLS init: {e}")))?;

    let client = imap::connect(("imap.gmail.com", 993), "imap.gmail.com", &tls)
        .map_err(|e| AgentError::network(format!("IMAP connect: {e}")))?;

    client
        .login(&config.email, &config.app_password)
        .map_err(|(e, _)| AgentError::auth(format!("IMAP login: {e}")))
}

fn decode_mime_str(raw: &[u8]) -> String {
    let raw_str = String::from_utf8_lossy(raw).to_string();
    if !raw_str.contains("=?") {
        return raw_str;
    }
    let fake = format!("X: {raw_str}");
    match mailparse::parse_header(fake.as_bytes()) {
        Ok((hdr, _)) => hdr.get_value(),
        Err(_) => raw_str,
    }
}

fn format_imap_addr(addr: &ImapAddress) -> String {
    let name = addr
        .name
        .as_ref()
        .map(|n| decode_mime_str(n))
        .unwrap_or_default();
    let mailbox = addr
        .mailbox
        .as_ref()
        .and_then(|m| std::str::from_utf8(m).ok())
        .unwrap_or("");
    let host = addr
        .host
        .as_ref()
        .and_then(|h| std::str::from_utf8(h).ok())
        .unwrap_or("");

    let email = if mailbox.is_empty() && host.is_empty() {
        return if name.is_empty() {
            "unknown".to_string()
        } else {
            name
        };
    } else if host.is_empty() {
        mailbox.to_string()
    } else {
        format!("{mailbox}@{host}")
    };

    if name.is_empty() {
        email
    } else {
        format!("{name} <{email}>")
    }
}

fn addrs_to_vec(addrs: &Option<Vec<ImapAddress>>) -> Vec<String> {
    addrs
        .as_ref()
        .map(|list| list.iter().map(format_imap_addr).collect())
        .unwrap_or_default()
}

fn parse_imap_date(bytes: Option<&[u8]>) -> i64 {
    bytes
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| mailparse::dateparse(s).ok())
        .unwrap_or(0)
}

fn extract_text_from_parsed(parsed: &mailparse::ParsedMail<'_>) -> String {
    if let Some(text) = find_text_part(parsed, "text/plain") {
        return text;
    }
    if let Some(html) = find_text_part(parsed, "text/html") {
        return strip_html(&html);
    }
    parsed.get_body().unwrap_or_default()
}

fn find_text_part(parsed: &mailparse::ParsedMail<'_>, target: &str) -> Option<String> {
    if parsed.subparts.is_empty() {
        let ct = parsed
            .get_headers()
            .get_first_value("Content-Type")
            .unwrap_or_default();
        if ct.starts_with(target) {
            return parsed.get_body().ok();
        }
        return None;
    }
    for part in &parsed.subparts {
        if let Some(text) = find_text_part(part, target) {
            return Some(text);
        }
    }
    None
}

fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn check_attachments(parsed: &mailparse::ParsedMail<'_>) -> bool {
    for part in &parsed.subparts {
        let disp = part
            .get_headers()
            .get_first_value("Content-Disposition")
            .unwrap_or_default();
        if disp.starts_with("attachment") {
            return true;
        }
        if check_attachments(part) {
            return true;
        }
    }
    false
}

fn find_attachment(parsed: &mailparse::ParsedMail<'_>, target: &str) -> Option<Vec<u8>> {
    for part in &parsed.subparts {
        let disp = part.get_content_disposition();
        let name = disp
            .params
            .get("filename")
            .or_else(|| disp.params.get("name"));
        if let Some(n) = name {
            if n == target {
                return part.get_body_raw().ok();
            }
        }
        let ct_params = part
            .get_headers()
            .get_first_value("Content-Type")
            .unwrap_or_default();
        let ct_disp = mailparse::parse_content_disposition(&ct_params);
        if let Some(n) = ct_disp.params.get("name") {
            if n == target {
                return part.get_body_raw().ok();
            }
        }
        if let Some(found) = find_attachment(part, target) {
            return Some(found);
        }
    }
    None
}

fn fetch_to_message(fetch: &imap::types::Fetch, folder: &str) -> Option<Message> {
    let envelope = fetch.envelope()?;
    let uid = fetch.uid.unwrap_or(0);

    let message_id = envelope
        .message_id
        .as_ref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("")
        .to_string();

    let id = if message_id.is_empty() {
        format!("uid:{uid}")
    } else {
        message_id
    };

    let sender = addrs_to_vec(&envelope.from)
        .into_iter()
        .next()
        .unwrap_or_else(|| "unknown".to_string());

    let subject = envelope
        .subject
        .as_ref()
        .map(|b| decode_mime_str(b))
        .unwrap_or_default();

    let timestamp = parse_imap_date(envelope.date);

    let reply_to = envelope
        .in_reply_to
        .as_ref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(|s| s.to_string());

    let body_raw = fetch.body().unwrap_or(&[]);
    let (text, attachment) = match mailparse::parse_mail(body_raw) {
        Ok(parsed) => {
            let t = extract_text_from_parsed(&parsed);
            let a = check_attachments(&parsed);
            (t, a)
        }
        Err(e) => {
            warn!("mailparse error: {e}");
            (String::from_utf8_lossy(body_raw).to_string(), false)
        }
    };

    let cc = addrs_to_vec(&envelope.cc);
    let bcc = addrs_to_vec(&envelope.bcc);

    Some(Message {
        id,
        platform: Platform::Gmail,
        channel_id: folder.to_string(),
        sender,
        text,
        timestamp,
        has_attachment: attachment,
        reply_to,
        meta: MessageMeta {
            subject: if subject.is_empty() {
                None
            } else {
                Some(subject)
            },
            cc: if cc.is_empty() { None } else { Some(cc) },
            bcc: if bcc.is_empty() { None } else { Some(bcc) },
            ..Default::default()
        },
    })
}

fn folder_to_channel(name: &imap::types::Name) -> Channel {
    let raw_name = name.name();
    let display = match raw_name {
        "INBOX" => "Inbox".to_string(),
        "[Gmail]/All Mail" => "All Mail".to_string(),
        "[Gmail]/Sent Mail" => "Sent".to_string(),
        "[Gmail]/Drafts" => "Drafts".to_string(),
        "[Gmail]/Spam" => "Spam".to_string(),
        "[Gmail]/Trash" => "Trash".to_string(),
        "[Gmail]/Starred" => "Starred".to_string(),
        "[Gmail]/Important" => "Important".to_string(),
        other => other.to_string(),
    };

    let channel_type = match raw_name {
        "INBOX" => ChannelType::Private,
        _ if raw_name.starts_with("[Gmail]/") => ChannelType::Other("system".to_string()),
        _ => ChannelType::Other("label".to_string()),
    };

    Channel {
        id: raw_name.to_string(),
        platform: Platform::Gmail,
        name: display,
        channel_type,
        unread_count: 0,
        description: None,
        member_count: None,
        last_message_date: None,
    }
}

fn guess_content_type(path: &Path) -> ContentType {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "zip" => "application/zip",
        "doc" | "docx" => "application/msword",
        "txt" => "text/plain",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    };

    mime.parse().unwrap_or(ContentType::TEXT_PLAIN)
}

fn imap_find_message(
    session: &mut ImapSession,
    message_id: &str,
) -> Result<Vec<u32>, AgentError> {
    let search_query = format!("HEADER Message-ID \"{message_id}\"");
    let results = session
        .search(&search_query)
        .map_err(|e| AgentError::network(format!("IMAP SEARCH: {e}")))?;

    if results.is_empty() {
        return Err(AgentError::not_found(format!(
            "message not found: {message_id}"
        )));
    }

    Ok(results.into_iter().collect())
}

fn uid_str(uids: &[u32]) -> String {
    uids.iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[async_trait]
impl MessagingPort for GmailAdapter {
    fn platform(&self) -> Platform {
        Platform::Gmail
    }

    async fn get_profile(&self) -> Result<Profile, AgentError> {
        let email = self.config.email.clone();
        let name = email.split('@').next().unwrap_or(&email).to_string();

        Ok(Profile {
            platform: Platform::Gmail,
            id: email.clone(),
            name,
            username: None,
            email: Some(email),
            phone: None,
        })
    }

    async fn list_channels(&self, limit: usize) -> Result<Vec<Channel>, AgentError> {
        self.with_session(move |session| {
            let names = session
                .list(Some(""), Some("*"))
                .map_err(|e| AgentError::network(format!("IMAP LIST: {e}")))?;

            let mut channels: Vec<Channel> = names.iter().map(folder_to_channel).collect();

            let skip = ["[Gmail]", "[Gmail]/All Mail"];
            channels.retain(|c| !skip.contains(&c.id.as_str()));

            if session.select("INBOX").is_ok() {
                if let Some(ch) = channels.iter_mut().find(|c| c.id == "INBOX") {
                    ch.unread_count = session
                        .search("UNSEEN")
                        .map(|r| r.len() as i32)
                        .unwrap_or(0);
                }
            }

            channels.truncate(limit);
            Ok(channels)
        })
        .await
    }

    async fn read_messages(
        &self,
        channel: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        let folder = channel.to_string();
        let limit = limit.min(200);
        let cursor_seq = cursor
            .and_then(|c| c.strip_prefix("gm:"))
            .and_then(|s| s.parse::<u32>().ok());

        self.with_session(move |session| {
            let mailbox = session
                .select(&folder)
                .map_err(|e| AgentError::not_found(format!("folder '{folder}': {e}")))?;

            let total = mailbox.exists;
            if total == 0 {
                return Ok(Paginated {
                    items: vec![],
                    has_more: false,
                    next_cursor: None,
                });
            }

            let end = cursor_seq.map(|s| s.saturating_sub(1)).unwrap_or(total);
            if end == 0 {
                return Ok(Paginated {
                    items: vec![],
                    has_more: false,
                    next_cursor: None,
                });
            }

            let start = end.saturating_sub(limit as u32) + 1;
            let range = format!("{start}:{end}");

            let fetches = session
                .fetch(&range, "(UID ENVELOPE BODY[])")
                .map_err(|e| AgentError::network(format!("IMAP FETCH: {e}")))?;

            let mut messages: Vec<Message> = fetches
                .iter()
                .filter_map(|f| fetch_to_message(f, &folder))
                .collect();

            messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            messages.truncate(limit);

            let has_more = start > 1;
            let next_cursor = if has_more {
                Some(format!("gm:{start}"))
            } else {
                None
            };

            Ok(Paginated {
                items: messages,
                has_more,
                next_cursor,
            })
        })
        .await
    }

    async fn send_message(
        &self,
        channel: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<Message, AgentError> {
        let _ = reply_to;
        self.send_email(
            &[channel.to_string()],
            &[],
            &[],
            "(no subject)",
            text,
            None,
            &[],
        )
        .await
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        let query = query.to_string();
        let limit = limit.min(100);
        let cursor_offset = cursor
            .and_then(|c| c.strip_prefix("gm:"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT INBOX: {e}")))?;

            let sanitized = query.replace(['"', '\\'], "");
            let search_result = session
                .search(&query)
                .or_else(|_| {
                    debug!("raw IMAP search failed, trying TEXT search");
                    session.search(format!("TEXT \"{sanitized}\""))
                })
                .map_err(|e| AgentError::network(format!("IMAP SEARCH: {e}")))?;

            if search_result.is_empty() {
                return Ok(Paginated {
                    items: vec![],
                    has_more: false,
                    next_cursor: None,
                });
            }

            let mut uids: Vec<u32> = search_result.into_iter().collect();
            uids.sort_unstable();
            uids.reverse();

            let total_results = uids.len();
            let uids: Vec<u32> = uids.into_iter().skip(cursor_offset).take(limit).collect();

            let uid_list = uid_str(&uids);

            let fetches = session
                .fetch(&uid_list, "(UID ENVELOPE BODY[])")
                .map_err(|e| AgentError::network(format!("IMAP FETCH: {e}")))?;

            let mut messages: Vec<Message> = fetches
                .iter()
                .filter_map(|f| fetch_to_message(f, "INBOX"))
                .collect();

            messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            messages.truncate(limit);

            let next_offset = cursor_offset + uids.len();
            let has_more = next_offset < total_results;
            let next_cursor = if has_more {
                Some(format!("gm:{next_offset}"))
            } else {
                None
            };

            Ok(Paginated {
                items: messages,
                has_more,
                next_cursor,
            })
        })
        .await
    }
}

#[async_trait]
impl GmailExt for GmailAdapter {
    async fn send_email(
        &self,
        to: &[String],
        cc: &[String],
        bcc: &[String],
        subject: &str,
        body: &str,
        reply_to: Option<&str>,
        attachments: &[String],
    ) -> Result<Message, AgentError> {
        if to.is_empty() {
            return Err(AgentError::invalid_input("'to' cannot be empty"));
        }

        let from_mailbox: Mailbox = self
            .config
            .email
            .parse()
            .map_err(|e| AgentError::internal(format!("invalid from: {e}")))?;

        let mut builder = lettre::Message::builder()
            .from(from_mailbox)
            .subject(subject);

        for addr in to {
            let mbox: Mailbox = addr
                .parse()
                .map_err(|e| AgentError::invalid_input(format!("invalid to '{addr}': {e}")))?;
            builder = builder.to(mbox);
        }
        for addr in cc {
            let mbox: Mailbox = addr
                .parse()
                .map_err(|e| AgentError::invalid_input(format!("invalid cc '{addr}': {e}")))?;
            builder = builder.cc(mbox);
        }
        for addr in bcc {
            let mbox: Mailbox = addr
                .parse()
                .map_err(|e| AgentError::invalid_input(format!("invalid bcc '{addr}': {e}")))?;
            builder = builder.bcc(mbox);
        }

        if let Some(reply_id) = reply_to {
            builder = builder.in_reply_to(reply_id.to_string());
            builder = builder.references(reply_id.to_string());
        }

        let text_part = SinglePart::builder()
            .content_type(ContentType::TEXT_PLAIN)
            .body(body.to_string());

        let email = if attachments.is_empty() {
            builder
                .singlepart(text_part)
                .map_err(|e| AgentError::internal(format!("email build: {e}")))?
        } else {
            let mut multi = MultiPart::mixed().singlepart(text_part);

            for path_str in attachments {
                for component in Path::new(path_str).components() {
                    if matches!(component, std::path::Component::ParentDir) {
                        return Err(AgentError::invalid_input(
                            "attachment path must not contain '..' components",
                        ));
                    }
                }

                let file_data = tokio::fs::read(path_str)
                    .await
                    .map_err(|e| AgentError::internal(format!("read {path_str}: {e}")))?;

                let filename = Path::new(path_str)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("attachment")
                    .to_string();

                let ct = guess_content_type(Path::new(path_str));
                let attachment = Attachment::new(filename).body(file_data, ct);
                multi = multi.singlepart(attachment);
            }

            builder
                .multipart(multi)
                .map_err(|e| AgentError::internal(format!("email build: {e}")))?
        };

        let message_id = email
            .headers()
            .get_raw("Message-ID")
            .unwrap_or_default()
            .to_string();

        let creds = Credentials::new(
            self.config.email.clone(),
            self.config.app_password.clone(),
        );

        let transport: AsyncSmtpTransport<Tokio1Executor> =
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay("smtp.gmail.com")
                .map_err(|e| AgentError::network(format!("SMTP relay: {e}")))?
                .credentials(creds)
                .build();

        transport
            .send(email)
            .await
            .map_err(|e| AgentError::network(format!("SMTP send: {e}")))?;

        info!(
            to = ?to,
            subject,
            attachments = attachments.len(),
            "email sent"
        );

        Ok(Message {
            id: message_id,
            platform: Platform::Gmail,
            channel_id: "[Gmail]/Sent Mail".to_string(),
            sender: self.config.email.clone(),
            text: body.to_string(),
            timestamp: now_ts(),
            has_attachment: !attachments.is_empty(),
            reply_to: reply_to.map(|s| s.to_string()),
            meta: MessageMeta {
                subject: Some(subject.to_string()),
                cc: if cc.is_empty() {
                    None
                } else {
                    Some(cc.to_vec())
                },
                bcc: if bcc.is_empty() {
                    None
                } else {
                    Some(bcc.to_vec())
                },
                ..Default::default()
            },
        })
    }

    async fn archive(&self, thread_id: &str) -> Result<(), AgentError> {
        let thread_id = thread_id.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &thread_id)?;
            let ids = uid_str(&uids);

            session
                .mv(&ids, "[Gmail]/All Mail")
                .map_err(|e| AgentError::network(format!("IMAP MOVE: {e}")))?;

            info!(thread_id, "archived (moved to All Mail)");
            Ok(())
        })
        .await
    }

    async fn list_labels(&self) -> Result<Vec<String>, AgentError> {
        self.with_session(|session| {
            let names = session
                .list(Some(""), Some("*"))
                .map_err(|e| AgentError::network(format!("IMAP LIST: {e}")))?;

            let labels: Vec<String> = names.iter().map(|n| n.name().to_string()).collect();
            Ok(labels)
        })
        .await
    }

    async fn add_label(&self, thread_id: &str, label: &str) -> Result<(), AgentError> {
        let thread_id = thread_id.to_string();
        let label = label.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &thread_id)?;
            let ids = uid_str(&uids);

            session
                .copy(&ids, &label)
                .map_err(|e| AgentError::network(format!("IMAP COPY to '{label}': {e}")))?;

            info!(thread_id, label, "label added");
            Ok(())
        })
        .await
    }

    async fn mark_read(&self, message_id: &str) -> Result<(), AgentError> {
        let message_id = message_id.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &message_id)?;
            let ids = uid_str(&uids);

            session
                .store(&ids, "+FLAGS (\\Seen)")
                .map_err(|e| AgentError::network(format!("IMAP STORE: {e}")))?;

            info!(message_id, "marked as read");
            Ok(())
        })
        .await
    }

    async fn mark_unread(&self, message_id: &str) -> Result<(), AgentError> {
        let message_id = message_id.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &message_id)?;
            let ids = uid_str(&uids);

            session
                .store(&ids, "-FLAGS (\\Seen)")
                .map_err(|e| AgentError::network(format!("IMAP STORE: {e}")))?;

            info!(message_id, "marked as unread");
            Ok(())
        })
        .await
    }

    async fn star(&self, message_id: &str) -> Result<(), AgentError> {
        let message_id = message_id.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &message_id)?;
            let ids = uid_str(&uids);

            session
                .store(&ids, "+FLAGS (\\Flagged)")
                .map_err(|e| AgentError::network(format!("IMAP STORE: {e}")))?;

            info!(message_id, "starred");
            Ok(())
        })
        .await
    }

    async fn unstar(&self, message_id: &str) -> Result<(), AgentError> {
        let message_id = message_id.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &message_id)?;
            let ids = uid_str(&uids);

            session
                .store(&ids, "-FLAGS (\\Flagged)")
                .map_err(|e| AgentError::network(format!("IMAP STORE: {e}")))?;

            info!(message_id, "unstarred");
            Ok(())
        })
        .await
    }

    async fn move_to(&self, message_id: &str, folder: &str) -> Result<(), AgentError> {
        let message_id = message_id.to_string();
        let folder = folder.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &message_id)?;
            let ids = uid_str(&uids);

            session
                .copy(&ids, &folder)
                .map_err(|e| AgentError::network(format!("IMAP COPY to '{folder}': {e}")))?;

            session
                .store(&ids, "+FLAGS (\\Deleted)")
                .map_err(|e| AgentError::network(format!("IMAP STORE: {e}")))?;

            session
                .expunge()
                .map_err(|e| AgentError::network(format!("IMAP EXPUNGE: {e}")))?;

            info!(message_id, folder, "moved");
            Ok(())
        })
        .await
    }

    async fn trash(&self, message_id: &str) -> Result<(), AgentError> {
        self.move_to(message_id, "[Gmail]/Trash").await
    }

    async fn remove_label(&self, message_id: &str, label: &str) -> Result<(), AgentError> {
        let message_id = message_id.to_string();
        let label = label.to_string();

        self.with_session(move |session| {
            session
                .select(&label)
                .map_err(|e| AgentError::not_found(format!("folder '{label}': {e}")))?;

            let uids = imap_find_message(session, &message_id)?;
            let ids = uid_str(&uids);

            session
                .store(&ids, "+FLAGS (\\Deleted)")
                .map_err(|e| AgentError::network(format!("IMAP STORE: {e}")))?;

            session
                .expunge()
                .map_err(|e| AgentError::network(format!("IMAP EXPUNGE: {e}")))?;

            info!(message_id, label, "label removed");
            Ok(())
        })
        .await
    }

    async fn get_attachment(
        &self,
        message_id: &str,
        filename: &str,
        save_path: &str,
    ) -> Result<String, AgentError> {
        let sp = Path::new(save_path);
        for component in sp.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(AgentError::invalid_input(
                    "save_path must not contain '..' components",
                ));
            }
        }

        let message_id = message_id.to_string();
        let filename = filename.to_string();
        let save_path = save_path.to_string();

        self.with_session(move |session| {
            session
                .select("INBOX")
                .map_err(|e| AgentError::network(format!("IMAP SELECT: {e}")))?;

            let uids = imap_find_message(session, &message_id)?;
            let ids = uid_str(&uids);

            let fetches = session
                .fetch(&ids, "BODY[]")
                .map_err(|e| AgentError::network(format!("IMAP FETCH: {e}")))?;

            let raw = fetches
                .iter()
                .next()
                .and_then(|f| f.body())
                .ok_or_else(|| AgentError::not_found(format!("no body for: {message_id}")))?;

            let parsed = mailparse::parse_mail(raw)
                .map_err(|e| AgentError::internal(format!("mailparse: {e}")))?;

            let body = find_attachment(&parsed, &filename).ok_or_else(|| {
                AgentError::not_found(format!(
                    "attachment '{filename}' not found in {message_id}"
                ))
            })?;

            if let Some(parent) = Path::new(&save_path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AgentError::internal(format!("mkdir: {e}")))?;
            }

            std::fs::write(&save_path, &body)
                .map_err(|e| AgentError::internal(format!("write {save_path}: {e}")))?;

            info!(
                message_id,
                filename,
                bytes = body.len(),
                save_path,
                "attachment downloaded"
            );

            Ok(save_path)
        })
        .await
    }

    async fn create_draft(
        &self,
        to: &[String],
        subject: &str,
        body: &str,
    ) -> Result<Message, AgentError> {
        if to.is_empty() {
            return Err(AgentError::invalid_input("'to' cannot be empty"));
        }

        let from_mailbox: Mailbox = self
            .config
            .email
            .parse()
            .map_err(|e| AgentError::internal(format!("invalid from: {e}")))?;

        let mut builder = lettre::Message::builder()
            .from(from_mailbox)
            .subject(subject);

        for addr in to {
            let mbox: Mailbox = addr
                .parse()
                .map_err(|e| AgentError::invalid_input(format!("invalid to '{addr}': {e}")))?;
            builder = builder.to(mbox);
        }

        let email = builder
            .singlepart(
                SinglePart::builder()
                    .content_type(ContentType::TEXT_PLAIN)
                    .body(body.to_string()),
            )
            .map_err(|e| AgentError::internal(format!("email build: {e}")))?;

        let message_id = email
            .headers()
            .get_raw("Message-ID")
            .unwrap_or_default()
            .to_string();

        let rfc_bytes = email.formatted();
        let subject = subject.to_string();
        let body_text = body.to_string();
        let to_vec = to.to_vec();
        let mid = message_id.clone();
        let sender = self.config.email.clone();

        self.with_session(move |session| {
            session
                .append("[Gmail]/Drafts", &rfc_bytes)
                .map_err(|e| AgentError::network(format!("IMAP APPEND drafts: {e}")))?;

            info!(to = ?to_vec, subject, "draft created");

            Ok(Message {
                id: mid,
                platform: Platform::Gmail,
                channel_id: "[Gmail]/Drafts".to_string(),
                sender,
                text: body_text,
                timestamp: now_ts(),
                has_attachment: false,
                reply_to: None,
                meta: MessageMeta {
                    subject: if subject.is_empty() {
                        None
                    } else {
                        Some(subject)
                    },
                    ..Default::default()
                },
            })
        })
        .await
    }
}
