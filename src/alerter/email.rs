use anyhow::Result;
use reqwest::Client;
use serde::Serialize;
use tracing::{error, info};
use uuid::Uuid;

/// SendGrid email sender
pub struct EmailSender {
    client: Client,
    api_key: String,
    from_email: String,
}

#[derive(Serialize)]
struct SendGridMessage {
    personalizations: Vec<Personalization>,
    from: EmailAddress,
    subject: String,
    content: Vec<Content>,
}

#[derive(Serialize)]
struct Personalization {
    to: Vec<EmailAddress>,
}

#[derive(Serialize)]
struct EmailAddress {
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize)]
struct Content {
    #[serde(rename = "type")]
    content_type: String,
    value: String,
}

impl EmailSender {
    pub fn new(api_key: String, from_email: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            from_email,
        }
    }

    /// Send email to user
    pub async fn send(
        &self,
        user_id: Uuid,
        to_email: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<()> {
        info!("Email to {} ({}): {}", user_id, to_email, subject);

        let message = SendGridMessage {
            personalizations: vec![Personalization {
                to: vec![EmailAddress {
                    email: to_email.to_string(),
                    name: None,
                }],
            }],
            from: EmailAddress {
                email: self.from_email.clone(),
                name: Some("Safety Net".to_string()),
            },
            subject: subject.to_string(),
            content: vec![Content {
                content_type: "text/html".to_string(),
                value: body_html.to_string(),
            }],
        };

        let response = self.client
            .post("https://api.sendgrid.com/v3/mail/send")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&message)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                info!("Email sent successfully");
            }
            Ok(resp) => {
                error!("SendGrid error: {}", resp.status());
            }
            Err(e) => {
                error!("SendGrid request failed: {}", e);
            }
        }

        Ok(())
    }

    /// Send health factor alert email
    pub async fn send_health_factor_alert(
        &self,
        user_id: Uuid,
        to_email: &str,
        current_hf: f64,
        suggested_repay: Option<f64>,
        alert_id: Uuid,
    ) -> Result<()> {
        let subject = format!("⚠️ Health Factor Alert: {:.2}", current_hf);

        let repay_section = if let Some(amount) = suggested_repay {
            format!(
                r#"<p>Suggested action: Repay <strong>${:.0}</strong> to restore healthy position.</p>"#,
                amount
            )
        } else {
            String::new()
        };

        let body = format!(
            r#"
            <html>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
                <h2>🚨 Health Factor Alert</h2>
                <p>Your DeFi position health factor has dropped to <strong>{:.2}</strong>.</p>
                {}
                <p>
                    <a href="https://safetynet.app/alert/{}"
                       style="background: #ef4444; color: white; padding: 12px 24px;
                              text-decoration: none; border-radius: 6px; display: inline-block;">
                        View Position
                    </a>
                </p>
                <p style="color: #666; font-size: 12px;">
                    You received this email because you have Safety Net monitoring enabled.
                </p>
            </body>
            </html>
            "#,
            current_hf, repay_section, alert_id
        );

        self.send(user_id, to_email, &subject, &body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sendgrid_message_serialization() {
        let message = SendGridMessage {
            personalizations: vec![Personalization {
                to: vec![EmailAddress {
                    email: "test@example.com".to_string(),
                    name: None,
                }],
            }],
            from: EmailAddress {
                email: "noreply@safetynet.app".to_string(),
                name: Some("Safety Net".to_string()),
            },
            subject: "Test".to_string(),
            content: vec![Content {
                content_type: "text/html".to_string(),
                value: "<p>Test</p>".to_string(),
            }],
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("test@example.com"));
        assert!(json.contains("Safety Net"));
    }
}
