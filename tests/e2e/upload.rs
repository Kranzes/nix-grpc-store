use crate::fixtures::*;
use crate::harness::{MACHINE, journal_count};

const INPUT_MIB: usize = 1024;
const BUILD_TIMEOUT_S: u64 = 120;

struct Outcome {
    code: i32,
    started: u64,
    finished: u64,
    log: String,
}

const DAEMON: &str = "nix-grpc-daemon";

fn uploads(event: &str) -> u64 {
    journal_count(
        MACHINE,
        DAEMON,
        &format!("{event} method=AddMultipleToStore"),
    )
}

/// Builds `jobs` derivations that share one large input through Envoy.
fn build_sharing_one_input(name: &str, jobs: usize) -> Outcome {
    wait_for_unit("envoy.service");
    wait_for_open_port(50060);
    let dir = workdir(name);
    let big = format!("{dir}/big");
    succeed(&format!("head -c {INPUT_MIB}M /dev/urandom > {big}"));
    write_file(
        &format!("{dir}/jobs.nix"),
        &format!(
            r#"let big = builtins.path {{ path = {big}; name = "big"; }}; in
builtins.genList (i: derivation {{
  name = "job-${{toString i}}";
  system = builtins.currentSystem;
  builder = "/bin/sh";
  args = [ "-c" "echo ok > $out" ];
  inherit big;
  tag = toString i;
}}) {jobs}"#
        ),
    );
    let src = format!("local?root={dir}/src");
    let targets = succeed(&format!(
        "for i in $(seq 0 {}); do nix-instantiate --store '{src}' {dir}/jobs.nix -A $i; done",
        jobs - 1
    ))
    .split_whitespace()
    .map(|drv| format!("'{drv}^out'"))
    .collect::<Vec<_>>()
    .join(" ");

    let (started0, finished0) = (uploads("event=rpc_start"), uploads("event=rpc"));
    let o = run_t(
        &format!(
            "timeout -s KILL {BUILD_TIMEOUT_S} nix build --store '{ENVOY_STORE}' --eval-store '{src}' --no-link {targets} 2>&1 | tail -n 20"
        ),
        BUILD_TIMEOUT_S + 60,
    );
    succeed(&format!("rm -rf {big} {dir}/src"));
    Outcome {
        code: o.code,
        started: uploads("event=rpc_start") - started0,
        finished: uploads("event=rpc") - finished0,
        log: o.stdout,
    }
}

impl Outcome {
    fn assert_finished(&self, what: &str) {
        assert_eq!(
            self.code, 0,
            "{what} did not finish within {BUILD_TIMEOUT_S}s: {} uploads started, {} finished\n{}",
            self.started, self.finished, self.log
        );
    }
}

#[test]
fn a_single_build_of_the_large_input_finishes() {
    build_sharing_one_input("upload-one", 1).assert_finished("one build");
}

#[test]
fn concurrent_builds_sharing_the_large_input_finish() {
    build_sharing_one_input("upload-many", 16).assert_finished("16 builds");
}
