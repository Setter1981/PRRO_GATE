//! SoftBalance XML-profile driven ESC/POS receipt compiler.
//!
//! PRRO Gateway ships with a handful of bundled vendor printer profiles
//! (Epson TM-T88II, Posiflex PP-8000 LAN, Citizen CT-S310II).  Each is
//! a data-driven command dictionary:
//!
//! ```xml
//! <printer>
//!   <info><name>TM-T88II</name>...</info>
//!   <hardware><interface>COM,LPT,LAN</interface>...</hardware>
//!   <language>
//!     <command name="Cut" code="1D5600" length="3">
//!       <value byte="2" name="DEFAULT" dvalue="1" />
//!     </command>
//!     <command name="Center" code="1B6101" length="3" />
//!     ...
//!     <procedure name="PrintBarCode" title="TYPE,WIDTH,HEIGHT,...">
//!       <prm parameter="3" command="HRIFont" />
//!       ...
//!     </procedure>
//!   </language>
//! </printer>
//! ```
//!
//! The library exposes:
//! - [`PrinterProfile`] — parsed XML profile
//! - [`CommandValue`] — parameter for a named enum-style command
//! - [`ReceiptCompiler`] — builds an ESC/POS byte stream from a sequence
//!   of semantic instructions
//! - [`Error`] — typed errors for unknown commands, bad profile XML,
//!   codepage issues.
//!
//! The compiler never performs I/O — it returns `Vec<u8>` that the
//! caller writes to TCP:9100 / Serial / USB.  That separation keeps
//! the library unit-testable with pure golden bytes.
pub mod compiler;
pub mod error;
pub mod executor;
pub mod profile;

pub use compiler::{Alignment, CodePage, Instruction, ReceiptCompiler};
pub use error::{Error, Result};
pub use executor::Executor;
pub use profile::{Command, CommandValue, PrinterProfile, Procedure};

/// Bundled vendor profiles distributed with the crate.  Operators can
/// load their own XML via [`PrinterProfile::from_xml_str`] instead.
pub mod bundled {
    /// Epson TM-T88II — most common Ukrainian retail thermal.
    pub const EPSON_TM_T88II: &str = include_str!("../assets/tm-t88ii.xml");
    /// Posiflex Aura PP-8000 LAN.
    pub const POSIFLEX_PP_8000_LAN: &str = include_str!("../assets/pp8000l.xml");
    /// Citizen CT-S310II.
    pub const CITIZEN_CT_S310II: &str = include_str!("../assets/cts310ii.xml");
}
