use crate::operation::MuchsyncProtocolMessage;

impl MuchsyncProtocolMessage for Error {
    const MESSAGE_NAME: &'static str = "error";
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Error {
    /// Error message
    pub message: String,
}

impl MuchsyncProtocolMessage for ErrorAcknowledgement {
    const MESSAGE_NAME: &'static str = "errorAck";
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ErrorAcknowledgement {
    // empty
}
