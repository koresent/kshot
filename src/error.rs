use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum KshotError {
    #[error("Wayland connection error: {0}")]
    WaylandConnection(String),

    #[error("Wayland protocol error: {0}")]
    WaylandProtocol(String),

    #[error("Screen capture failed: {0}")]
    CaptureFailed(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Image encoding/decoding error: {0}")]
    Image(#[from] image::ImageError),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Geometry parse error: {0}")]
    InvalidGeometry(String),
}

pub type Result<T> = std::result::Result<T, KshotError>;
