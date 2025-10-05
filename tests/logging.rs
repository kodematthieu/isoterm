use std::process::Command;

fn run_isoterm_with_args(args: &[&str]) -> (String, String) {
    let bin_path = env!("CARGO_BIN_EXE_isoterm");
    let output = Command::new(bin_path)
        .args(args)
        // Set a dummy dest_dir to avoid cluttering the user's home directory
        .args(&["/tmp/isoterm-test-env"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).expect("Failed to read stdout");
    let stderr = String::from_utf8(output.stderr).expect("Failed to read stderr");

    (stdout, stderr)
}

#[test]
fn test_logging_no_verbose() {
    let (_stdout, stderr) = run_isoterm_with_args(&[]);
    assert!(!stderr.contains("info log from run_inner"));
    assert!(!stderr.contains("debug log from run_inner"));
    assert!(!stderr.contains("trace log from run_inner"));
}

#[test]
fn test_logging_v() {
    let (_stdout, stderr) = run_isoterm_with_args(&["-v"]);
    assert!(stderr.contains("info log from run_inner"));
    assert!(!stderr.contains("debug log from run_inner"));
    assert!(!stderr.contains("trace log from run_inner"));
}

#[test]
fn test_logging_vv() {
    let (_stdout, stderr) = run_isoterm_with_args(&["-vv"]);
    assert!(stderr.contains("info log from run_inner"));
    assert!(stderr.contains("debug log from run_inner"));
    assert!(!stderr.contains("trace log from run_inner"));
}

#[test]
fn test_logging_vvv() {
    let (_stdout, stderr) = run_isoterm_with_args(&["-vvv"]);
    assert!(stderr.contains("info log from run_inner"));
    assert!(stderr.contains("debug log from run_inner"));
    assert!(stderr.contains("trace log from run_inner"));
}

#[test]
fn test_logging_vvvv() {
    let (_stdout, stderr) = run_isoterm_with_args(&["-vvvv"]);
    assert!(stderr.contains("info log from run_inner"));
    assert!(stderr.contains("debug log from run_inner"));
    assert!(stderr.contains("trace log from run_inner"));
}
