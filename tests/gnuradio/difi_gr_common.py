"""Shared helpers for the GNU Radio <-> difi-interop interop test (Route 1).

Reproduces the SAME reference signal and float->DIFI-integer mapping as the Rust/C++
`signal.rs` + `mapper.rs`, and builds/parses DIFI packets with the DIFI Consortium
`construct` definitions (fetched by tests/conformance/setup.sh). This lets a GNU Radio
flowgraph exchange the exact byte stream our `difi-tool` export/import expects.

The point of Route 1 is to put GNU Radio's runtime + its UDP (socket_pdu) block in the
path. The DIFI packet logic here is the Consortium reference, not an independent codec;
gr-difi (Route 2) would replace that with GNU Radio's own DIFI blocks.
"""
import math
import os
import struct
import sys

# DIFI Consortium reference definitions (fetched into tests/conformance/.difi-cert).
_CONF = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "conformance", ".difi-cert")
if not os.path.isdir(os.path.join(_CONF, "packet_definitions")):
    sys.stderr.write("Run tests/conformance/setup.sh first (fetches DIFI reference defs).\n")
    sys.exit(3)
sys.path.insert(0, _CONF)

from construct import Container  # noqa: E402
from packet_definitions.difi_data_v1_1 import difi_data_definition as DATA  # noqa: E402

# Must match the difi-tool export/import constants.
STREAM_ID = 0x12345678
INT_SECONDS = 1700000000
FRAC_PS = 123456789000
BIT_DEPTH = 16
FULL_SCALE = 1.0
SAMPLES_PER_PACKET = 256

# Must match rust/src/signal.rs SignalParams::default().
NUM_SAMPLES = 4096
SAMPLE_RATE_HZ = 1.25e6
TONE1_HZ = 50000.0
TONE2_HZ = -120000.0
PEAK = 0.9


def generate_signal():
    """Reproduce the reference float32 I/Q signal (mirrors signal.rs::generate)."""
    tau = 2.0 * math.pi
    w1 = tau * TONE1_HZ / SAMPLE_RATE_HZ
    w2 = tau * TONE2_HZ / SAMPLE_RATE_HZ
    raw = []
    max_mag = 0.0
    for k in range(NUM_SAMPLES):
        t = float(k)
        env = 0.75 + 0.25 * math.cos(tau * k / NUM_SAMPLES)
        re = env * (math.cos(w1 * t) + 0.6 * math.cos(w2 * t))
        im = env * (math.sin(w1 * t) + 0.6 * math.sin(w2 * t))
        mag = math.hypot(re, im)
        if mag > max_mag:
            max_mag = mag
        raw.append((re, im))
    scale = (PEAK / max_mag) if max_mag > 0 else 0.0
    # Match Rust: values are stored as f32 at generation time.
    return [(_as_f32(re * scale), _as_f32(im * scale)) for (re, im) in raw]


def _as_f32(x):
    """Round a Python float to float32 precision, as Rust's f32 cast does."""
    return struct.unpack("f", struct.pack("f", x))[0]


def float_to_code(x):
    """Mirror mapper.rs ScalingConvention::float_to_code (16-bit, full_scale=1.0)."""
    fs = (1 << (BIT_DEPTH - 1)) - 1  # 32767
    mn, mx = -(1 << (BIT_DEPTH - 1)), fs
    norm = max(-1.0, min(1.0, x / FULL_SCALE))
    scaled = norm * fs
    rounded = math.floor(scaled + 0.5) if scaled >= 0 else math.ceil(scaled - 0.5)
    return max(mn, min(mx, int(rounded)))


def map_signal_to_codes(iq):
    return [(float_to_code(i), float_to_code(q)) for (i, q) in iq]


def build_context_packet():
    """Build a DIFI Signal Context packet (27 words) with the reference definition."""
    from packet_definitions.difi_context_v1_1 import difi_context_definition as CTX

    def fix64(v):
        return int(round(v * (2 ** 20)))

    def fix16(v):
        return int(round(v * (2 ** 7)))

    pkt = Container(
        header=Container(pktType=4, classId=1, reserved=0, tsm=1, tsi="UTC",
                         tsf=2, seqNum=0, pktSize=27),
        streamId=STREAM_ID,
        classId=Container(paddingBits=0, reserved1=0, oui=0x6A621E,
                          infoClassCode=0, packetClassCode=1),
        intSecsTimestamp=INT_SECONDS, fracSecsTimestamp=FRAC_PS,
        cif0=0xFBB98000, refPoint=0x64,
        bandwidth=fix64(SAMPLE_RATE_HZ) / (2 ** 20),
        ifFreq=0.0, rfFreq=fix64(2.401e9) / (2 ** 20), ifBandOffset=0.0,
        refLevel1=0.0, refLevel2=0.0,
        stage1GainAtten=fix16(-3.5) / (2 ** 7), stage2GainAtten=0.0,
        sampleRate=fix64(SAMPLE_RATE_HZ) / (2 ** 20),
        timeStampAdj=0, timeStampCal=0,
        stateEventInd=Container(misc_enables=0, reserved=0, calibrated_time_indicator=0,
                                valid_data_indicator=0, reference_lock_indicator=0,
                                agc_mgc_indicator=0, detected_signal_indicator=0,
                                spectral_inversion_indicator=0, over_range_indicator=0,
                                sample_loss_indicator=0, reserved2=0, user_defined=0),
        dataPacketFormat=Container(
            packing_method="link_efficient", real_complex_type="complex_cartesian",
            data_item_format="signed_fixed_point", sample_repeat_indicator="no_repeat",
            event_tag_size=0, channel_tag_size=0, data_item_fraction_size=0,
            item_packing_field_size=BIT_DEPTH - 1, data_item_size=BIT_DEPTH - 1,
            repeat_count=0, vector_size=0))
    return CTX.build(pkt)


def build_data_packet(samples, seq):
    """Build one DIFI Signal Data packet with the Consortium reference definition."""
    payload = b"".join(struct.pack(">hh", i, q) for (i, q) in samples)
    pkt = Container(
        header=Container(pktType=1, classId=1, reserved=0, tsm=0, tsi="UTC",
                         tsf=2, seqNum=seq & 0xF, pktSize=7 + len(payload) // 4),
        streamId=STREAM_ID,
        classId=Container(paddingBits=0, reserved1=0, oui=0x6A621E,
                          infoClassCode=0, packetClassCode=0),
        intSecsTimestamp=INT_SECONDS, fracSecsTimestamp=FRAC_PS, payload=payload)
    return DATA.build(pkt)


def parse_data_samples(raw):
    """Parse a DIFI Signal Data packet, returning (seq, [(i,q),...]) or None if not data."""
    parsed = DATA.parse(raw)
    if parsed.header.pktType != 1:
        return None
    payload = parsed.payload
    n = len(payload) // 4
    samples = [struct.unpack_from(">hh", payload, k * 4) for k in range(n)]
    return int(parsed.header.seqNum), samples
