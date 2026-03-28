use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, sleep};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};

use nimesvc::parser::parse_project;

fn status(label: &str, detail: impl AsRef<str>) {
    println!("Dev: {:<10} {}", label, detail.as_ref());
}

fn is_access_log_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('[') && trimmed.contains(" -> ")
}

fn stream_output<R: std::io::Read + Send + 'static>(reader: R, prefix: &'static str) {
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if prefix == "[err]" && is_access_log_line(&line) {
                        println!("{}", line);
                    } else {
                        println!("{} {}", prefix, line);
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn print_captured_output(output: &Output, stdout_prefix: &str, stderr_prefix: &str) {
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if !line.trim().is_empty() {
            println!("{} {}", stdout_prefix, line);
        }
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if !line.trim().is_empty() {
            if stderr_prefix == "[err]" && is_access_log_line(line) {
                println!("{}", line);
            } else {
                eprintln!("{} {}", stderr_prefix, line);
            }
        }
    }
}

fn collect_watch_paths(input: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![input.to_path_buf()];
    let src = fs::read_to_string(input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)
        .with_context(|| format!("Failed to parse project '{}'", input.display()))?;
    let input_dir = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    for env_name in [".env", ".env.local"] {
        paths.push(input_dir.join(env_name));
    }

    for module in &project.common.modules {
        if let Some(path) = &module.path {
            paths.push(resolve_watch_path(&input_dir, path));
        }
    }
    for service in &project.services {
        for module in &service.common.modules {
            if let Some(path) = &module.path {
                paths.push(resolve_watch_path(&input_dir, path));
            }
        }
        for rpc in &service.rpc.methods {
            for module in &rpc.modules {
                if let Some(path) = &module.path {
                    paths.push(resolve_watch_path(&input_dir, path));
                }
            }
        }
        for socket in &service.sockets.sockets {
            for module in &socket.modules {
                if let Some(path) = &module.path {
                    paths.push(resolve_watch_path(&input_dir, path));
                }
            }
        }
    }

    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn resolve_watch_path(input_dir: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        input_dir.join(path)
    }
}

fn snapshot_paths(paths: &[PathBuf]) -> HashMap<PathBuf, Option<SystemTime>> {
    let mut snapshot = HashMap::new();
    for path in paths {
        let modified = fs::metadata(path).and_then(|m| m.modified()).ok();
        snapshot.insert(path.clone(), modified);
    }
    snapshot
}

fn has_changes(
    previous: &HashMap<PathBuf, Option<SystemTime>>,
    current: &HashMap<PathBuf, Option<SystemTime>>,
) -> bool {
    if previous.len() != current.len() {
        return true;
    }
    for (path, current_time) in current {
        if previous.get(path) != Some(current_time) {
            return true;
        }
    }
    false
}

fn changed_paths(
    previous: &HashMap<PathBuf, Option<SystemTime>>,
    current: &HashMap<PathBuf, Option<SystemTime>>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (path, current_time) in current {
        if previous.get(path) != Some(current_time) {
            paths.push(path.clone());
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            paths.push(path.clone());
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn watch_path_changes(previous: &[PathBuf], current: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for path in current {
        if !previous.contains(path) {
            added.push(path.clone());
        }
    }
    for path in previous {
        if !current.contains(path) {
            removed.push(path.clone());
        }
    }
    (added, removed)
}

fn spawn_run(
    exe: &Path,
    input: &Path,
    kind: &Option<String>,
    lang: &Option<String>,
    no_log: bool,
    out: &Option<PathBuf>,
) -> Result<Child> {
    let mut cmd = Command::new(exe);
    cmd.arg("run").arg(input);
    if let Some(kind) = kind {
        cmd.arg(kind);
    }
    if let Some(lang) = lang {
        cmd.arg("--lang").arg(lang);
    }
    if no_log {
        cmd.arg("--no-log");
    }
    if let Some(out) = out {
        cmd.arg("--out").arg(out);
    }
    cmd.env("NIMESVC_SKIP_GENERATE", "1");
    cmd.env("NIMESVC_DISABLE_CTRLC", "1");
    cmd.env("NIMESVC_DEV_RUNTIME", "1");
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to start `nimesvc run` for '{}'", input.display()))?;
    if let Some(stdout) = child.stdout.take() {
        stream_output(stdout, "[run]");
    }
    if let Some(stderr) = child.stderr.take() {
        stream_output(stderr, "[err]");
    }
    Ok(child)
}

fn run_generate(
    exe: &Path,
    input: &Path,
    kind: &Option<String>,
    lang: &Option<String>,
    out: &Option<PathBuf>,
) -> Result<()> {
    let mut cmd = Command::new(exe);
    cmd.arg("generate").arg(input);
    if let Some(kind) = kind {
        cmd.arg(kind);
    }
    if let Some(lang) = lang {
        cmd.arg("--lang").arg(lang);
    }
    if let Some(out) = out {
        cmd.arg("--out").arg(out);
    }
    let output = cmd
        .output()
        .with_context(|| format!("Failed to run `nimesvc generate` for '{}'", input.display()))?;
    print_captured_output(&output, "[gen]", "[gen-err]");
    if !output.status.success() {
        anyhow::bail!(
            "`nimesvc generate {}` exited with status {}",
            input.display(),
            output.status
        );
    }
    Ok(())
}

fn run_stop(exe: &Path, input: &Path, out: &Option<PathBuf>) -> Result<()> {
    let mut cmd = Command::new(exe);
    cmd.arg("stop").arg(input);
    if let Some(out) = out {
        cmd.arg("--out").arg(out);
    }
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    let status = cmd
        .status()
        .with_context(|| format!("Failed to run `nimesvc stop` for '{}'", input.display()))?;
    if !status.success() {
        anyhow::bail!(
            "`nimesvc stop {}` exited with status {}",
            input.display(),
            status
        );
    }
    Ok(())
}

fn wait_for_child_exit(child: &mut Child, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if child.try_wait().ok().flatten().is_some() {
            return true;
        }
        sleep(Duration::from_millis(100));
    }
    false
}

fn restart_run(
    exe: &Path,
    input: &Path,
    kind: &Option<String>,
    lang: &Option<String>,
    no_log: bool,
    out: &Option<PathBuf>,
    running: &mut Option<Child>,
) -> Result<()> {
    if let Some(mut child) = running.take() {
        status("stopping", input.display().to_string());
        let _ = run_stop(exe, input, out);
        if !wait_for_child_exit(&mut child, Duration::from_secs(10)) {
            status("killing", "previous process did not stop in time");
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    status("generating", input.display().to_string());
    run_generate(exe, input, kind, lang, out)?;
    status("generated", "output updated");
    status("starting", input.display().to_string());
    *running = Some(spawn_run(exe, input, kind, lang, no_log, out)?);
    status("running", "live output attached");
    Ok(())
}

pub(super) fn dev_cmd(
    kind: Option<String>,
    lang: Option<String>,
    no_log: bool,
    input: PathBuf,
    out: Option<PathBuf>,
    debounce_ms: u64,
) -> Result<()> {
    let exe = std::env::current_exe().with_context(|| "Failed to resolve current executable")?;
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = stop_requested.clone();
    ctrlc::set_handler(move || {
        stop_flag.store(true, Ordering::SeqCst);
    })
    .with_context(|| "Failed to install Ctrl+C handler for `nimesvc dev`")?;

    let debounce = Duration::from_millis(debounce_ms.max(100));
    let mut running = None;
    status("scanning", input.display().to_string());
    let mut watch_paths = match collect_watch_paths(&input) {
        Ok(paths) => paths,
        Err(err) => {
            status("failed", "initial project scan failed");
            eprintln!("Error: {}", err);
            vec![input.clone()]
        }
    };
    let mut snapshot = snapshot_paths(&watch_paths);
    status("parsed", "project is valid");

    status("watching", input.display().to_string());
    for path in &watch_paths {
        println!("Dev: watch      {}", path.display());
    }

    match restart_run(&exe, &input, &kind, &lang, no_log, &out, &mut running) {
        Ok(()) => status("ready", "initial run completed"),
        Err(err) => {
            status("failed", "initial run failed");
            eprintln!("Error: {}", err);
            status("waiting", "waiting for the next file change");
        }
    }

    while !stop_requested.load(Ordering::SeqCst) {
        if let Some(child) = running.as_mut() {
            if let Some(exit_status) = child.try_wait().with_context(|| {
                format!("Failed to poll `nimesvc run` for '{}'", input.display())
            })? {
                status(
                    "exited",
                    format!("process exited with status {}", exit_status),
                );
                running = None;
                status("waiting", "waiting for the next file change");
            }
        }

        sleep(debounce);

        let next_watch_paths = match collect_watch_paths(&input) {
            Ok(paths) => paths,
            Err(err) => {
                status("failed", "project scan failed");
                eprintln!("Error: {}", err);
                status("waiting", "waiting for the next file change");
                watch_paths.clone()
            }
        };
        let current = snapshot_paths(&next_watch_paths);
        if has_changes(&snapshot, &current) {
            let changed = changed_paths(&snapshot, &current);
            status("restart", "change detected");
            for path in &changed {
                println!("Dev: changed    {}", path.display());
            }
            let (added, removed) = watch_path_changes(&watch_paths, &next_watch_paths);
            for path in &added {
                println!("Dev: watch +    {}", path.display());
            }
            for path in &removed {
                println!("Dev: watch -    {}", path.display());
            }
            watch_paths = next_watch_paths;
            snapshot = current;
            status("parsed", "project is valid");
            match restart_run(&exe, &input, &kind, &lang, no_log, &out, &mut running) {
                Ok(()) => status("ready", "restart completed"),
                Err(err) => {
                    running = None;
                    status("failed", "restart failed");
                    eprintln!("Error: {}", err);
                    status("waiting", "waiting for the next file change");
                }
            }
        } else {
            watch_paths = next_watch_paths;
            snapshot = current;
        }
    }

    status("stopping", input.display().to_string());
    if let Some(mut child) = running.take() {
        let _ = run_stop(&exe, &input, &out);
        if !wait_for_child_exit(&mut child, Duration::from_secs(10)) {
            status("killing", "process did not stop in time");
            let _ = child.kill();
        }
        let _ = child.wait();
    }
    status("stopped", input.display().to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{changed_paths, collect_watch_paths, watch_path_changes};

    #[test]
    fn collect_watch_paths_includes_input_and_declared_modules() {
        let mut dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("nimesvc-dev-watch-test-{stamp}"));
        fs::create_dir_all(dir.join("modules")).unwrap();

        let input = dir.join("api.ns");
        fs::write(
            &input,
            r#"use shared "./modules/shared.rs"

service Api:
    use api "./modules/api.rs"
    GET "/health":
        response 200
        healthcheck
"#,
        )
        .unwrap();
        fs::write(dir.join("modules/shared.rs"), "pub fn shared() {}\n").unwrap();
        fs::write(dir.join("modules/api.rs"), "pub fn api() {}\n").unwrap();
        fs::write(dir.join(".env"), "PORT=3000\n").unwrap();

        let paths = collect_watch_paths(&input).unwrap();

        assert!(paths.contains(&input));
        assert!(paths.contains(&dir.join("modules/shared.rs")));
        assert!(paths.contains(&dir.join("modules/api.rs")));
        assert!(paths.contains(&dir.join(".env")));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn changed_paths_reports_modified_and_removed_files() {
        let previous = HashMap::from([
            (PathBuf::from("a.ns"), None),
            (
                PathBuf::from("b.rs"),
                Some(std::time::UNIX_EPOCH + Duration::from_secs(1)),
            ),
        ]);
        let current = HashMap::from([
            (
                PathBuf::from("b.rs"),
                Some(std::time::UNIX_EPOCH + Duration::from_secs(2)),
            ),
            (PathBuf::from("c.rs"), None),
        ]);

        let changed = changed_paths(&previous, &current);

        assert_eq!(
            changed,
            vec![
                PathBuf::from("a.ns"),
                PathBuf::from("b.rs"),
                PathBuf::from("c.rs")
            ]
        );
    }

    #[test]
    fn watch_path_changes_reports_added_and_removed_paths() {
        let previous = vec![PathBuf::from("a.ns"), PathBuf::from("old.rs")];
        let current = vec![PathBuf::from("a.ns"), PathBuf::from("new.rs")];

        let (added, removed) = watch_path_changes(&previous, &current);

        assert_eq!(added, vec![PathBuf::from("new.rs")]);
        assert_eq!(removed, vec![PathBuf::from("old.rs")]);
    }
}
