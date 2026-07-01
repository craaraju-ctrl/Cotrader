//! Telegram Alert — Send alerts via Telegram.

pub struct TelegramAlert {
    bot_token: String,
    chat_id: String,
}

impl TelegramAlert {
    pub fn new(bot_token: &str, chat_id: &str) -> Self {
        Self {
            bot_token: bot_token.to_string(),
            chat_id: chat_id.to_string(),
        }
    }

    pub async fn send(&self, message: &str) -> Result<(), String> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        let client = reqwest::Client::new();
        let _ = client.post(&url)
            .json(&serde_json::json!({
                "chat_id": self.chat_id,
                "text": message,
                "parse_mode": "HTML"
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
