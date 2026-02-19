# Nexus Usage Guide

Complete reference for using Nexus with AI agents.

## Table of Contents

- [How It Works](#how-it-works)
- [Universal Tools](#universal-tools)
- [Telegram](#telegram)
- [Gmail](#gmail)
- [WhatsApp](#whatsapp)
- [Slack](#slack)
- [Discord](#discord)
- [Format System](#format-system)
- [Pagination](#pagination)
- [Error Handling](#error-handling)
- [Troubleshooting](#troubleshooting)

## How It Works

Nexus is an MCP (Model Context Protocol) server. It communicates over stdio using newline-delimited JSON-RPC 2.0:

```
AI Agent <──stdin/stdout──> Nexus <──API──> Telegram, Gmail, Slack, ...
```

1. The AI agent (Claude Code, Claude Desktop, etc.) starts Nexus as a subprocess
2. Agent sends JSON-RPC requests on stdin
3. Nexus calls the appropriate platform API
4. Nexus returns JSON-RPC responses on stdout
5. All logs go to stderr (never pollutes the JSON-RPC stream)

### Tool Discovery

When the agent calls `tools/list`, Nexus returns only the tools for platforms that are configured. If you only set `TELEGRAM_API_ID` and `TELEGRAM_API_HASH`, the agent sees universal tools + Telegram tools. Gmail, Slack, Discord, and WhatsApp tools are hidden.

This means the agent never tries to call a tool for a platform that isn't connected.

## Universal Tools

These work on every connected platform.

### list_platforms

Lists all connected platforms. Call this first to discover what's available.

```json
{"platform": "telegram", "status": "connected"}
{"platform": "gmail", "status": "connected"}
```

### get_profile

Returns the authenticated user's profile on a platform.

**Parameters:**
- `platform` (required): telegram, gmail, whatsapp, slack, discord
- `format`: compact, expanded, full

**Example output (compact):**
```
Alice Johnson (@alice) | id:123456789 | phone:+1... | platform:telegram
```

### list_channels

Lists chats, inboxes, or conversations.

**Parameters:**
- `platform` (required)
- `limit`: 1-100 (default: 20)
- `format`

**What "channels" means per platform:**
- **Telegram**: Chats sorted by last activity (private chats, groups, supergroups, channels)
- **Gmail**: Folders/labels (INBOX, [Gmail]/Sent Mail, [Gmail]/Drafts, custom labels)
- **Slack**: Channels, DMs, group DMs
- **Discord**: Text channels across all guilds the bot is in

### read_messages

Reads messages from a specific chat or folder. Returns newest first.

**Parameters:**
- `platform` (required)
- `channel` (required): Chat name, @username, numeric ID, folder name, or channel ID
- `limit`: 1-200 (default: 20)
- `cursor`: Pagination cursor from a previous response
- `format`

**Channel resolution (Telegram):**
Telegram is flexible about how you identify chats:
- By name: `"Dev Team"` — matches chat title
- By username: `"@alice"` — matches public username
- By ID: `"-1001234567890"` — direct numeric ID

Names are cached for 5 minutes after first resolution.

**Example output (compact):**
```
20 messages (has more → cursor:12345):
  [Feb 19 05:10] Alice Johnson (@alice): Hey, are we still meeting at 3pm? (id:17459838976)
  [Feb 18 15:23] Bob Smith (@bob): Hi, when you get a chance please review the PR (id:17440964608)
```

### send_message

Sends a message. Supports threading via `reply_to`.

**Parameters:**
- `platform` (required)
- `channel` (required): Chat name, email address, phone number, or channel ID
- `text` (required): Message content
- `reply_to`: Message ID to reply to (creates threaded reply)
- `format`

**reply_to formats per platform:**
- **Telegram**: Numeric message ID (e.g., `"17459838976"`)
- **Gmail**: Message-ID header for email threading
- **Slack**: Message timestamp (`ts` field, e.g., `"1705312200.000100"`)
- **Discord**: Snowflake message ID
- **WhatsApp**: WAMID string

### search

Searches messages across a platform.

**Parameters:**
- `platform` (required)
- `query` (required): Search text
- `limit`: 1-100 (default: 20)
- `cursor`: Pagination cursor
- `format`

**Search syntax per platform:**
- **Telegram**: Plain text search across all chats
- **Gmail**: IMAP search criteria — `"meeting"` searches body, `FROM sender@example.com`, `SUBJECT keyword`, `SINCE 01-Jan-2025`, `UNSEEN`, combinable
- **Slack**: Slack search syntax
- **Discord**: Not available via Bot API

## Telegram

Full-featured Telegram integration via TDLib.

### telegram_search_chat

Search within a single chat (unlike `search` which searches all chats).

**Parameters:**
- `chat` (required): Chat name, @username, or ID
- `query` (required): Text to search for
- `limit`: 1-100 (default: 20)
- `format`

### telegram_get_message

Get a single message with full metadata including reactions, views, edit history, pin status.

**Parameters:**
- `chat` (required)
- `message_id` (required): Integer message ID
- `format`

### telegram_get_chat_info

Get detailed chat information.

**Parameters:**
- `chat` (required)
- `format`

**Output includes:** name, type, member count, description, invite link, verified/scam flags.

### telegram_get_chat_members

List members of a group or supergroup with roles.

**Parameters:**
- `chat` (required): Must be a group or supergroup
- `limit`: 1-200 (default: 50)
- `format`

**Output includes:** name, @username, role (owner/admin/member), user ID.

### telegram_send_media

Send a photo, video, or document.

**Parameters:**
- `chat` (required)
- `file_path` (required): Local file path
- `caption`: Optional caption text
- `media_type`: Force type — `photo`, `video`, or `document`. Auto-detected from extension if omitted.
- `format`

### telegram_download_media

Download media from a message to a local file.

**Parameters:**
- `chat` (required)
- `message_id` (required)
- `save_path` (required): Local path to save to (e.g., `/tmp/photo.jpg`)

### telegram_forward_message

Forward a message between chats. Preserves original sender attribution.

**Parameters:**
- `from_chat` (required): Source chat
- `to_chat` (required): Destination chat
- `message_id` (required)
- `format`

### telegram_edit_message

Edit a previously sent text message. Only works on your own messages.

**Parameters:**
- `chat` (required)
- `message_id` (required)
- `text` (required): New message text
- `format`

### telegram_delete_messages

Delete one or more messages. Revokes for all users when possible.

**Parameters:**
- `chat` (required)
- `message_ids` (required): Array of integer message IDs

### telegram_pin_message / telegram_unpin_message

Pin or unpin a message in a chat.

**Parameters:**
- `chat` (required)
- `message_id` (required)

### telegram_mark_read

Mark messages as read up to a given message ID.

**Parameters:**
- `chat` (required)
- `message_id` (required): Mark all up to and including this ID

### telegram_react

Add an emoji reaction to a message.

**Parameters:**
- `chat` (required)
- `message_id` (required)
- `emoji` (required): Unicode emoji (e.g., `👍`, `❤️`, `🔥`)

## Gmail

Full email management via IMAP and SMTP.

### gmail_send_email

Send an email with full control over recipients and attachments.

**Parameters:**
- `to` (required): Array of recipient emails
- `subject` (required)
- `body` (required): Plain text body
- `cc`: Array of CC addresses
- `bcc`: Array of BCC addresses
- `reply_to`: Message-ID for threading
- `attachments`: Array of local file paths
- `format`

### gmail_create_draft

Create a draft without sending.

**Parameters:**
- `to` (required): Array of recipients
- `subject` (required)
- `body` (required)
- `format`

### gmail_archive

Remove an email from INBOX. It remains in All Mail.

**Parameters:**
- `thread_id` (required): Gmail Message-ID

### gmail_trash

Move to trash (auto-deleted after 30 days).

**Parameters:**
- `message_id` (required)

### gmail_move_to

Move to any folder.

**Parameters:**
- `message_id` (required)
- `folder` (required): e.g., `[Gmail]/Trash`, `INBOX`, `Work`

### gmail_list_labels

List all labels/folders.

### gmail_add_label / gmail_remove_label

Add or remove a label.

**Parameters:**
- `thread_id` or `message_id` (required)
- `label` (required): Label name

### gmail_mark_read / gmail_mark_unread

Toggle read status.

**Parameters:**
- `message_id` (required)

### gmail_star / gmail_unstar

Toggle star.

**Parameters:**
- `message_id` (required)

### gmail_get_attachment

Download an attachment from an email.

**Parameters:**
- `message_id` (required)
- `filename` (required): Attachment filename
- `save_path` (required): Local path to save to

## WhatsApp

Limited to sending via WhatsApp Business Cloud API.

### send_message (universal)

Send a text message to a phone number.

- `channel`: Phone number in international format (e.g., `"1234567890"`)

### whatsapp_send_media

Send a media file.

**Parameters:**
- `chat` (required): Phone number
- `file_path` (required): Local file path
- `caption` (required)

**Note:** WhatsApp Business Cloud API does not support reading messages, searching, or listing conversations. These operations will return "not implemented" errors.

## Slack

Full workspace integration via Slack Web API.

### slack_set_status

Set your status message and emoji.

**Parameters:**
- `text` (required): Status text
- `emoji` (required): Emoji code (e.g., `:coffee:`)

### slack_create_channel

Create a new channel.

**Parameters:**
- `name` (required): Lowercase, hyphens, no spaces
- `is_private`: Boolean (default: false)
- `format`

### slack_invite_to_channel

Invite a user to a channel.

**Parameters:**
- `channel` (required): Channel ID
- `user_id` (required)

### slack_set_topic

Set a channel's topic.

**Parameters:**
- `channel` (required): Channel ID
- `topic` (required)

### slack_add_reaction / slack_remove_reaction

Add or remove an emoji reaction.

**Parameters:**
- `channel` (required): Channel ID
- `message_ts` (required): Message timestamp
- `emoji` (required): Emoji name without colons (e.g., `thumbsup`)

### slack_upload_file

Upload a file to channels.

**Parameters:**
- `channels` (required): Array of channel IDs
- `file_path` (required): Local file path
- `title`: Optional file title

### slack_list_users

List workspace users.

**Parameters:**
- `limit`: Max users (default: 50)
- `format`

### slack_get_user_info

Get detailed user profile.

**Parameters:**
- `user` (required): User ID
- `format`

## Discord

Bot API integration for Discord servers.

### discord_list_guilds

List all servers the bot is a member of.

### discord_list_guild_channels

List channels in a server.

**Parameters:**
- `guild_id` (required)
- `format`

### discord_create_thread

Create a thread in a channel.

**Parameters:**
- `channel` (required): Channel ID
- `name` (required): Thread name
- `message_id`: Optional message to attach thread to
- `format`

### discord_add_reaction / discord_remove_reaction

Add or remove emoji reactions.

**Parameters:**
- `channel` (required)
- `message_id` (required)
- `emoji` (required): Unicode emoji or `name:id` for custom

### discord_pin_message

Pin a message.

**Parameters:**
- `channel` (required)
- `message_id` (required)

## Format System

All data-returning tools accept `format`:

### compact (default)

One-liner per item. Text truncated to 200 characters. Optimized for token efficiency when browsing.

```
[Jan 15 10:30] Alice (@alice): Hey, I was thinking about the architecture for the new messaging service. We should probably use a hexagonal pattern wi...  (id:3707764736)
```

### expanded

One-liner per item. Full untruncated text. Good for reading full messages.

```
[Jan 15 10:30] Alice (@alice): Hey, I was thinking about the architecture for the new messaging service. We should probably use a hexagonal pattern with ports and adapters. What do you think about that approach? (id:3707764736)
```

### full

Complete JSON serialization with all fields. Use when you need exact field values, metadata, or structured data.

```json
{"id":"3707764736","platform":"telegram","channel_id":"-1001234567890","sender":"Alice Johnson","text":"Hey, I was thinking about the architecture...","timestamp":1705312200,"has_attachment":false,...}
```

## Pagination

Tools that return lists support cursor-based pagination:

1. First call returns items and optionally `has_more` + `next_cursor`
2. Pass `cursor` from the response to get the next page
3. Repeat until `has_more` is false or no cursor is returned

**Example flow:**
```
→ read_messages(platform: "telegram", channel: "Dev Team", limit: 20)
← 20 messages (has more → cursor:17459838976)

→ read_messages(platform: "telegram", channel: "Dev Team", limit: 20, cursor: "17459838976")
← 20 messages (has more → cursor:17440964608)
```

## Error Handling

Nexus returns structured errors with suggestions:

```
[auth] Telegram session expired | try: re-run `nexus auth telegram` | retryable: false
```

**Error categories:**
- `auth` — Authentication failed or session expired
- `api` — Platform API returned an error
- `network` — Connection or timeout issue (usually retryable)
- `session` — Session state issue
- `not_found` — Chat, message, or resource not found
- `invalid_input` — Bad parameters
- `platform_not_available` — Platform not configured
- `not_implemented` — Feature not available for this platform
- `internal` — Unexpected error

## Troubleshooting

### Nexus starts but no platforms connect

Check that environment variables are set correctly. Nexus logs platform connection status to stderr:

```bash
RUST_LOG=nexus=debug nexus mcp 2>nexus.log
```

Look for lines like:
```
INFO nexus: telegram connected
INFO nexus: gmail configured
INFO nexus: slack not configured: SLACK_BOT_TOKEN env var not set
```

### Telegram auth fails

- Ensure TDLib (`libtdjson.so`) is installed at `/usr/local/lib` or set `TDLIB_DIR`
- Run `nexus auth telegram` interactively first
- Check that `~/.nexus/tdlib/` directory exists and has session files
- If session is corrupted, delete `~/.nexus/tdlib/` and re-authenticate

### Gmail connection issues

- Verify you're using an App Password, not your regular Google password
- Ensure 2-Step Verification is enabled on your Google account
- Check that "Less secure app access" is not blocking you (App Passwords bypass this)
- IMAP must be enabled in Gmail settings

### Large message IDs (Telegram)

Telegram supergroup message IDs can exceed 32-bit integer range. Nexus uses 64-bit integers internally. If you're building tools that consume Nexus output, ensure your JSON parser handles large integers.

### Tool not showing up

Tools are filtered by connected platforms. If `telegram_search_chat` doesn't appear in `tools/list`, Telegram is not connected. Check your environment variables and re-run.

### MCP client can't find Nexus binary

Ensure the binary is in your PATH or use an absolute path in your MCP config:

```json
{
  "command": "/home/user/.local/bin/nexus",
  "args": ["mcp"]
}
```

### Logging

Control log verbosity:

```bash
# Minimal (default)
RUST_LOG=nexus=info

# Debug (shows API calls)
RUST_LOG=nexus=debug

# Trace (very verbose)
RUST_LOG=nexus=trace

# Silence all logs
RUST_LOG=off
```
