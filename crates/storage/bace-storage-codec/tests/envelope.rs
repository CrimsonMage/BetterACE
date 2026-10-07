use bace_storage_codec::{CodecError, CodecLimits, decode, encode, inspect};

#[test]
fn envelope_fields_and_payload_have_fixed_format() {
    let bytes = encode(1, 1, &42_u32, CodecLimits::default()).unwrap();
    assert_eq!(
        &bytes[..20],
        &[
            65, 67, 69, 82, 66, 73, 78, 0, 1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0
        ]
    );
    // Independent hashlib SHA256 of the documented header bytes + 0x2a.
    assert_eq!(
        &bytes[20..52],
        &[
            16, 11, 185, 112, 166, 196, 34, 10, 106, 152, 165, 220, 227, 185, 201, 41, 8, 233, 200,
            248, 83, 48, 26, 170, 46, 98, 208, 183, 96, 128, 237, 28
        ]
    );
    assert_eq!(&bytes[52..], &[42]);
    assert_eq!(
        decode::<u32>(&bytes, 1, 1, CodecLimits::default()).unwrap(),
        42
    );
    assert_eq!(
        inspect(&bytes, CodecLimits::default())
            .unwrap()
            .payload_bytes,
        1
    );
}

#[test]
fn reject_corruption_truncation_unsupported_schema_and_limits() {
    let bytes = encode(1, 1, &42_u32, CodecLimits::default()).unwrap();
    for len in 0..bytes.len() {
        assert!(inspect(&bytes[..len], CodecLimits::default()).is_err());
    }
    let mut corrupt = bytes.clone();
    corrupt[52] ^= 1;
    assert!(matches!(
        inspect(&corrupt, CodecLimits::default()),
        Err(CodecError::Integrity)
    ));
    assert!(matches!(
        decode::<u32>(&bytes, 1, 2, CodecLimits::default()),
        Err(CodecError::Schema { .. })
    ));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        inspect(&trailing, CodecLimits::default()),
        Err(CodecError::Length)
    ));
    assert!(matches!(
        inspect(
            &bytes,
            CodecLimits {
                max_payload_bytes: 0
            }
        ),
        Err(CodecError::Limit)
    ));
    assert!(matches!(
        encode(
            1,
            1,
            &"large",
            CodecLimits {
                max_payload_bytes: 1
            }
        ),
        Err(CodecError::Limit)
    ));
}
