use std::time::Instant;

use crate::fixtures::*;
use crate::harness::*;

fn copy_cmd(uri: &str, path: &str) -> String {
    format!("nix copy --no-check-sigs --from '{uri}' --to /root/bench '{path}'")
}

fn blob(tag: &str) -> String {
    succeed(
        MACHINE,
        &format!("nix build --impure -f /etc/blob.nix {tag} --no-link --print-out-paths"),
    )
    .trim()
    .to_string()
}

fn bench(label: &str, uri: &str, path: &str) -> f64 {
    succeed(MACHINE, "rm -rf /root/bench && mkdir -p /root/bench");
    let t0 = Instant::now();
    succeed(MACHINE, &copy_cmd(uri, path));
    let dt = t0.elapsed().as_secs_f64();
    println!("[bench] {label:16} {dt:6.2}s  {:6.1} MiB/s", 256.0 / dt);
    dt
}

#[test]
fn throughput_grpc_vs_unix_socket_daemon() {
    for tag in ["text", "rand"] {
        let path = blob(tag);
        bench(&format!("{tag}/warmup"), "daemon", &path);
        let unix = bench(&format!("{tag}/unix"), "daemon", &path);
        let grpc = bench(&format!("{tag}/grpc"), STORE, &path);
        println!("[bench] {tag}: grpc={:.2}x unix", grpc / unix);
    }
}

#[test]
fn perf_profile_of_grpc_copy() {
    let path = blob("rand");
    for (label, uri) in [("unix", "daemon"), ("grpc", STORE)] {
        succeed(MACHINE, "rm -rf /root/bench && mkdir -p /root/bench");
        let out = succeed(
            MACHINE,
            &format!(
                "perf stat -a \
                 -e task-clock,context-switches,cycles,instructions,cache-misses,syscalls:sys_enter_read,syscalls:sys_enter_write \
                 -- {} 2>&1",
                copy_cmd(uri, &path)
            ),
        );
        println!("[perf-stat {label}]\n{out}");
    }
    succeed(MACHINE, "rm -rf /root/bench && mkdir -p /root/bench");
    succeed(
        MACHINE,
        &format!(
            "perf record -a -g -F 999 -o /root/perf.data -- {}",
            copy_cmd(STORE, &path)
        ),
    );
    let report = succeed(
        MACHINE,
        "perf report -i /root/perf.data --stdio --no-children --percent-limit 0.5 > /root/perf.txt 2>/dev/null && head -80 /root/perf.txt",
    );
    println!("[perf-report grpc top functions]\n{report}");
    let dso = succeed(
        MACHINE,
        "perf report -i /root/perf.data --stdio --sort=dso --percent-limit 0.5 > /root/perf.txt 2>/dev/null && head -40 /root/perf.txt",
    );
    println!("[perf-report grpc per-DSO]\n{dso}");
}
