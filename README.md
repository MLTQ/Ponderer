# Ponderer

## Telegram Bot Setup

Ponderer has a built-in Telegram bot that lets you message the agent from your phone. It uses a dedicated conversation separate from the desktop UI.

### 1. Create a bot with BotFather

1. Open Telegram and search for **@BotFather**
2. Send `/newbot` and follow the prompts (pick a name and username)
3. BotFather will give you a **bot token** — save it

### 2. Find your chat ID

1. Start a conversation with your new bot (send any message)
2. Open this URL in a browser, replacing `<TOKEN>` with your token:
   ```
   https://api.telegram.org/bot<TOKEN>/getUpdates
   ```
3. Look for `"chat":{"id":...}` in the response — that number is your **chat ID**

### 3. Configure environment variables

Set these before starting Ponderer:

```bash
export TELEGRAM_BOT_TOKEN="<your-bot-token>"
export TELEGRAM_CHAT_ID="<your-chat-id>"   # optional but recommended
```

`TELEGRAM_CHAT_ID` restricts the bot to your account only. If omitted, anyone who messages the bot can talk to the agent.

### 4. Start Ponderer

No extra steps — the bot starts automatically when `TELEGRAM_BOT_TOKEN` is set. You should see a log line:

```
Telegram bot active (allowed_chat_id: Some(<id>))
```

Messages you send to the bot are routed into a conversation named `"telegram"` and replies come back as Telegram messages.

---

