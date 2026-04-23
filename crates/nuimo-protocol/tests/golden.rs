//! Golden byte-vector tests pinning the Nuimo GATT wire format.
//!
//! If any of these break, either the Nuimo firmware protocol changed, or
//! `nuimo-protocol`'s parse/encode diverged from `nuimo-rs` — investigate
//! before updating the expectations.

use nuimo_protocol::{
    build_led_payload, parse_notification, DisplayOptions, DisplayTransition, Glyph, NuimoEvent,
    ParseError, BUTTON_CLICK, FLY, ROTATION, TOUCH_OR_SWIPE,
};

// --- Button ---------------------------------------------------------------

#[test]
fn button_down_parses_as_button_down() {
    let ev = parse_notification(&BUTTON_CLICK, &[0x01]).unwrap();
    assert_eq!(ev, Some(NuimoEvent::ButtonDown));
}

#[test]
fn button_up_parses_as_button_up() {
    let ev = parse_notification(&BUTTON_CLICK, &[0x00]).unwrap();
    assert_eq!(ev, Some(NuimoEvent::ButtonUp));
}

#[test]
fn button_empty_payload_is_error() {
    let err = parse_notification(&BUTTON_CLICK, &[]).unwrap_err();
    assert!(matches!(err, ParseError::TooShort { kind: "button_click", got: 0, need: 1 }));
}

// --- Rotation -------------------------------------------------------------

#[test]
fn rotation_positive_100_produces_expected_delta() {
    // 100 points / 2650 points-per-cycle
    let ev = parse_notification(&ROTATION, &[0x64, 0x00]).unwrap().unwrap();
    match ev {
        NuimoEvent::Rotate { delta, rotation } => {
            assert!((delta - 100.0 / 2650.0).abs() < 1e-9, "delta was {delta}");
            assert_eq!(rotation, 0.0);
        }
        other => panic!("expected Rotate, got {other:?}"),
    }
}

#[test]
fn rotation_negative_100_produces_negative_delta() {
    // i16::from_le_bytes([0x9C, 0xFF]) = -100
    let ev = parse_notification(&ROTATION, &[0x9C, 0xFF]).unwrap().unwrap();
    match ev {
        NuimoEvent::Rotate { delta, .. } => {
            assert!((delta + 100.0 / 2650.0).abs() < 1e-9, "delta was {delta}");
        }
        other => panic!("expected Rotate, got {other:?}"),
    }
}

#[test]
fn rotation_single_byte_is_too_short() {
    let err = parse_notification(&ROTATION, &[0x64]).unwrap_err();
    assert!(matches!(err, ParseError::TooShort { kind: "rotation", got: 1, need: 2 }));
}

// --- Swipe / Touch --------------------------------------------------------

#[test]
fn swipe_left_code_0() {
    assert_eq!(
        parse_notification(&TOUCH_OR_SWIPE, &[0x00]).unwrap(),
        Some(NuimoEvent::SwipeLeft)
    );
}

#[test]
fn swipe_right_code_1() {
    assert_eq!(
        parse_notification(&TOUCH_OR_SWIPE, &[0x01]).unwrap(),
        Some(NuimoEvent::SwipeRight)
    );
}

#[test]
fn swipe_up_code_2() {
    assert_eq!(
        parse_notification(&TOUCH_OR_SWIPE, &[0x02]).unwrap(),
        Some(NuimoEvent::SwipeUp)
    );
}

#[test]
fn swipe_down_code_3() {
    assert_eq!(
        parse_notification(&TOUCH_OR_SWIPE, &[0x03]).unwrap(),
        Some(NuimoEvent::SwipeDown)
    );
}

#[test]
fn touch_edges_codes_4_through_7() {
    let cases = [
        (4, NuimoEvent::TouchLeft),
        (5, NuimoEvent::TouchRight),
        (6, NuimoEvent::TouchTop),
        (7, NuimoEvent::TouchBottom),
    ];
    for (code, expected) in cases {
        assert_eq!(
            parse_notification(&TOUCH_OR_SWIPE, &[code]).unwrap(),
            Some(expected),
            "code {code}"
        );
    }
}

#[test]
fn long_touch_codes_8_through_11() {
    let cases = [
        (8, NuimoEvent::LongTouchLeft),
        (9, NuimoEvent::LongTouchRight),
        (10, NuimoEvent::LongTouchTop),
        (11, NuimoEvent::LongTouchBottom),
    ];
    for (code, expected) in cases {
        assert_eq!(
            parse_notification(&TOUCH_OR_SWIPE, &[code]).unwrap(),
            Some(expected),
            "code {code}"
        );
    }
}

#[test]
fn unknown_touch_code_returns_none() {
    // Unknown codes are dropped, not errors (firmware reserves future codes).
    assert_eq!(parse_notification(&TOUCH_OR_SWIPE, &[0xFF]).unwrap(), None);
}

// --- Fly ------------------------------------------------------------------

#[test]
fn fly_left_and_right() {
    assert_eq!(
        parse_notification(&FLY, &[0x00]).unwrap(),
        Some(NuimoEvent::FlyLeft)
    );
    assert_eq!(
        parse_notification(&FLY, &[0x01]).unwrap(),
        Some(NuimoEvent::FlyRight)
    );
}

#[test]
fn fly_hover_returns_clamped_proximity() {
    // raw=125 (mid-range) -> (125-2)/(250-2-1) = 123/247 ≈ 0.498
    let ev = parse_notification(&FLY, &[0x04, 125]).unwrap().unwrap();
    match ev {
        NuimoEvent::Hover { proximity } => {
            assert!(proximity > 0.49 && proximity < 0.51, "proximity was {proximity}");
        }
        other => panic!("expected Hover, got {other:?}"),
    }
}

// --- Unknown characteristic ----------------------------------------------

#[test]
fn unknown_characteristic_is_error() {
    let unknown = uuid::Uuid::from_u128(0xdeadbeef_dead_beef_dead_beefdeadbeef);
    let err = parse_notification(&unknown, &[0x00]).unwrap_err();
    assert!(matches!(err, ParseError::UnknownCharacteristic(u) if u == unknown));
}

// --- LED encode -----------------------------------------------------------

#[test]
fn empty_glyph_default_opts_encodes_to_known_bytes() {
    let bytes = build_led_payload(&Glyph::empty(), &DisplayOptions::default());
    // 11 bytes bitmap (all zero), brightness 255, timeout 2000/100 = 20.
    assert_eq!(
        bytes,
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 20],
        "crossfade + max brightness + 2s timeout should match wire format"
    );
}

#[test]
fn single_pixel_top_left_sets_bit_0() {
    let g = Glyph::from_ascii("*");
    let bytes = build_led_payload(&g, &DisplayOptions::default());
    assert_eq!(bytes[0], 0x01);
    assert_eq!(&bytes[1..11], &[0u8; 10]);
    assert_eq!(bytes[11], 255);
    assert_eq!(bytes[12], 20);
}

#[test]
fn full_first_row_fills_9_bits_across_bytes_0_and_1() {
    let g = Glyph::from_ascii("*********");
    let bytes = build_led_payload(&g, &DisplayOptions::default());
    assert_eq!(bytes[0], 0xFF);
    assert_eq!(bytes[1], 0x01);
    assert_eq!(&bytes[2..11], &[0u8; 9]);
}

#[test]
fn immediate_transition_sets_fade_flag_on_byte_10() {
    let g = Glyph::empty();
    let immediate = build_led_payload(
        &g,
        &DisplayOptions {
            transition: DisplayTransition::Immediate,
            ..DisplayOptions::default()
        },
    );
    // Byte 10 of an empty glyph is 0; Immediate XORs in LED_FADE_FLAG (0b0001_0000).
    assert_eq!(immediate[10], 0b0001_0000);

    let crossfade = build_led_payload(&g, &DisplayOptions::default());
    assert_eq!(crossfade[10], 0);
}

#[test]
fn half_brightness_and_5_second_timeout() {
    let bytes = build_led_payload(
        &Glyph::empty(),
        &DisplayOptions {
            brightness: 0.5,
            timeout_ms: 5000,
            transition: DisplayTransition::CrossFade,
        },
    );
    assert_eq!(bytes[11], 127); // floor(0.5 * 255)
    assert_eq!(bytes[12], 50); // 5000 / 100
}

#[test]
fn filled_glyph_has_81_bits_set() {
    let bytes = build_led_payload(&Glyph::filled(), &DisplayOptions::default());
    let total_bits: u32 = bytes[..11].iter().map(|b| b.count_ones()).sum();
    assert_eq!(total_bits, 81);
}
