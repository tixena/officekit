use core::str;

use assert_cmd::Command;

#[test]
fn version_prints_crate_name_and_version() {
    let output = Command::cargo_bin("officekit")
        .unwrap()
        .arg("--version")
        .output()
        .unwrap();

    assert!(output.status.success(), "command failed: {output:?}");
    let stdout = str::from_utf8(&output.stdout).unwrap();
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(stdout.trim_end(), format!("officekit {version}"));
}
