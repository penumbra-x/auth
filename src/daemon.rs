use crate::{serve::Serve, BootArgs};
use anyhow::Result;
use daemonize::Daemonize;
use std::{
    fs::{File, Permissions},
    os::unix::fs::PermissionsExt,
    path::Path,
};

const PID_PATH: &str = "/var/run/auth.pid";
const DEFAULT_STDOUT_PATH: &str = "/var/run/auth.out";
const DEFAULT_STDERR_PATH: &str = "/var/run/auth.err";

/// Get the pid of the daemon
fn get_pid() -> Option<String> {
    if let Ok(data) = std::fs::read(PID_PATH) {
        let binding = String::from_utf8(data).expect("pid file is not utf8");
        return Some(binding.trim().to_string());
    }
    None
}

/// Check if the current user is root
fn check_root() {
    if !nix::unistd::Uid::effective().is_root() {
        println!("You must run this executable with root permissions");
        std::process::exit(-1)
    }
}

/// Start the daemon
pub fn start(args: BootArgs) -> Result<()> {
    if let Some(pid) = get_pid() {
        println!("auth is already running with pid: {}", pid);
        return Ok(());
    }

    check_root();

    let pid_file = File::create(PID_PATH)?;
    pid_file.set_permissions(Permissions::from_mode(0o755))?;

    let stdout = File::create(DEFAULT_STDOUT_PATH)?;
    stdout.set_permissions(Permissions::from_mode(0o755))?;

    let stderr = File::create(DEFAULT_STDERR_PATH)?;
    stdout.set_permissions(Permissions::from_mode(0o755))?;

    let mut daemonize = Daemonize::new()
        .pid_file(PID_PATH)
        .chown_pid_file(true)
        .umask(0o777)
        .stdout(stdout)
        .stderr(stderr)
        .privileged_action(|| "Executed before drop privileges");

    if let Ok(user) = std::env::var("SUDO_USER") {
        if let Ok(Some(real_user)) = nix::unistd::User::from_name(&user) {
            daemonize = daemonize
                .user(real_user.name.as_str())
                .group(real_user.gid.as_raw());
        }
    }

    if let Some(err) = daemonize.start().err() {
        eprintln!("Error: {err}");
        std::process::exit(-1)
    }

    Serve(args).run()
}

/// Stop the daemon
pub fn stop() -> Result<()> {
    use nix::sys::signal;
    use nix::unistd::Pid;

    check_root();

    if let Some(pid) = get_pid() {
        let pid = pid.parse::<i32>()?;
        for _ in 0..360 {
            if signal::kill(Pid::from_raw(pid), signal::SIGINT).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1))
        }
        let _ = std::fs::remove_file(PID_PATH);
    }

    Ok(())
}

/// Restart the daemon
pub fn restart(args: BootArgs) -> Result<()> {
    stop()?;
    start(args)
}

/// Show the status of the daemon
pub fn status() {
    match get_pid() {
        Some(pid) => println!("auth is running with pid: {}", pid),
        None => println!("auth is not running"),
    }
}

/// Show the log of the daemon
pub fn log() -> Result<()> {
    fn read_and_print_file(file_path: &Path, placeholder: &str) -> Result<()> {
        if !file_path.exists() {
            return Ok(());
        }

        // Check if the file is empty before opening it
        let metadata = std::fs::metadata(file_path)?;
        if metadata.len() == 0 {
            return Ok(());
        }

        let file = File::open(file_path)?;
        let reader = std::io::BufReader::new(file);
        let mut start = true;

        use std::io::BufRead;

        for line in reader.lines() {
            if let Ok(content) = line {
                if start {
                    start = false;
                    println!("{placeholder}");
                }
                println!("{}", content);
            } else if let Err(err) = line {
                eprintln!("Error reading line: {}", err);
            }
        }

        Ok(())
    }

    let stdout_path = Path::new(DEFAULT_STDOUT_PATH);
    read_and_print_file(stdout_path, "STDOUT>")?;

    let stderr_path = Path::new(DEFAULT_STDERR_PATH);
    read_and_print_file(stderr_path, "STDERR>")?;

    Ok(())
}
