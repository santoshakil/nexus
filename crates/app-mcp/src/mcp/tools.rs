use serde_json::json;

use super::types::ToolDef;

pub fn available_tools(
    has_telegram: bool,
    has_gmail: bool,
    has_whatsapp: bool,
    has_slack: bool,
    has_discord: bool,
) -> Vec<ToolDef> {
    let mut tools = universal_tools();
    if has_telegram {
        tools.extend(telegram_tools());
    }
    if has_gmail {
        tools.extend(gmail_tools());
    }
    if has_whatsapp {
        tools.extend(whatsapp_tools());
    }
    if has_slack {
        tools.extend(slack_tools());
    }
    if has_discord {
        tools.extend(discord_tools());
    }
    tools
}

fn format_param() -> serde_json::Value {
    json!({
        "type": "string",
        "description": "Response format: 'compact' (default, one-liner per item, truncated to 200 chars), 'expanded' (one-liner per item, full text), or 'full' (complete JSON with all fields). Use compact for browsing, expanded for reading full messages, full when you need exact field values.",
        "enum": ["compact", "expanded", "full"]
    })
}

fn platform_param() -> serde_json::Value {
    json!({
        "type": "string",
        "description": "Platform: telegram, gmail, whatsapp, slack, or discord",
        "enum": ["telegram", "gmail", "whatsapp", "slack", "discord"]
    })
}

fn universal_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "list_platforms",
            description: "List all connected messaging platforms and their status. Call this first to discover which platforms are available before using platform-specific tools.",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "get_profile",
            description: "Get the authenticated user's profile on a platform",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "platform": platform_param(),
                    "format": format_param()
                },
                "required": ["platform"]
            }),
        },
        ToolDef {
            name: "list_channels",
            description: "List chats, inboxes, or conversations on a platform. For Telegram: returns chats sorted by last activity. For Gmail: returns folders/labels (INBOX, Sent, Drafts, etc.). For Slack: returns channels, DMs, group DMs. For Discord: returns text channels across guilds.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "platform": platform_param(),
                    "limit": {
                        "type": "integer",
                        "description": "Max channels to return (default: 20, max: 100)"
                    },
                    "format": format_param()
                },
                "required": ["platform"]
            }),
        },
        ToolDef {
            name: "read_messages",
            description: "Read messages from a specific chat, inbox thread, or conversation. Returns newest first. Supports pagination via cursor for browsing history. For Telegram: use chat name, @username, or numeric ID. For Gmail: use folder name like 'INBOX', '[Gmail]/Sent Mail', or a label name. For Slack: use channel ID. For Discord: use channel ID.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "platform": platform_param(),
                    "channel": {
                        "type": "string",
                        "description": "Chat name, @username, numeric chat ID, email folder, Slack channel ID, or Discord channel ID"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max messages to return (default: 20, max: 200)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Pagination cursor from a previous response to fetch the next page of older messages"
                    },
                    "format": format_param()
                },
                "required": ["platform", "channel"]
            }),
        },
        ToolDef {
            name: "send_message",
            description: "Send a message to a chat, email, or conversation. For Telegram: sends to a chat (name, @username, or ID). For Gmail: sends a plain email. For Slack: posts to channel (reply_to = thread_ts). For Discord: sends to channel (reply_to = message ID). For WhatsApp: sends to a phone number.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "platform": platform_param(),
                    "channel": {
                        "type": "string",
                        "description": "Chat name, @username, email address, channel ID, or phone number"
                    },
                    "text": {
                        "type": "string",
                        "description": "Message text to send"
                    },
                    "reply_to": {
                        "type": "string",
                        "description": "Message ID to reply to. Creates a threaded reply. For Telegram: numeric ID. For Slack: message timestamp (ts). For Discord: snowflake ID. For WhatsApp: wamid string."
                    },
                    "format": format_param()
                },
                "required": ["platform", "channel", "text"]
            }),
        },
        ToolDef {
            name: "search",
            description: "Search messages across a platform. For Telegram: searches all chats by text content. For Gmail: uses IMAP search syntax — simple text searches the body, or use IMAP criteria like 'FROM sender@example.com', 'SUBJECT keyword', 'SINCE 01-Jan-2025'. For Slack: searches across workspace. For Discord: searches across guild.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "platform": platform_param(),
                    "query": {
                        "type": "string",
                        "description": "Search query text, or IMAP search criteria for Gmail"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results (default: 20, max: 100)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Pagination cursor from a previous response to fetch the next page"
                    },
                    "format": format_param()
                },
                "required": ["platform", "query"]
            }),
        },
    ]
}

fn telegram_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "telegram_download_media",
            description: "Download a media file (photo, video, document, voice note, GIF, audio) from a Telegram message to a local path. The file is first downloaded by TDLib, then copied to the specified save_path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or numeric chat ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID containing the media"
                    },
                    "save_path": {
                        "type": "string",
                        "description": "Local file path to save the media to (e.g. /tmp/photo.jpg)"
                    }
                },
                "required": ["chat", "message_id", "save_path"]
            }),
        },
        ToolDef {
            name: "telegram_forward_message",
            description: "Forward a message from one Telegram chat to another. Preserves the original sender attribution.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from_chat": {
                        "type": "string",
                        "description": "Source chat name, @username, or ID"
                    },
                    "to_chat": {
                        "type": "string",
                        "description": "Destination chat name, @username, or ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID to forward"
                    },
                    "format": format_param()
                },
                "required": ["from_chat", "to_chat", "message_id"]
            }),
        },
        ToolDef {
            name: "telegram_edit_message",
            description: "Edit the text of a previously sent Telegram message. Only works on your own messages. The message must be a text message.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID to edit (must be your own message)"
                    },
                    "text": {
                        "type": "string",
                        "description": "New message text"
                    },
                    "format": format_param()
                },
                "required": ["chat", "message_id", "text"]
            }),
        },
        ToolDef {
            name: "telegram_delete_messages",
            description: "Delete one or more messages from a Telegram chat. Revokes for all users when possible (depends on chat permissions and message age).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "message_ids": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Array of message IDs to delete"
                    }
                },
                "required": ["chat", "message_ids"]
            }),
        },
        ToolDef {
            name: "telegram_pin_message",
            description: "Pin a message in a Telegram chat. Sends a notification to chat members.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID to pin"
                    }
                },
                "required": ["chat", "message_id"]
            }),
        },
        ToolDef {
            name: "telegram_unpin_message",
            description: "Unpin a previously pinned message in a Telegram chat.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID to unpin"
                    }
                },
                "required": ["chat", "message_id"]
            }),
        },
        ToolDef {
            name: "telegram_get_chat_info",
            description: "Get detailed info about a Telegram chat including member count, description, invite link, and verification/scam status. Works on groups, supergroups, channels, and private chats.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "format": format_param()
                },
                "required": ["chat"]
            }),
        },
        ToolDef {
            name: "telegram_mark_read",
            description: "Mark messages as read in a Telegram chat up to the given message ID. This clears the unread counter for the chat.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Mark all messages up to and including this ID as read"
                    }
                },
                "required": ["chat", "message_id"]
            }),
        },
        ToolDef {
            name: "telegram_get_message",
            description: "Get a single Telegram message by ID with full metadata (reactions, views, edit status, pin status, reply info). Useful for inspecting a specific message after seeing its ID in read_messages or search results.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID to retrieve"
                    },
                    "format": format_param()
                },
                "required": ["chat", "message_id"]
            }),
        },
        ToolDef {
            name: "telegram_send_media",
            description: "Send a media file (photo, video, document) to a Telegram chat. Auto-detects type from file extension, or specify explicitly. Supports optional caption text.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Local file path to send (e.g. /tmp/photo.jpg)"
                    },
                    "caption": {
                        "type": "string",
                        "description": "Optional caption text for the media"
                    },
                    "media_type": {
                        "type": "string",
                        "description": "Force media type: 'photo', 'video', or 'document'. Auto-detected from extension if not specified.",
                        "enum": ["photo", "video", "document"]
                    },
                    "format": format_param()
                },
                "required": ["chat", "file_path"]
            }),
        },
        ToolDef {
            name: "telegram_react",
            description: "Add an emoji reaction to a Telegram message. Common reactions: thumbs up, heart, fire, etc.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID to react to"
                    },
                    "emoji": {
                        "type": "string",
                        "description": "Emoji to react with (e.g. '👍', '❤️', '🔥', '😂')"
                    }
                },
                "required": ["chat", "message_id", "emoji"]
            }),
        },
        ToolDef {
            name: "telegram_search_chat",
            description: "Search messages within a specific Telegram chat. Unlike the universal search which searches all chats, this searches only within one chat.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID"
                    },
                    "query": {
                        "type": "string",
                        "description": "Text to search for within the chat"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results to return (default: 20, max: 100)"
                    },
                    "format": format_param()
                },
                "required": ["chat", "query"]
            }),
        },
        ToolDef {
            name: "telegram_get_chat_members",
            description: "Get the member list of a Telegram group or supergroup. Returns user names, usernames, roles (owner/admin/member), and IDs.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Chat name, @username, or ID (must be a group or supergroup)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max members to return (default: 50, max: 200)"
                    },
                    "format": format_param()
                },
                "required": ["chat"]
            }),
        },
    ]
}

fn gmail_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "gmail_send_email",
            description: "Send an email via Gmail with full control over recipients, subject, and attachments. Supports CC, BCC, reply threading, and file attachments from local paths.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Recipient email addresses"
                    },
                    "cc": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "CC email addresses"
                    },
                    "bcc": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "BCC email addresses"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Email subject line"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email body text (plain text)"
                    },
                    "reply_to": {
                        "type": "string",
                        "description": "Message-ID to reply to (creates a threaded reply). Get this from a message's id field."
                    },
                    "attachments": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Local file paths to attach (e.g. ['/tmp/report.pdf'])"
                    },
                    "format": format_param()
                },
                "required": ["to", "subject", "body"]
            }),
        },
        ToolDef {
            name: "gmail_archive",
            description: "Archive a Gmail thread by removing it from INBOX. The thread remains accessible in All Mail and via search.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Gmail Message-ID (from the message's id field)"
                    }
                },
                "required": ["thread_id"]
            }),
        },
        ToolDef {
            name: "gmail_list_labels",
            description: "List all Gmail labels/folders including system labels ([Gmail]/Sent Mail, etc.) and custom labels. Use these names as channel IDs for read_messages.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "format": format_param()
                }
            }),
        },
        ToolDef {
            name: "gmail_add_label",
            description: "Add a label to a Gmail thread by copying it to the label folder. The label must already exist in Gmail.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    },
                    "label": {
                        "type": "string",
                        "description": "Label/folder name (e.g. 'Work', '[Gmail]/Important')"
                    }
                },
                "required": ["thread_id", "label"]
            }),
        },
        ToolDef {
            name: "gmail_mark_read",
            description: "Mark a Gmail message as read (adds \\Seen flag)",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    }
                },
                "required": ["message_id"]
            }),
        },
        ToolDef {
            name: "gmail_mark_unread",
            description: "Mark a Gmail message as unread (removes \\Seen flag)",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    }
                },
                "required": ["message_id"]
            }),
        },
        ToolDef {
            name: "gmail_star",
            description: "Star a Gmail message (adds \\Flagged flag). Starred messages appear in [Gmail]/Starred.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    }
                },
                "required": ["message_id"]
            }),
        },
        ToolDef {
            name: "gmail_unstar",
            description: "Remove star from a Gmail message (removes \\Flagged flag)",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    }
                },
                "required": ["message_id"]
            }),
        },
        ToolDef {
            name: "gmail_move_to",
            description: "Move a Gmail message to a different folder. Common targets: '[Gmail]/Trash', '[Gmail]/Spam', 'INBOX', or any label name. Moves by copying to destination then deleting from source.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    },
                    "folder": {
                        "type": "string",
                        "description": "Destination folder (e.g. '[Gmail]/Trash', 'INBOX', 'Work')"
                    }
                },
                "required": ["message_id", "folder"]
            }),
        },
        ToolDef {
            name: "gmail_trash",
            description: "Move a Gmail message to trash. Shortcut for gmail_move_to with '[Gmail]/Trash'. Messages in trash are permanently deleted after 30 days.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    }
                },
                "required": ["message_id"]
            }),
        },
        ToolDef {
            name: "gmail_remove_label",
            description: "Remove a label from a Gmail message. Removes the message from the label's folder.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    },
                    "label": {
                        "type": "string",
                        "description": "Label name to remove"
                    }
                },
                "required": ["message_id", "label"]
            }),
        },
        ToolDef {
            name: "gmail_get_attachment",
            description: "Download an attachment from a Gmail message to a local file. Specify the message ID and the filename of the attachment you want.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail Message-ID"
                    },
                    "filename": {
                        "type": "string",
                        "description": "Name of the attachment file to download"
                    },
                    "save_path": {
                        "type": "string",
                        "description": "Local file path to save the attachment (e.g. /tmp/report.pdf)"
                    }
                },
                "required": ["message_id", "filename", "save_path"]
            }),
        },
        ToolDef {
            name: "gmail_create_draft",
            description: "Create an email draft in Gmail without sending it. The draft appears in [Gmail]/Drafts and can be sent later from Gmail UI.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Recipient email addresses"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Email subject line"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email body text (plain text)"
                    },
                    "format": format_param()
                },
                "required": ["to", "subject", "body"]
            }),
        },
    ]
}

fn whatsapp_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "whatsapp_send_media",
            description: "Send a media file (image, video, document, audio) via WhatsApp to a phone number. The file is uploaded to WhatsApp's servers first, then sent.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat": {
                        "type": "string",
                        "description": "Recipient phone number in international format (e.g. '1234567890')"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Local file path to send"
                    },
                    "caption": {
                        "type": "string",
                        "description": "Caption text for the media"
                    }
                },
                "required": ["chat", "file_path", "caption"]
            }),
        },
    ]
}

fn slack_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "slack_set_status",
            description: "Set your Slack status message and emoji. Use an empty string to clear.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Status text (e.g. 'In a meeting')"
                    },
                    "emoji": {
                        "type": "string",
                        "description": "Status emoji (e.g. ':calendar:' or ':coffee:')"
                    }
                },
                "required": ["text", "emoji"]
            }),
        },
        ToolDef {
            name: "slack_create_channel",
            description: "Create a new Slack channel. Names must be lowercase, no spaces (use hyphens).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Channel name (lowercase, hyphens, no spaces)"
                    },
                    "is_private": {
                        "type": "boolean",
                        "description": "Create as private channel (default: false)"
                    },
                    "format": format_param()
                },
                "required": ["name"]
            }),
        },
        ToolDef {
            name: "slack_invite_to_channel",
            description: "Invite a user to a Slack channel by their user ID.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID"
                    },
                    "user_id": {
                        "type": "string",
                        "description": "User ID to invite"
                    }
                },
                "required": ["channel", "user_id"]
            }),
        },
        ToolDef {
            name: "slack_set_topic",
            description: "Set the topic of a Slack channel.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID"
                    },
                    "topic": {
                        "type": "string",
                        "description": "New channel topic"
                    }
                },
                "required": ["channel", "topic"]
            }),
        },
        ToolDef {
            name: "slack_add_reaction",
            description: "Add an emoji reaction to a Slack message. Use emoji name without colons (e.g. 'thumbsup' not ':thumbsup:').",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID"
                    },
                    "message_ts": {
                        "type": "string",
                        "description": "Message timestamp (ts field from messages)"
                    },
                    "emoji": {
                        "type": "string",
                        "description": "Emoji name without colons (e.g. 'thumbsup', 'heart', 'fire')"
                    }
                },
                "required": ["channel", "message_ts", "emoji"]
            }),
        },
        ToolDef {
            name: "slack_remove_reaction",
            description: "Remove an emoji reaction from a Slack message.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID"
                    },
                    "message_ts": {
                        "type": "string",
                        "description": "Message timestamp"
                    },
                    "emoji": {
                        "type": "string",
                        "description": "Emoji name without colons"
                    }
                },
                "required": ["channel", "message_ts", "emoji"]
            }),
        },
        ToolDef {
            name: "slack_upload_file",
            description: "Upload a file to one or more Slack channels.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channels": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Channel IDs to share the file in"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Local file path to upload"
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional title for the file"
                    }
                },
                "required": ["channels", "file_path"]
            }),
        },
        ToolDef {
            name: "slack_list_users",
            description: "List users in the Slack workspace. Returns names, usernames, roles, and IDs.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Max users to return (default: 50)"
                    },
                    "format": format_param()
                }
            }),
        },
        ToolDef {
            name: "slack_get_user_info",
            description: "Get detailed profile info for a Slack user by their user ID.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user": {
                        "type": "string",
                        "description": "Slack user ID"
                    },
                    "format": format_param()
                },
                "required": ["user"]
            }),
        },
    ]
}

fn discord_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "discord_list_guilds",
            description: "List all Discord servers (guilds) the bot is a member of.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "format": format_param()
                }
            }),
        },
        ToolDef {
            name: "discord_list_guild_channels",
            description: "List all channels in a Discord server (guild). Returns text, voice, and category channels.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "guild_id": {
                        "type": "string",
                        "description": "Discord guild/server ID"
                    },
                    "format": format_param()
                },
                "required": ["guild_id"]
            }),
        },
        ToolDef {
            name: "discord_create_thread",
            description: "Create a new thread in a Discord channel. Can be attached to a specific message or created as a standalone thread.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID to create thread in"
                    },
                    "name": {
                        "type": "string",
                        "description": "Thread name"
                    },
                    "message_id": {
                        "type": "string",
                        "description": "Optional message ID to attach the thread to"
                    },
                    "format": format_param()
                },
                "required": ["channel", "name"]
            }),
        },
        ToolDef {
            name: "discord_add_reaction",
            description: "Add an emoji reaction to a Discord message. Use Unicode emoji or custom emoji in name:id format.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID"
                    },
                    "message_id": {
                        "type": "string",
                        "description": "Message ID"
                    },
                    "emoji": {
                        "type": "string",
                        "description": "Emoji (Unicode like '👍' or custom like 'name:id')"
                    }
                },
                "required": ["channel", "message_id", "emoji"]
            }),
        },
        ToolDef {
            name: "discord_remove_reaction",
            description: "Remove your emoji reaction from a Discord message.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID"
                    },
                    "message_id": {
                        "type": "string",
                        "description": "Message ID"
                    },
                    "emoji": {
                        "type": "string",
                        "description": "Emoji to remove"
                    }
                },
                "required": ["channel", "message_id", "emoji"]
            }),
        },
        ToolDef {
            name: "discord_pin_message",
            description: "Pin a message in a Discord channel. Pinned messages appear in the channel's pin list.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID"
                    },
                    "message_id": {
                        "type": "string",
                        "description": "Message ID to pin"
                    }
                },
                "required": ["channel", "message_id"]
            }),
        },
    ]
}
