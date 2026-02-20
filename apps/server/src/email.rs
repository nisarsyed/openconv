use openconv_shared::error::OpenConvError;

use crate::config::EmailConfig;

#[async_trait::async_trait]
pub trait EmailService: Send + Sync {
    async fn send_verification_code(&self, to: &str, code: &str) -> Result<(), OpenConvError>;
    async fn send_recovery_code(&self, to: &str, code: &str) -> Result<(), OpenConvError>;
}

/// Mock email service that logs codes via tracing. Used for development and testing.
#[derive(Default)]
pub struct MockEmailService;

impl MockEmailService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl EmailService for MockEmailService {
    async fn send_verification_code(&self, to: &str, code: &str) -> Result<(), OpenConvError> {
        tracing::info!(to = to, code = code, "mock: verification code");
        Ok(())
    }

    async fn send_recovery_code(&self, to: &str, code: &str) -> Result<(), OpenConvError> {
        tracing::info!(to = to, code = code, "mock: recovery code");
        Ok(())
    }
}

/// SMTP email service using lettre.
pub struct SmtpEmailService {
    transport: lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    from: lettre::message::Mailbox,
}

impl SmtpEmailService {
    pub fn new(config: &EmailConfig) -> Result<Self, OpenConvError> {
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::AsyncSmtpTransport;

        let creds = Credentials::new(
            config.smtp_username.clone(),
            config.smtp_password.clone(),
        );

        let transport = AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| OpenConvError::Internal(format!("SMTP relay error: {e}")))?
            .port(config.smtp_port)
            .credentials(creds)
            .build();

        let from = format!("{} <{}>", config.from_name, config.from_address)
            .parse()
            .map_err(|e| {
                OpenConvError::Internal(format!("invalid from address: {e}"))
            })?;

        Ok(Self { transport, from })
    }

    async fn send_email(&self, to: &str, subject: &str, body: String) -> Result<(), OpenConvError> {
        use lettre::{AsyncTransport, Message};

        let to_mailbox: lettre::message::Mailbox = to
            .parse()
            .map_err(|e| OpenConvError::Validation(format!("invalid email address: {e}")))?;

        let message = Message::builder()
            .from(self.from.clone())
            .to(to_mailbox)
            .subject(subject)
            .body(body)
            .map_err(|e| OpenConvError::Internal(format!("email build error: {e}")))?;

        self.transport
            .send(message)
            .await
            .map_err(|e| {
                OpenConvError::ServiceUnavailable(format!("email send failed: {e}"))
            })?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl EmailService for SmtpEmailService {
    async fn send_verification_code(&self, to: &str, code: &str) -> Result<(), OpenConvError> {
        self.send_email(
            to,
            "OpenConv - Verification Code",
            format!("Your OpenConv verification code is: {code}\n\nThis code expires in 10 minutes."),
        )
        .await
    }

    async fn send_recovery_code(&self, to: &str, code: &str) -> Result<(), OpenConvError> {
        self.send_email(
            to,
            "OpenConv - Account Recovery Code",
            format!("Your OpenConv recovery code is: {code}\n\nThis code expires in 10 minutes."),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_email_service_implements_trait() {
        let svc = MockEmailService::new();
        assert!(svc.send_verification_code("a@b.com", "123456").await.is_ok());
        assert!(svc.send_recovery_code("a@b.com", "654321").await.is_ok());
    }

    #[tokio::test]
    async fn mock_email_send_verification_code_succeeds() {
        let svc = MockEmailService::new();
        let result = svc.send_verification_code("test@example.com", "999999").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn mock_email_send_recovery_code_succeeds() {
        let svc = MockEmailService::new();
        let result = svc.send_recovery_code("test@example.com", "111111").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn smtp_email_service_initializes_with_valid_config() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_username: "user".to_string(),
            smtp_password: "pass".to_string(),
            from_address: "noreply@example.com".to_string(),
            from_name: "OpenConv".to_string(),
        };
        let result = SmtpEmailService::new(&config);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn smtp_email_service_fails_with_invalid_config() {
        let config = EmailConfig {
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_username: String::new(),
            smtp_password: String::new(),
            from_address: "not-an-email".to_string(),
            from_name: String::new(),
        };
        let result = SmtpEmailService::new(&config);
        assert!(result.is_err());
    }
}
