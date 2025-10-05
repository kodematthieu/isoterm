use std::fs;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use tempfile::tempdir;

fn run_isoterm_with_args(args: &[&str]) -> (String, String) {
    let bin_path = env!("CARGO_BIN_EXE_isoterm");
    let dest_dir_temp = tempdir().expect("Failed to create temp dir");
    let dest_dir = dest_dir_temp.path();
    let bin_dir = dest_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("Failed to create test bin dir");

    // Pre-create dummy tool files to prevent the app from trying to download them.
    let dummy_tools = ["fish", "starship", "zoxide", "atuin", "rg", "hx"];
    for tool in &dummy_tools {
        let tool_path = bin_dir.join(tool);
        fs::write(&tool_path, "").expect("Failed to create dummy tool file");
        // On Unix, make the dummy file executable to mimic a real binary.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tool_path, fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions on dummy tool");
        }
    }

    // Special setup for fish: create a dummy `share` directory to prevent
    // the `provision_source_share` function from triggering a real download.
    let fish_runtime_dir = dest_dir.join("fish_runtime").join("share");
    fs::create_dir_all(&fish_runtime_dir).expect("Failed to create dummy fish share dir");

    let mut child = Command::new(bin_path)
        .args(args)
        .arg(dest_dir.to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to execute command");

    let mut stdout_pipe = child.stdout.take().unwrap();
    let mut stderr_pipe = child.stderr.take().unwrap();

    let stdout_thread = thread::spawn(move || {
        let mut buffer = Vec::new();
        stdout_pipe
            .read_to_end(&mut buffer)
            .expect("Failed to read from stdout");
        String::from_utf8(buffer).expect("Failed to parse stdout")
    });

    let stderr_thread = thread::spawn(move || {
        let mut buffer = Vec::new();
        stderr_pipe
            .read_to_end(&mut buffer)
            .expect("Failed to read from stderr");
        String::from_utf8(buffer).expect("Failed to parse stderr")
    });

    // First, join the reader threads to collect all the output. The `join` calls
    // will block until the child process exits and closes the pipes, at which
    // point `read_to_end` will return. This is the correct way to avoid a deadlock.
    let stdout_str = stdout_thread.join().unwrap();
    let stderr_str = stderr_thread.join().unwrap();

    // Now that I/O is complete, we can wait for the child process to get its
    // exit status. This should return immediately.
    child.wait().expect("Child process failed to exit");

    (stdout_str, stderr_str)
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
