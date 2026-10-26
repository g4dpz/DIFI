//! UDP transport tests (spec task 9 / Req 5): loopback exchange and Stream-ID filtering.

use std::time::Duration;

use difi::{peek_stream_id, Decoder, DifiPacket, IqSample, StreamEncoder, UdpReceiver, UdpSender};

#[test]
fn loopback_exchange_over_udp() {
    // Receiver on an ephemeral port.
    let mut rx = UdpReceiver::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let addr = rx.local_addr().unwrap();

    let stream_id = 0x0BAD_F00D;
    let mut enc = StreamEncoder::new(stream_id, 16);
    let tx = UdpSender::new(addr).unwrap();

    let ctx = enc.encode_context(1e9, 1e6, 1e6, 0.0, 0, 0).unwrap();
    tx.send(&ctx).unwrap();
    let samples = vec![IqSample::new(7, -7); 16];
    let data = enc.encode_data(&samples, 0, 0).unwrap();
    tx.send(&data).unwrap();

    let mut dec = Decoder::new();
    let (b1, _) = rx.recv().unwrap();
    assert!(matches!(
        dec.decode(&b1).unwrap(),
        DifiPacket::SignalContext(_)
    ));
    let (b2, _) = rx.recv().unwrap();
    match dec.decode(&b2).unwrap() {
        DifiPacket::SignalData(d) => assert_eq!(d.samples, samples),
        _ => panic!("expected data"),
    }
}

#[test]
fn stream_id_filter_separates_streams() {
    let mut rx = UdpReceiver::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    rx.set_stream_filter(Some(0xAAAA_0001));
    let addr = rx.local_addr().unwrap();

    let tx = UdpSender::new(addr).unwrap();
    // Interleave two streams on the same port; receiver must only yield 0xAAAA0001.
    let other = StreamEncoder::new(0xBBBB_0002, 16);
    let mut wanted = StreamEncoder::new(0xAAAA_0001, 16);
    let other_ctx = other.encode_context(1e9, 1e6, 1e6, 0.0, 0, 0).unwrap();
    let wanted_ctx = wanted.encode_context(2e9, 1e6, 1e6, 0.0, 0, 0).unwrap();
    let wanted_data = wanted
        .encode_data(&vec![IqSample::new(1, 2); 16], 0, 0)
        .unwrap();

    // Send an off-stream packet first; it must be dropped by the filter.
    tx.send(&other_ctx).unwrap();
    tx.send(&wanted_ctx).unwrap();
    tx.send(&wanted_data).unwrap();

    let (b1, _) = rx.recv().unwrap();
    assert_eq!(peek_stream_id(&b1), Some(0xAAAA_0001));
    let (b2, _) = rx.recv().unwrap();
    assert_eq!(peek_stream_id(&b2), Some(0xAAAA_0001));
}
