// SPDX-License-Identifier: Apache-2.0
//! # difi — reference DIFI 1.1 I/Q codec (independent Rust implementation)
//!
//! A strict, fail-closed encoder/decoder for the DIFI 1.1 profile of VITA 49.2
//! (IEEE-ISTO Std 4900-2021). This is one of two independent implementations in the
//! `difi-interop` toolkit (the other is C++); both are validated against the same
//! committed golden packets so neither is "correct by its own definition".
//!
//! See `docs/reference/DIFI-VITA49-PRIMER.md` and `.kiro/specs/difi-iq-interop/`.

pub mod constants;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod iq;
pub mod manifest;
pub mod mapper;
pub mod packet;
pub mod sha256;
pub mod signal;
pub mod transport;

pub use decoder::{Decoder, DifiPacket};
pub use encoder::StreamEncoder;
pub use error::{DifiError, Result};
pub use iq::IqSample;
pub use manifest::{build as build_manifest, DifiContext, Manifest};
pub use mapper::{
    difi_to_float, float_to_difi, measure_implementation_loss, LossReport, ScalingConvention,
};
pub use packet::{
    DataFormat, Header, SignalContextPacket, SignalDataPacket, VersionContextPacket,
};
pub use signal::{generate as generate_signal, SignalParams};
pub use transport::{peek_stream_id, TcpReceiver, TcpSender, UdpReceiver, UdpSender};
