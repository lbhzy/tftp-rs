mod cli;
mod packet;
mod window;

pub use crate::cli::Cli;
pub use crate::packet::TftpPacket;
pub use crate::window::Window;

pub const DEF_BLOCK_SIZE: u16 = 512; // RFC 1350
pub const MIN_BLOCK_SIZE: u16 = 8; // RFC 2348
pub const MAX_BLOCK_SIZE: u16 = 65464; // RFC 2348

pub const DEF_WINDOW_SIZE: u16 = 1;
pub const DEF_TIMEOUT_SEC: u64 = 1;
pub const MAX_RETRY_COUNT: u8 = 3;
