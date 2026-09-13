use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Audio error: {0}")]
    Audio(#[from] hound::Error),
    #[error("Speech recognition error: {0}")]
    Speech(String),
    #[error("Credential store error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Threadbox could not locate its data directory")]
    DataDirectory,
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
