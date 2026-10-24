//! Why the controller could not be built.

use std::fmt;

use crate::demo::server::DemoError;
use crate::session::sessionmanager::SessionManagerError;

/// Why the controller could not be built.
#[derive(Debug)]
pub enum ControllerError {
    /// A seeded session could not be registered.
    Session(SessionManagerError),
    /// The demo could not attach or seed a network.
    Demo(DemoError),
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => write!(formatter, "{error}"),
            Self::Demo(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ControllerError {}

impl From<SessionManagerError> for ControllerError {
    fn from(error: SessionManagerError) -> Self {
        Self::Session(error)
    }
}

impl From<DemoError> for ControllerError {
    fn from(error: DemoError) -> Self {
        Self::Demo(error)
    }
}
