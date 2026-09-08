use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderFault {
    #[error("provider rate limited the request: {message}")]
    RateLimited { message: String },
    #[error("provider payment is required: {message}")]
    PaymentRequired { message: String },
    #[error("provider authentication failed: {message}")]
    Authentication { message: String },
    #[error("model context overflowed: {message}")]
    ContextOverflow { message: String },
    #[error("provider upstream failed with HTTP {status}: {message}")]
    Upstream { status: u16, message: String },
    #[error("provider returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("provider transport failed: {message}")]
    Transport { message: String },
    #[error("provider stream failed: {message}")]
    Stream { message: String },
    #[error("provider response was invalid: {message}")]
    InvalidResponse { message: String },
    #[error("provider returned no assistant content or tool call")]
    EmptyCompletion,
    #[error("tool call {index} was malformed: {message}")]
    MalformedToolCall { index: usize, message: String },
    #[error("provider configuration is invalid: {message}")]
    Configuration { message: String },
}

impl ProviderFault {
    pub fn from_http(status: u16, message: impl Into<String>) -> Self {
        let message = message.into();
        let normalized = message.to_ascii_lowercase();
        match status {
            429 => Self::RateLimited { message },
            402 => Self::PaymentRequired { message },
            401 | 403 => Self::Authentication { message },
            400 | 413
                if normalized.contains("context")
                    && (normalized.contains("length")
                        || normalized.contains("token")
                        || normalized.contains("overflow")) =>
            {
                Self::ContextOverflow { message }
            }
            500..=599 => Self::Upstream { status, message },
            _ => Self::Http { status, message },
        }
    }
}
