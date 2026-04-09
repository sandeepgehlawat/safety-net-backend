use anyhow::Result;
use reqwest::Client;
use serde::Serialize;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Firebase Cloud Messaging push notifier
pub struct PushNotifier {
    client: Client,
    api_key: String,
}

#[derive(Serialize)]
struct FcmMessage {
    to: String,
    notification: FcmNotification,
    data: FcmData,
}

#[derive(Serialize)]
struct FcmNotification {
    title: String,
    body: String,
}

#[derive(Serialize)]
struct FcmData {
    deep_link: String,
    alert_id: String,
}

impl PushNotifier {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    /// Send push notification to user using their FCM token
    pub async fn send_to_token(
        &self,
        fcm_token: &str,
        title: &str,
        body: &str,
        deep_link: &str,
    ) -> Result<()> {
        info!("Sending push notification: {}", title);

        let message = FcmMessage {
            to: fcm_token.to_string(),
            notification: FcmNotification {
                title: title.to_string(),
                body: body.to_string(),
            },
            data: FcmData {
                deep_link: deep_link.to_string(),
                alert_id: Uuid::new_v4().to_string(),
            },
        };

        // Send to FCM
        let response = self.client
            .post("https://fcm.googleapis.com/fcm/send")
            .header("Authorization", format!("key={}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&message)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                info!("Push notification sent successfully");
            }
            Ok(resp) => {
                error!("FCM error: {}", resp.status());
            }
            Err(e) => {
                error!("FCM request failed: {}", e);
            }
        }

        Ok(())
    }

    /// Legacy method that logs when no token is available
    pub async fn send(
        &self,
        user_id: Uuid,
        title: &str,
        body: &str,
        _deep_link: &str,
    ) -> Result<()> {
        warn!(
            "Push notification to {} without FCM token: {} - {}",
            user_id, title, body
        );
        // Would need to look up FCM token from database
        // This method kept for backward compatibility
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fcm_message_serialization() {
        let message = FcmMessage {
            to: "token123".to_string(),
            notification: FcmNotification {
                title: "Test".to_string(),
                body: "Test body".to_string(),
            },
            data: FcmData {
                deep_link: "/test".to_string(),
                alert_id: "abc".to_string(),
            },
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("token123"));
        assert!(json.contains("Test"));
    }
}
