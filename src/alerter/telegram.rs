use anyhow::Result;
use reqwest::Client;
use serde::Serialize;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Telegram bot for alerts
pub struct TelegramBot {
    client: Client,
    bot_token: String,
}

#[derive(Serialize)]
struct TelegramMessage {
    chat_id: String,
    text: String,
    parse_mode: String,
}

#[derive(Serialize)]
struct TelegramMessageWithKeyboard {
    chat_id: String,
    text: String,
    parse_mode: String,
    reply_markup: InlineKeyboardMarkup,
}

#[derive(Serialize)]
struct InlineKeyboardMarkup {
    inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

#[derive(Serialize)]
struct InlineKeyboardButton {
    text: String,
    callback_data: Option<String>,
    url: Option<String>,
}

impl TelegramBot {
    pub fn new(bot_token: String) -> Self {
        Self {
            client: Client::new(),
            bot_token,
        }
    }

    /// Send message to a specific Telegram chat ID
    pub async fn send_to_chat(&self, chat_id: &str, message: &str) -> Result<()> {
        info!("Sending Telegram message to chat {}", chat_id);

        let msg = TelegramMessage {
            chat_id: chat_id.to_string(),
            text: message.to_string(),
            parse_mode: "Markdown".to_string(),
        };

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let response = self.client
            .post(&url)
            .json(&msg)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                info!("Telegram message sent successfully");
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                error!("Telegram error {}: {}", status, body);
            }
            Err(e) => {
                error!("Telegram request failed: {}", e);
            }
        }

        Ok(())
    }

    /// Legacy method that logs when no chat ID is available
    pub async fn send(&self, user_id: Uuid, message: &str) -> Result<()> {
        warn!("Telegram message to {} without chat ID: {}", user_id, message);
        // Would need to look up chat ID from database
        Ok(())
    }

    /// Send alert with action buttons (inline keyboard)
    pub async fn send_alert_with_actions(
        &self,
        chat_id: &str,
        message: &str,
        alert_id: Uuid,
    ) -> Result<()> {
        let msg = TelegramMessageWithKeyboard {
            chat_id: chat_id.to_string(),
            text: message.to_string(),
            parse_mode: "Markdown".to_string(),
            reply_markup: InlineKeyboardMarkup {
                inline_keyboard: vec![
                    vec![
                        InlineKeyboardButton {
                            text: "✅ Approve".to_string(),
                            callback_data: Some(format!("approve_{}", alert_id)),
                            url: None,
                        },
                        InlineKeyboardButton {
                            text: "⏰ Snooze".to_string(),
                            callback_data: Some(format!("snooze_{}", alert_id)),
                            url: None,
                        },
                    ],
                    vec![
                        InlineKeyboardButton {
                            text: "🔗 View in App".to_string(),
                            callback_data: None,
                            url: Some(format!("https://safetynet.app/alert/{}", alert_id)),
                        },
                    ],
                ],
            },
        };

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let response = self.client
            .post(&url)
            .json(&msg)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                info!("Telegram alert sent successfully");
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                error!("Telegram error {}: {}", status, body);
            }
            Err(e) => {
                error!("Telegram request failed: {}", e);
            }
        }

        Ok(())
    }

    /// Send alert with action buttons (legacy method)
    pub async fn send_with_actions(
        &self,
        user_id: Uuid,
        message: &str,
        alert_id: Uuid,
    ) -> Result<()> {
        warn!("Telegram alert to {} without chat ID", user_id);
        let formatted_message = format!(
            "{}\n\n[View Alert](https://safetynet.app/alert/{})",
            message, alert_id
        );
        self.send(user_id, &formatted_message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_message_serialization() {
        let msg = TelegramMessage {
            chat_id: "12345".to_string(),
            text: "Test message".to_string(),
            parse_mode: "Markdown".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("12345"));
        assert!(json.contains("Test message"));
    }
}
