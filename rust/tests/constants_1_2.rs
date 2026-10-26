// SPDX-License-Identifier: Apache-2.0
//! DIFI 1.2 constants — spec `difi-1.2-dataplane` task 1.
//!
//! Verifies the 1.2 class/type additions have the authoritative values (Table 4-1) and
//! that no DIFI 1.1 constant value changed. The matching C++ assertions live in
//! `cpp/tests/test_codec.cpp` (`constants_1_2`); the two implementations MUST agree.

use difi::constants::*;

#[test]
fn difi_1_1_constants_unchanged() {
    // These are the shipped 1.1 values; the 1.2 upgrade must not alter them.
    assert_eq!(VITA_OUI, 0x006A_621E);
    assert_eq!(HEADER_WORDS, 7);
    assert_eq!(packet_type::SIGNAL_DATA, 0x1);
    assert_eq!(packet_type::CONTEXT, 0x4);
    assert_eq!(version_packet_type::VERSION_CONTEXT, 0x5);
    assert_eq!(information_class::STANDARD_FLOW, 0x0000);
    assert_eq!(information_class::VERSION_FLOW, 0x0001);
    assert_eq!(packet_class::SIGNAL_DATA, 0x0000);
    assert_eq!(packet_class::SIGNAL_CONTEXT, 0x0001);
    assert_eq!(packet_class::VERSION_CONTEXT, 0x0004);
    assert_eq!(context::WORDS, 27);
    assert_eq!(version::WORDS, 11);
}

#[test]
fn difi_1_2_packet_type_command() {
    assert_eq!(packet_type::COMMAND, 0x6);
}

#[test]
fn difi_1_2_information_classes() {
    assert_eq!(information_class::DATA_PLANE_FLOW_CONTROL, 0x0002);
    assert_eq!(information_class::DATA_PLANE_FLOW_CONTROL_RT, 0x0003);
    assert_eq!(information_class::BASIC_DATA_PLANE_2, 0x0004);
}

#[test]
fn difi_1_2_packet_classes() {
    assert_eq!(packet_class::SAMPLE_COUNT_SIGNAL_DATA, 0x0002);
    assert_eq!(packet_class::SAMPLE_COUNT_SIGNAL_CONTEXT, 0x0003);
    assert_eq!(packet_class::SAMPLE_COUNT_FLOW_CONTROL, 0x0005);
    assert_eq!(packet_class::REAL_TIME_FLOW_CONTROL, 0x0006);
}

#[test]
fn difi_1_2_command_and_buffer_status() {
    assert_eq!(command::WORDS, 21);
    assert_eq!(command::REF_POINT, 0x0000_0064);
    // Buffer Status flags occupy the low nibble: OF=3, NF=2, NE=1, UF=0.
    assert_eq!(buffer_status::OVERFLOW_BIT, 3);
    assert_eq!(buffer_status::NEAR_FULL_BIT, 2);
    assert_eq!(buffer_status::NEAR_EMPTY_BIT, 1);
    assert_eq!(buffer_status::UNDERFLOW_BIT, 0);
}

/// Information-Class and Packet-Class codes share numeric values but are different
/// fields; confirm the sample-count codes are consistent across both namespaces where
/// they are meant to line up, and that the Command type is distinct from data/context.
#[test]
fn class_and_type_namespaces_are_distinct_but_consistent() {
    // Packet Class 0x0004 (Version Context, low 16 bits) and Information Class 0x0004
    // (Basic Data Plane 2, high 16 bits) intentionally share the number 4.
    assert_eq!(
        packet_class::VERSION_CONTEXT,
        information_class::BASIC_DATA_PLANE_2
    );
    // The three VRT packet types we now handle are distinct.
    assert_ne!(packet_type::SIGNAL_DATA, packet_type::CONTEXT);
    assert_ne!(packet_type::CONTEXT, packet_type::COMMAND);
    assert_ne!(packet_type::SIGNAL_DATA, packet_type::COMMAND);
}
