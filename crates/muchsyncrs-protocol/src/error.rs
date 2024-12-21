#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Deserialization failed")]
    Deserialization(#[source] serde_json::Error),

    #[error("Serialization failed")]
    Serialization(#[source] serde_json::Error),

    #[error("Error reading line")]
    ReadingLine(#[source] std::io::Error),

    #[error("Error writing line")]
    WritingLine(#[source] std::io::Error),

}
