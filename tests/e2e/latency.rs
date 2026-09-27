//! netem on lo delays the gRPC TCP path (unix sockets are unaffected), so
//! one VM can measure how well the protocol hides RTT.
use crate::fixtures::*;

const N_PATHS: usize = 200;
const N_DRVS: usize = 20;
const RTT_MS: u64 = 50;
const STORE_URI: &str = "grpc://127.0.0.1:50051?insecure=1";

/// Runs `cmd` on the VM and returns the elapsed seconds, so ssh latency stays out of it.
fn timed(cmd: &str) -> f64 {
    let o = run_t(
        &format!(
            "s=$(date +%s%N); rc=0; {{ {cmd}\n}} >/root/timed.log 2>&1 || rc=$?; e=$(date +%s%N); \
             echo $(( (e - s) / 1000000 )); exit $rc"
        ),
        600,
    );
    assert_eq!(
        o.code,
        0,
        "{cmd}\n{}",
        succeed("tail -n 30 /root/timed.log")
    );
    let ms: f64 = o.stdout.trim().parse().expect("elapsed ms");
    ms / 1000.0
}

struct Netem;

impl Netem {
    fn add(one_way_ms: u64) -> Netem {
        succeed(&format!(
            "tc qdisc add dev lo root netem delay {one_way_ms}ms"
        ));
        Netem
    }
}

impl Drop for Netem {
    fn drop(&mut self) {
        let _ = run_t("tc qdisc del dev lo root netem", 30);
    }
}

#[test]
fn copy_and_remote_build_hide_the_round_trip() {
    wait_for_unit("nix-grpc-daemon.socket");
    wait_for_open_port(50051);
    let chain = env("NGS_CHAIN_EXPR");
    let system = env("NGS_SYSTEM");

    succeed(&format!(
        "mkdir -p /root/small && for i in $(seq {N_PATHS}); do \
         head -c 4096 /dev/urandom | base64 > /root/small/f$i; done"
    ));
    let paths: Vec<String> = succeed("cd /root/small && nix-store --store /root/src --add f*")
        .lines()
        .map(|p| format!("'{p}'"))
        .collect();
    assert_eq!(paths.len(), N_PATHS);
    let paths = paths.join(" ");
    succeed(&format!(
        "nix copy --no-check-sigs --from /root/src --to '{STORE_URI}' {paths}"
    ));

    let download = |label: &str| {
        succeed("rm -rf /root/dst");
        let t = timed(&format!(
            "nix copy --no-check-sigs --from '{STORE_URI}' --to /root/dst {paths}"
        ));
        println!("[bench] download/{label:9} {t:6.2}s ({N_PATHS} paths)");
        t
    };
    let build = |label: &str| {
        let t = timed(&format!(
            "nix build -f {chain} --argstr salt {label} --store /root/bstore --no-link \
             --max-jobs 0 --builders '{STORE_URI} {system} - 4 1'"
        ));
        println!("[bench] build/{label:12} {t:6.2}s ({N_DRVS} drvs)");
        t
    };

    download("loopback");
    let build_base = build("loopback");

    let (copy_delayed, build_delayed) = {
        let _netem = Netem::add(RTT_MS / 2);
        (download("50ms-rtt"), build("delayed"))
    };

    // A client paying one round trip per path would need at least this long.
    let floor = N_PATHS as f64 * RTT_MS as f64 / 1000.0;
    println!("[bench] sequential floor {floor:6.2}s");
    assert!(
        copy_delayed < floor * 0.8,
        "download took {copy_delayed:.2}s, expected pipelining to stay well below the sequential floor of {floor:.2}s"
    );

    let per_drv = (build_delayed - build_base) / N_DRVS as f64;
    let limit = 25.0 * RTT_MS as f64 / 1000.0;
    println!(
        "[bench] build RTT overhead {:6.0}ms per drv",
        per_drv * 1000.0
    );
    assert!(
        per_drv < limit,
        "remote build pays {per_drv:.2}s of RTT overhead per derivation, expected less than {limit:.2}s"
    );
}
