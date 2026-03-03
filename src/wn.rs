use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

/// Build a Command with the --json flag.
fn command(args: &[&str]) -> Command {
    let mut cmd = Command::new("wn");
    cmd.arg("--json");
    cmd.args(args);
    cmd
}

/// Parse a CLI JSON response, extracting the result or returning the error.
pub fn parse_response(json: &str) -> Result<Value> {
    let val: Value =
        serde_json::from_str(json).with_context(|| format!("Invalid JSON from CLI: {json}"))?;

    if let Some(err) = val.get("error") {
        bail!("{}", err.as_str().unwrap_or("Unknown CLI error"));
    }

    Ok(val.get("result").cloned().unwrap_or(Value::Null))
}

/// Spawn `wn --json <args>` and return parsed JSON result.
pub async fn exec(args: &[&str]) -> Result<Value> {
    let output = command(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to run wn — is it installed and in PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Try parsing stdout for structured error first
        if !stdout.trim().is_empty() {
            if let Ok(val) = serde_json::from_str::<Value>(stdout.trim()) {
                if let Some(err) = val.get("error") {
                    bail!("{}", err.as_str().unwrap_or("Unknown error"));
                }
            }
        }
        bail!("wn failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_response(stdout.trim())
}

/// Spawn `wn --json <args>` with data written to stdin, then return result.
pub async fn exec_with_stdin(args: &[&str], input: &str) -> Result<Value> {
    let mut child = command(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn wn — is it installed and in PATH?")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        drop(stdin);
    }

    let output = child.wait_with_output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            if let Ok(val) = serde_json::from_str::<Value>(stdout.trim()) {
                if let Some(err) = val.get("error") {
                    bail!("{}", err.as_str().unwrap_or("Unknown error"));
                }
            }
        }
        bail!("wn failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_response(stdout.trim())
}

/// Result of attempting to parse JSON values from a byte buffer.
enum ParseOutcome {
    /// Values were extracted; may have more to parse.
    Values(Vec<Value>),
    /// Incomplete data — need more bytes.
    Incomplete,
    /// Stream signaled end.
    StreamEnd(Vec<Value>),
}

/// Parse as many complete JSON values as possible from `buf`, draining consumed bytes.
///
/// Only yields values that have a `"result"` key (the CLI streaming protocol).
/// Recognises `"stream_end": true` as a termination signal.
fn parse_stream_buf(buf: &mut Vec<u8>) -> ParseOutcome {
    let mut values = Vec::new();

    loop {
        // Skip leading whitespace
        match buf.iter().position(|b| !b.is_ascii_whitespace()) {
            Some(s) if s > 0 => {
                buf.drain(..s);
            }
            Some(_) => {}
            None => {
                buf.clear();
                break;
            }
        }

        let mut de = serde_json::Deserializer::from_slice(buf).into_iter::<Value>();
        let val = match de.next() {
            Some(Ok(v)) => v,
            Some(Err(e)) if e.is_eof() => return ParseOutcome::Incomplete,
            Some(Err(_)) => {
                // Malformed byte — discard and retry
                buf.remove(0);
                continue;
            }
            None => break,
        };
        let consumed = de.byte_offset();
        buf.drain(..consumed);

        let is_end = val
            .get("stream_end")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if val.get("result").is_some() {
            values.push(val);
        }

        if is_end {
            return ParseOutcome::StreamEnd(values);
        }
    }

    ParseOutcome::Values(values)
}

/// Spawn `wn --json <args>` as a long-lived streaming process.
/// Returns the child process and a receiver of parsed JSON values.
///
/// The CLI outputs pretty-printed (multi-line) JSON. We use
/// `serde_json::StreamDeserializer` to correctly parse successive
/// JSON values from the byte stream — this handles braces inside
/// strings, escaped characters, and all edge cases.
pub async fn stream(args: &[&str]) -> Result<(Child, mpsc::UnboundedReceiver<Value>)> {
    let mut child = command(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn wn for streaming")?;

    let mut stdout = child.stdout.take().context("No stdout on child process")?;

    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 8192];

        loop {
            match tokio::io::AsyncReadExt::read(&mut stdout, &mut tmp).await {
                Ok(0) => break,
                Err(_) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }

            match parse_stream_buf(&mut buf) {
                ParseOutcome::Values(vals) => {
                    for val in vals {
                        if tx.send(val).is_err() {
                            return;
                        }
                    }
                }
                ParseOutcome::StreamEnd(vals) => {
                    for val in vals {
                        if tx.send(val).is_err() {
                            return;
                        }
                    }
                    return;
                }
                ParseOutcome::Incomplete => {}
            }
        }

        // Drain remaining buffered data
        if let ParseOutcome::Values(vals) | ParseOutcome::StreamEnd(vals) =
            parse_stream_buf(&mut buf)
        {
            for val in vals {
                if tx.send(val).is_err() {
                    return;
                }
            }
        }
    });

    Ok((child, rx))
}

/// Find the `wnd` binary. Checks next to `wn` on PATH first, then PATH itself.
pub fn find_wnd() -> Option<std::path::PathBuf> {
    // Try to find wn on PATH and look for wnd next to it
    if let Ok(wn_path) = which::which("wn") {
        let wnd_path = wn_path.with_file_name("wnd");
        if wnd_path.exists() {
            return Some(wnd_path);
        }
    }
    // Fall back to wnd on PATH
    which::which("wnd").ok()
}

/// Check if the daemon is reachable by running `wn --json whoami`.
pub async fn is_daemon_running() -> bool {
    let result = Command::new("wn")
        .args(["--json", "whoami"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    match result {
        Ok(output) => {
            // If the command ran at all (even with an error response), the daemon is up.
            // A connection-refused or "daemon not running" error comes through stderr
            // and typically gives a non-zero exit code with no JSON output.
            let stdout = String::from_utf8_lossy(&output.stdout);
            // If we got valid JSON back, daemon is running
            serde_json::from_str::<serde_json::Value>(stdout.trim()).is_ok()
        }
        Err(_) => false, // wn binary not found
    }
}

/// Start the daemon as a background process. Returns the child handle.
pub async fn start_daemon() -> Result<Child> {
    let wnd_path =
        find_wnd().context("Could not find wnd binary. Ensure wn and wnd are on your PATH.")?;

    let child = Command::new(&wnd_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Failed to start daemon: {}", wnd_path.display()))?;

    // Wait for daemon to become ready (retry a few times)
    for i in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(if i < 5 {
            100
        } else {
            250
        }))
        .await;
        if is_daemon_running().await {
            return Ok(child);
        }
    }

    bail!("Daemon started but did not become ready within 5 seconds")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_result_object() {
        let json = r#"{"result": {"npub": "npub1abc"}}"#;
        let val = parse_response(json).unwrap();
        assert_eq!(val["npub"].as_str().unwrap(), "npub1abc");
    }

    #[test]
    fn parse_result_array() {
        let json = r#"{"result": [{"npub": "npub1a"}, {"npub": "npub1b"}]}"#;
        let val = parse_response(json).unwrap();
        assert_eq!(val.as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_error_returns_err() {
        let json = r#"{"error": "No accounts found"}"#;
        let err = parse_response(json).unwrap_err();
        assert!(err.to_string().contains("No accounts found"));
    }

    #[test]
    fn parse_empty_object_returns_null() {
        let json = "{}";
        let val = parse_response(json).unwrap();
        assert!(val.is_null());
    }

    #[test]
    fn parse_invalid_json_returns_err() {
        assert!(parse_response("not json").is_err());
    }

    #[test]
    fn parse_null_result() {
        let json = r#"{"result": null}"#;
        let val = parse_response(json).unwrap();
        assert!(val.is_null());
    }

    #[test]
    fn parse_error_takes_precedence() {
        let json = r#"{"result": "ok", "error": "bad"}"#;
        assert!(parse_response(json).is_err());
    }

    #[test]
    fn find_wnd_returns_path_or_none() {
        let result = find_wnd();
        if let Some(path) = result {
            assert!(path.exists());
            assert!(path.to_string_lossy().contains("wnd"));
        }
    }

    // ── Stream buffer parsing ────────────────────────────────────────

    fn extract_values(outcome: ParseOutcome) -> Vec<Value> {
        match outcome {
            ParseOutcome::Values(v) | ParseOutcome::StreamEnd(v) => v,
            ParseOutcome::Incomplete => vec![],
        }
    }

    fn is_incomplete(outcome: &ParseOutcome) -> bool {
        matches!(outcome, ParseOutcome::Incomplete)
    }

    fn is_stream_end(outcome: &ParseOutcome) -> bool {
        matches!(outcome, ParseOutcome::StreamEnd(_))
    }

    #[test]
    fn stream_parse_single_line_json() {
        let mut buf = br#"{"result": {"name": "alice"}}"#.to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["name"], "alice");
        assert!(buf.is_empty());
    }

    #[test]
    fn stream_parse_pretty_printed_json() {
        let mut buf = b"{\n  \"result\": {\n    \"name\": \"alice\"\n  }\n}".to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["name"], "alice");
    }

    #[test]
    fn stream_parse_multiple_objects() {
        let mut buf = br#"{"result": {"id": 1}} {"result": {"id": 2}}"#.to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 2);
        assert_eq!(vals[0]["result"]["id"], 1);
        assert_eq!(vals[1]["result"]["id"], 2);
    }

    #[test]
    fn stream_parse_incomplete_waits_for_more() {
        let mut buf = br#"{"result": {"na"#.to_vec();
        let outcome = parse_stream_buf(&mut buf);
        assert!(is_incomplete(&outcome));
        assert!(!buf.is_empty(), "incomplete data should remain in buffer");
    }

    #[test]
    fn stream_parse_completes_after_more_data() {
        let mut buf = br#"{"result": {"na"#.to_vec();
        assert!(is_incomplete(&parse_stream_buf(&mut buf)));

        // Append rest of the JSON
        buf.extend_from_slice(br#"me": "bob"}}"#);
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["name"], "bob");
    }

    #[test]
    fn stream_parse_braces_inside_strings() {
        let mut buf = br#"{"result": {"msg": "hello { world } foo"}}"#.to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["msg"], "hello { world } foo");
    }

    #[test]
    fn stream_parse_escaped_quotes_in_strings() {
        let mut buf = br#"{"result": {"msg": "she said \"hi\""}}"#.to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["msg"], r#"she said "hi""#);
    }

    #[test]
    fn stream_parse_skips_values_without_result_key() {
        let mut buf = br#"{"status": "ok"} {"result": {"id": 1}}"#.to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["id"], 1);
    }

    #[test]
    fn stream_parse_recognises_stream_end() {
        let mut buf = br#"{"result": {"id": 1}} {"stream_end": true}"#.to_vec();
        let outcome = parse_stream_buf(&mut buf);
        assert!(is_stream_end(&outcome));
        let vals = extract_values(outcome);
        assert_eq!(vals.len(), 1);
    }

    #[test]
    fn stream_parse_stream_end_with_result() {
        let mut buf = br#"{"result": {"final": true}, "stream_end": true}"#.to_vec();
        let outcome = parse_stream_buf(&mut buf);
        assert!(is_stream_end(&outcome));
        let vals = extract_values(outcome);
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["final"], true);
    }

    #[test]
    fn stream_parse_empty_buffer() {
        let mut buf = Vec::new();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert!(vals.is_empty());
    }

    #[test]
    fn stream_parse_whitespace_only() {
        let mut buf = b"   \n\t  ".to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert!(vals.is_empty());
        assert!(buf.is_empty());
    }

    #[test]
    fn stream_parse_whitespace_between_objects() {
        let mut buf = b"  {\"result\": {\"a\": 1}}  \n\n  {\"result\": {\"b\": 2}}  ".to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 2);
    }

    #[test]
    fn stream_parse_nested_objects() {
        let input = serde_json::json!({
            "result": {
                "item": {
                    "name": "test",
                    "members": [{"npub": "a"}, {"npub": "b"}]
                },
                "trigger": "new"
            }
        });
        let mut buf = serde_json::to_vec_pretty(&input).unwrap();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(
            vals[0]["result"]["item"]["members"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn stream_parse_recovers_from_garbage() {
        let mut buf = b"xxx{\"result\": {\"id\": 1}}".to_vec();
        let vals = extract_values(parse_stream_buf(&mut buf));
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["result"]["id"], 1);
    }
}
