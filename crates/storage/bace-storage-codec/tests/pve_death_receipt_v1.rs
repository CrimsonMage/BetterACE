use bace_storage_codec::{CodecLimits, PVE_DEATH_RECEIPT_KIND, PveDeathReceiptV1, encode};

fn receipt() -> PveDeathReceiptV1 {
    PveDeathReceiptV1 {
        marker_object_id: 0x8000_0010,
        event_id: [7; 16],
        victim_object_id: 0x5000_0001,
        position: bace_content::Position {
            obj_cell_id: 0x1234_0100,
            position_x: 1.,
            position_y: 2.,
            position_z: 3.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        },
        world_epoch: 9,
        killed: true,
    }
}

#[test]
fn exact_receipt_roundtrip_and_rejects_invalid_identity_or_pose() {
    let value = receipt();
    assert_eq!(
        PveDeathReceiptV1::decode(&value.encode().unwrap()).unwrap(),
        value
    );
    for invalid in [
        PveDeathReceiptV1 {
            marker_object_id: 0,
            ..value.clone()
        },
        PveDeathReceiptV1 {
            victim_object_id: value.marker_object_id,
            ..value.clone()
        },
        PveDeathReceiptV1 {
            event_id: [0; 16],
            ..value.clone()
        },
        PveDeathReceiptV1 {
            world_epoch: 0,
            ..value.clone()
        },
        PveDeathReceiptV1 {
            killed: false,
            ..value.clone()
        },
    ] {
        assert!(invalid.encode().is_err());
        let unchecked = encode(
            PVE_DEATH_RECEIPT_KIND,
            1,
            &invalid,
            CodecLimits {
                max_payload_bytes: 1024,
            },
        )
        .unwrap();
        assert!(PveDeathReceiptV1::decode(&unchecked).is_err());
    }
    let mut invalid = value.clone();
    invalid.position.position_x = f32::NAN;
    assert!(invalid.encode().is_err());
    let mut invalid = value.clone();
    invalid.position.rotation_w = 0.;
    assert!(invalid.encode().is_err());
    let mut broken = value.encode().unwrap();
    *broken.last_mut().unwrap() ^= 0xff;
    assert!(PveDeathReceiptV1::decode(&broken).is_err());
}
