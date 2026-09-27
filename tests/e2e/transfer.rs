use std::time::Instant;

use crate::fixtures::*;
use crate::harness::*;

#[test]
fn add_path_and_read_it_back_locally() {
    let p = succeed(
        MACHINE,
        &format!("nix store add --store '{STORE}' /etc/hello.nix"),
    );
    let p = p.trim();
    succeed(MACHINE, &format!("nix path-info '{p}'"));
    succeed(MACHINE, &format!("test -e '{p}'"));
}

#[test]
fn build_over_grpc() {
    let p = hello_path();
    succeed(MACHINE, &format!("grep -q hello-over-grpc '{p}'"));
}

#[test]
fn build_whose_log_is_not_utf8() {
    let p = succeed(
        MACHINE,
        &format!(
            "nix build --store '{STORE}' --impure -f /etc/rawlog.nix --no-link --print-out-paths"
        ),
    );
    succeed(MACHINE, &format!("grep -q rawlog-over-grpc '{}'", p.trim()));
}

#[test]
fn copy_to_a_local_scratch_store() {
    let p = hello_path();
    let dir = workdir("scratch");
    succeed(
        MACHINE,
        &format!("nix copy --no-check-sigs --from '{STORE}' --to {dir}/store '{p}'"),
    );
    succeed(
        MACHINE,
        &format!("test -e {dir}/store/nix/store/$(basename '{p}')"),
    );
}

#[test]
fn bulk_upload_is_not_pinned_after_the_rpc_returns() {
    let dir = workdir("bulk");
    succeed(
        MACHINE,
        &format!("dd if=/dev/urandom of={dir}/blob bs=1M count=128 status=none"),
    );
    let up = succeed(
        MACHINE,
        &format!("nix store add --store {dir}/src --mode flat {dir}/blob"),
    );
    let up = up.trim();
    succeed(
        MACHINE,
        &format!("nix copy --no-check-sigs --from {dir}/src --to '{STORE}' '{up}'"),
    );
    succeed(MACHINE, &format!("test -e '{up}'"));
    succeed(MACHINE, &format!("nix-store --delete '{up}'"));
    fail(MACHINE, &format!("test -e '{up}'"));
}

#[test]
fn many_small_paths_round_trip() {
    let dir = workdir("small");
    succeed(
        MACHINE,
        &format!(
            "mkdir -p {dir}/files && for i in $(seq 200); do \
             head -c 4096 /dev/urandom | base64 > {dir}/files/f$i; done"
        ),
    );
    let added = succeed(
        MACHINE,
        &format!("cd {dir}/files && nix-store --store {dir}/src --add f*"),
    );
    let small: Vec<&str> = added.lines().collect();
    assert_eq!(small.len(), 200, "{added}");
    let paths = small
        .iter()
        .map(|p| format!("'{p}'"))
        .collect::<Vec<_>>()
        .join(" ");

    let t0 = Instant::now();
    succeed(
        MACHINE,
        &format!("nix copy --no-check-sigs --from {dir}/src --to '{STORE}' {paths}"),
    );
    println!(
        "[bench] small/upload   {:6.2}s (200 paths)",
        t0.elapsed().as_secs_f64()
    );
    for p in &small[..3] {
        succeed(MACHINE, &format!("test -e '{p}'"));
    }

    let t0 = Instant::now();
    succeed(
        MACHINE,
        &format!("nix copy --no-check-sigs --from '{STORE}' --to {dir}/dst {paths}"),
    );
    println!(
        "[bench] small/download {:6.2}s (200 paths)",
        t0.elapsed().as_secs_f64()
    );
    for p in &small[..3] {
        succeed(
            MACHINE,
            &format!(
                "b=$(basename '{p}'); test -e {dir}/dst/nix/store/$b && \
                 cmp {dir}/src/nix/store/$b {dir}/dst/nix/store/$b"
            ),
        );
    }
}
