use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum BuildError {
    BuilderIncomplete(&'static str),
    Duplicate(String, String),
    ConnectorNotFound(String),
    Error(anyhow::Error),
}

impl Display for BuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for BuildError {}

impl From<anyhow::Error> for BuildError {
    fn from(value: anyhow::Error) -> Self {
        BuildError::Error(value)
    }
}
