//! Email Alert — Send alerts via email.

pub struct EmailAlert {
    smtp_server: String,
    from: String,
    to: String,
}

impl EmailAlert {
    pub fn new(smtp_server: &str, from: &str, to: &str) -> Self {
        Self {
            smtp_server: smtp_server.to_string(),
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    pub async fn send(&self, subject: &str, body: &str) -> Result<(), String> {
        let _ = (subject, body);
        // TODO: Implement SMTP sending
        Ok(())
    }
}
