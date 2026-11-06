// CONFI — Windows Shell Integration Prototype Library
//
// Modules are public; callers import from them directly (`confi_shell::protocol::…`).
// There are no flat re-exports on top of the modules: one import path per item.

pub mod client;
pub mod protocol;
pub mod server;
pub mod uri;
