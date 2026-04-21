//! Typed errors for the ESC/POS driver.  Everything reports as a
//! single `Error` enum so callers can match on semantic failure modes
//! without string-parsing.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("xml parse error: {0}")]
    Xml(String),

    #[error("invalid hex code {found:?} for command {command}")]
    InvalidHex { command: String, found: String },

    #[error("unknown command: {0}")]
    UnknownCommand(String),

    #[error("unknown command value {value:?} for command {command}")]
    UnknownValue { command: String, value: String },

    #[error("unknown procedure: {0}")]
    UnknownProcedure(String),

    #[error("codepage encoding failure: byte not representable ({char:?})")]
    Codepage { char: char },

    #[error("command {command} requires a parameter value")]
    MissingParameter { command: String },

    #[error("command {command} does not take a parameter but was given {value:?}")]
    UnexpectedParameter { command: String, value: String },
}

pub type Result<T> = std::result::Result<T, Error>;
