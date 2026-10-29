use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub fn run_output(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
    max_stream_bytes: usize,
    label: &str,
) -> Result<Output, String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("{label}: failed to start: {error}"))?;

    if let Some(input) = input {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("{label}: child stdin unavailable"))?;
        stdin
            .write_all(input)
            .map_err(|error| format!("{label}: write child stdin: {error}"))?;
        drop(stdin);
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label}: child stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{label}: child stderr unavailable"))?;
    let stdout = thread::spawn(move || read_bounded(stdout, max_stream_bytes));
    let stderr = thread::spawn(move || read_bounded(stderr, max_stream_bytes));

    let status = wait_bounded(&mut child, timeout, label)?;
    let stdout = join_reader(stdout, label, "stdout")?;
    let stderr = join_reader(stderr, label, "stderr")?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn wait_bounded(
    child: &mut std::process::Child,
    timeout: Duration,
    label: &str,
) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("{label}: poll child: {error}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child
                .wait()
                .map_err(|error| format!("{label}: reap timed-out child: {error}"))?;
            return Err(format!(
                "{label}: timed out after {} seconds",
                timeout.as_secs_f64()
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(kept)
}

fn join_reader(
    thread: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    label: &str,
    stream: &str,
) -> Result<Vec<u8>, String> {
    thread
        .join()
        .map_err(|_| format!("{label}: {stream} reader panicked"))?
        .map_err(|error| format!("{label}: read {stream}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::run_output;

    #[test]
    fn kills_and_reaps_a_child_after_the_deadline() {
        let start = Instant::now();
        let error = run_output(
            Command::new("/bin/sleep").arg("2"),
            None,
            Duration::from_millis(20),
            1024,
            "sleep test",
        )
        .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn captures_bounded_output() {
        let output = run_output(
            Command::new("/usr/bin/printf").arg("abcdefghij"),
            None,
            Duration::from_secs(1),
            4,
            "printf test",
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"abcd");
    }
}
