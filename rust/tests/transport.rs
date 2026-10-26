// SPDX-License-Identifier: Apache-2.0
//! UDP transport tests (spec task 9 / Req 5): loopback exchange and Stream-ID filtering.

use std::thread;
use std::time::Duration;

use difi::{
    peek_stream_id, Decoder, DifiPacket, IqSample, StreamEncoder, TcpReceiver, TcpSender,
    UdpReceiver, UdpSender,
};

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

#[test]
fn tcp_dev_mode_exchange_lossless() {
    // Req 5.2 (non-standard dev mode): length-framed DIFI over TCP round-trips exactly.
    let mut rx = TcpReceiver::bind("127.0.0.1:0").unwrap();
    let addr = rx.local_addr().unwrap();

    let samples: Vec<IqSample> = (0..32).map(|k| IqSample::new(k - 16, 16 - k)).collect();
    let samples_tx = samples.clone();

    // Sender runs in a thread (receiver.accept blocks until it connects).
    let tx = thread::spawn(move || {
        let mut sender = TcpSender::connect(addr).unwrap();
        let mut enc = StreamEncoder::new(0xFEED_0001, 16);
        let ctx = enc.encode_context(1e9, 1e6, 1e6, 0.0, 0, 0).unwrap();
        sender.send(&ctx).unwrap();
        for _ in 0..5 {
            let d = enc.encode_data(&samples_tx, 0, 0).unwrap();
            sender.send(&d).unwrap();
        }
        // Dropping the sender closes the connection (clean EOF for the receiver).
    });

    let mut dec = Decoder::new();
    // First packet is the context.
    let b1 = rx.recv().unwrap().expect("context");
    assert!(matches!(
        dec.decode(&b1).unwrap(),
        DifiPacket::SignalContext(_)
    ));
    let mut data_seen = 0;
    while let Some(buf) = rx.recv().unwrap() {
        match dec.decode(&buf).unwrap() {
            DifiPacket::SignalData(d) => {
                assert_eq!(d.samples, samples);
                data_seen += 1;
            }
            _ => panic!("expected data"),
        }
    }
    assert_eq!(data_seen, 5, "all 5 data packets received losslessly");
    tx.join().unwrap();
}

#[test]
fn tcp_stream_id_filter() {
    let mut rx = TcpReceiver::bind("127.0.0.1:0").unwrap();
    rx.set_stream_filter(Some(0xAAAA_0001));
    let addr = rx.local_addr().unwrap();

    let tx = thread::spawn(move || {
        let mut sender = TcpSender::connect(addr).unwrap();
        let mut other = StreamEncoder::new(0xBBBB_0002, 16);
        let mut wanted = StreamEncoder::new(0xAAAA_0001, 16);
        // Interleave; only the wanted stream should be delivered.
        sender.send(&other.encode_context(1e9, 1e6, 1e6, 0.0, 0, 0).unwrap()).unwrap();
        sender.send(&wanted.encode_context(2e9, 1e6, 1e6, 0.0, 0, 0).unwrap()).unwrap();
        sender
            .send(&other.encode_data(&vec![IqSample::new(9, 9); 16], 0, 0).unwrap())
            .unwrap();
        sender
            .send(&wanted.encode_data(&vec![IqSample::new(1, 2); 16], 0, 0).unwrap())
            .unwrap();
    });

    let b1 = rx.recv().unwrap().unwrap();
    assert_eq!(peek_stream_id(&b1), Some(0xAAAA_0001));
    let b2 = rx.recv().unwrap().unwrap();
    assert_eq!(peek_stream_id(&b2), Some(0xAAAA_0001));
    tx.join().unwrap();
}
