// CONFI — Windows Shell Integration Prototype Library

pub mod client;
pub mod protocol;
pub mod server;
pub mod uri;

pub use client::{connect_to_pipe, get_pipe_name, send_pipe_request, IpcClientError};
pub use protocol::{
    read_framed_message, validate_request, write_framed_message, ConfiRequest, ConfiResponse,
    IpcError, DEFAULT_MAX_PAYLOAD_SIZE,
};
pub use server::{handle_client_stream, DefaultQueueHandler, RequestHandler};
pub use uri::{build_deep_link, parse_deep_link, percent_decode, percent_encode_str};

