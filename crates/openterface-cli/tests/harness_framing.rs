use std::path::Path;
use std::process::Command;

use openterface_core::event::{AbsPosition, ButtonMask, HidUsage, Modifiers};
use openterface_core::protocol::ch9329::{
    keyboard, keyboard_release, mouse_absolute, mouse_relative,
};

#[test]
fn kvm_debug_dryrun_frames_match_core_builders() {
    if Command::new("bash").arg("--version").output().is_err() {
        return;
    }

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/kvm-debug.sh");
    // Skip gracefully if the script is not present (e.g. a packaged crate that
    // does not ship the workspace `tools/` directory).
    if !script.exists() {
        return;
    }
    let output = Command::new("bash")
        .arg(&script)
        .arg("frames")
        .env("DRYRUN", "1")
        .env("LC_ALL", "C")
        .output()
        .expect("bash was available but failed to run kvm-debug.sh");

    assert!(
        output.status.success(),
        "kvm-debug.sh frames failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "DRYRUN frames must not write stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("frames output must be UTF-8");
    let actual: Vec<(String, Vec<u8>)> = stdout
        .lines()
        .map(parse_labeled_frame)
        .collect::<Result<_, _>>()
        .expect("all frames must parse as `LABEL: HEXBYTES`");

    let centre = AbsPosition { x: 2048, y: 2048 };
    let bottom_right = AbsPosition { x: 4095, y: 4095 };
    let expected = vec![
        (
            "MOVE_ABS",
            mouse_absolute(bottom_right, ButtonMask::NONE, 0),
        ),
        ("CLICK_PRESS", mouse_absolute(centre, ButtonMask::LEFT, 0)),
        ("CLICK_RELEASE", mouse_absolute(centre, ButtonMask::NONE, 0)),
        ("TYPE_A_PRESS", keyboard(Modifiers::NONE, &[HidUsage(0x04)])),
        ("TYPE_A_RELEASE", keyboard_release()),
        ("KEY_A_PRESS", keyboard(Modifiers::NONE, &[HidUsage(0x04)])),
        ("KEY_A_RELEASE", keyboard_release()),
        ("MOVE_REL", mouse_relative(5, -3, ButtonMask::NONE, 0)),
        ("ABS_CENTRE", mouse_absolute(centre, ButtonMask::NONE, 0)),
    ];

    assert_eq!(
        actual.len(),
        expected.len(),
        "unexpected frames output:\n{stdout}"
    );
    for ((actual_label, actual_bytes), (expected_label, expected_bytes)) in
        actual.iter().zip(expected.iter())
    {
        assert_eq!(actual_label.as_str(), *expected_label);
        assert_eq!(
            actual_bytes, expected_bytes,
            "frame mismatch for {expected_label}"
        );
    }
}

fn parse_labeled_frame(line: &str) -> Result<(String, Vec<u8>), String> {
    let (label, hex) = line
        .split_once(": ")
        .ok_or_else(|| format!("missing label separator in {line:?}"))?;
    if label.is_empty()
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!("invalid frame label {label:?}"));
    }

    let bytes = hex
        .split(' ')
        .map(|token| {
            if token.len() != 2 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("invalid hex byte {token:?} in {line:?}"));
            }
            if !token.bytes().all(|byte| !byte.is_ascii_lowercase()) {
                return Err(format!("hex byte is not uppercase {token:?} in {line:?}"));
            }
            u8::from_str_radix(token, 16)
                .map_err(|err| format!("invalid hex byte {token:?}: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok((label.to_owned(), bytes))
}
