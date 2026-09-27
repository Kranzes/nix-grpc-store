use std::env as std_env;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Once;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct Node(pub &'static str);

pub const MACHINE: Node = Node("machine");

pub fn env(key: &str) -> String {
    std_env::var(key).unwrap_or_else(|_| panic!("missing env var {key}"))
}

fn ssh_args(node: Node) -> Vec<String> {
    let mut args = vec!["-F".into(), env("NGS_SSH_CONFIG")];
    for opt in [
        "User=root",
        "StrictHostKeyChecking=no",
        "UserKnownHostsFile=/dev/null",
        "ConnectTimeout=30",
        "ServerAliveInterval=30",
        "ServerAliveCountMax=10",
        "LogLevel=ERROR",
    ] {
        args.push("-o".into());
        args.push(opt.into());
    }
    args.push(format!(
        "vsock-mux/{}",
        env(&format!("NGS_{}_SOCK", node.0.to_uppercase()))
    ));
    args
}

pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl Out {
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

fn drain<R: Read + Send + 'static>(mut r: R) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut b = Vec::new();
        let _ = r.read_to_end(&mut b);
        String::from_utf8_lossy(&b).into_owned()
    })
}

fn run_once(node: Node, cmd: &str, timeout: Duration) -> Out {
    let mut child = Command::new(env("NGS_SSH"))
        .args(ssh_args(node))
        .arg(format!("set -euo pipefail\n{cmd}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ssh");
    // Readers keep a chatty command from filling the pipe and blocking.
    let out = drain(child.stdout.take().unwrap());
    let err = drain(child.stderr.take().unwrap());

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(s) = child.try_wait().expect("try_wait") {
            break s;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            timed_out = true;
            break child.wait().expect("wait after kill");
        }
        thread::sleep(Duration::from_millis(100));
    };
    Out {
        code: status.code().unwrap_or(-1),
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
        timed_out,
    }
}

fn connect_failed(stderr: &str) -> bool {
    [
        "banner exchange",
        "Connection refused",
        "Connection timed out",
        "Connection closed by remote host",
        "kex_exchange_identification",
    ]
    .iter()
    .any(|m| stderr.contains(m))
}

static READY: Once = Once::new();

fn ensure_ready(node: Node) {
    READY.call_once(|| {
        let start = Instant::now();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(30));
                println!("[e2e] heartbeat t={}s", start.elapsed().as_secs());
            }
        });
        let deadline = Instant::now() + Duration::from_secs(120);
        while run_once(node, "true", Duration::from_secs(20)).code != 0 {
            assert!(Instant::now() < deadline, "ssh backdoor never became ready");
            thread::sleep(Duration::from_millis(500));
        }
    });
}

pub fn run_t(node: Node, cmd: &str, secs: u64) -> Out {
    ensure_ready(node);
    let mut o = run_once(node, cmd, Duration::from_secs(secs));
    // Failing before the command starts, so a retry cannot run it twice.
    for _ in 0..4 {
        if o.code != 255 || o.timed_out || !connect_failed(&o.stderr) {
            break;
        }
        thread::sleep(Duration::from_secs(2));
        o = run_once(node, cmd, Duration::from_secs(secs));
    }
    assert!(
        !o.timed_out,
        "[{}] timed out after {secs}s: {cmd}\n{}",
        node.0,
        o.combined()
    );
    assert!(
        o.code != 255,
        "[{}] ssh transport error: {cmd}\n{}",
        node.0,
        o.combined()
    );
    o
}

pub fn succeed(node: Node, cmd: &str) -> String {
    let o = run_t(node, cmd, 300);
    assert_eq!(
        o.code,
        0,
        "[{}] failed ({}): {cmd}\n{}",
        node.0,
        o.code,
        o.combined()
    );
    o.stdout
}

pub fn fail(node: Node, cmd: &str) -> String {
    let o = run_t(node, cmd, 300);
    assert_ne!(
        o.code,
        0,
        "[{}] expected failure: {cmd}\n{}",
        node.0,
        o.combined()
    );
    o.combined()
}

pub fn wait_until_succeeds(node: Node, cmd: &str, secs: u64) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while run_t(node, cmd, 60).code != 0 {
        assert!(
            Instant::now() < deadline,
            "[{}] not true after {secs}s: {cmd}",
            node.0
        );
        thread::sleep(Duration::from_millis(500));
    }
}

pub fn wait_for_unit(node: Node, unit: &str) {
    wait_until_succeeds(node, &format!("systemctl is-active {unit}"), 120);
}

pub fn wait_for_open_port(node: Node, port: u16) {
    wait_until_succeeds(node, &format!(": < /dev/tcp/127.0.0.1/{port}"), 60);
}

pub fn unit_state(node: Node, unit: &str) -> String {
    succeed(node, &format!("systemctl show -P ActiveState {unit}"))
        .trim()
        .to_string()
}

pub fn journal_count(node: Node, unit: &str, pattern: &str) -> u64 {
    succeed(
        node,
        &format!("journalctl -u {unit} --no-pager | grep -c '{pattern}' || true"),
    )
    .trim()
    .parse()
    .expect("grep -c prints a number")
}

pub fn assert_journal(node: Node, unit: &str, pattern: &str) {
    assert!(
        journal_count(node, unit, pattern) > 0,
        "[{}] journal of {unit} has no line matching: {pattern}",
        node.0
    );
}

pub fn assert_no_journal(node: Node, unit: &str, pattern: &str) {
    assert_eq!(
        journal_count(node, unit, pattern),
        0,
        "[{}] journal of {unit} has a line matching: {pattern}",
        node.0
    );
}

pub fn wait_journal_above(node: Node, unit: &str, pattern: &str, before: u64, secs: u64) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while journal_count(node, unit, pattern) <= before {
        assert!(
            Instant::now() < deadline,
            "[{}] journal of {unit} never got a new line matching: {pattern}",
            node.0
        );
        thread::sleep(Duration::from_millis(500));
    }
}

pub fn write_file(node: Node, path: &str, content: &str) {
    succeed(node, &format!("cat > {path} <<'NGSEOF'\n{content}\nNGSEOF"));
}
