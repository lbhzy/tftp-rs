mod packet;
mod window;
mod server;
mod session;

pub use crate::server::TftpServer;
pub use crate::session::SessionConfig;

pub const DEF_BLOCK_SIZE: u16 = 512; // RFC 1350
pub const MIN_BLOCK_SIZE: u16 = 8; // RFC 2348
pub const MAX_BLOCK_SIZE: u16 = 65464; // RFC 2348

pub const DEF_WINDOW_SIZE: u16 = 1;
