use super::*;
#[test]
fn missing_verified_assets_return_exact_work_and_shutdown_keeps_pending_owner() {
    let directory = tempfile::tempdir().unwrap();
    let manifest = RegionAssetManifest {
        portal: directory.path().join("absent-portal.dat"),
        cell: directory.path().join("absent-cell.dat"),
        portal_sha256: "00".repeat(32),
        cell_sha256: "00".repeat(32),
    };
    let mut owner = StaffMapWorker::start(manifest).unwrap();
    let work = StaffMapWork {
        token: 7,
        request: MapTeleportRequest {
            cell: 0x0101_0001,
            origin: [24., 24., 9000.],
            rotation: [1., 0., 0., 0.],
        },
        expected_epoch: 9,
    };
    owner.submit(work).unwrap();
    assert_eq!(
        owner.submit(StaffMapWork { token: 8, ..work }),
        Err(StaffMapWork { token: 8, ..work })
    );
    let mut owner = *owner
        .shutdown()
        .expect_err("pending work must survive shutdown");
    let mut result = None;
    for _ in 0..200 {
        if let Some(done) = owner.poll().unwrap() {
            result = Some(done);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let result = result.expect("bounded cold worker result");
    assert_eq!(result.work, work);
    assert!(result.result.is_err());
    assert!(!owner.has_pending());
    assert!(owner.shutdown().is_ok());
}
