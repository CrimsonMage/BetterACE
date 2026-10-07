use bace_dat_service::{DddLimits, prepare_record};
use bace_wire::DddDatabase;
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// Independent system-zlib decoder, separate from flate2's Rust miniz backend.
/// Run explicitly when python3 is available; absence is not a compatibility pass.
#[test]
#[ignore = "requires python3 with its system zlib module"]
fn prepared_zlib_is_decoded_by_independent_system_zlib() {
    for raw in [
        vec![42; 8192],
        (0..16384).map(|i| (i % 251) as u8).collect(),
    ] {
        let prepared =
            prepare_record(DddDatabase::Portal, 1, 1, 1, &raw, DddLimits::default()).unwrap();
        assert!(prepared.metadata().compressed);
        let mut child = Command::new("python3")
            .args(["-c", "import sys,struct,zlib; p=sys.stdin.buffer.read(); n=struct.unpack('<I',p[:4])[0]; d=zlib.decompress(p[4:]); assert len(d)==n; sys.stdout.buffer.write(d)"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("python3 prerequisite");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(prepared.payload())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, raw);
    }
}
