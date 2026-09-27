use super::helpers::*;
use crate::harness::*;

#[test]
fn outputs_carry_the_cache_signature() {
    farm_ready();
    let pubkey = env("NGS_SIGNING_PUBKEY");
    let signed = build(&envoy(), "sig", "");
    for w in WORKERS {
        let out = succeed(
            CLIENT,
            &format!("nix path-info --sigs --store '{}' {signed}", direct(w)),
        );
        let key = pubkey.split(':').next().unwrap();
        assert!(out.contains(&format!("{key}:")), "{} {out}", w.0);
    }
    succeed(
        CLIENT,
        &format!(
            "nix-store --delete {signed} && nix copy --from '{}' --option trusted-public-keys '{pubkey}' {signed}",
            envoy()
        ),
    );
}

#[test]
fn build_hook_through_nix_daemon() {
    farm_ready();
    let out = succeed(
        CLIENT,
        &format!(
            "NIX_REMOTE=daemon nix build --log-format internal-json {} --print-out-paths --no-link -f {} --argstr tag hook 2>/tmp/hook.log",
            hook(),
            expr("JOB")
        ),
    );
    let built = succeed(CLIENT, &format!("cat {}", out.trim()));
    assert!(built.contains("farm-top-hook"), "{built}");
    let log = succeed(CLIENT, "cat /tmp/hook.log");
    let has = |parts: &[&str]| log.lines().any(|l| parts.iter().all(|p| l.contains(p)));
    assert!(has(&["LOG-farm-top-hook", r#""type":101"#]), "{log}");
    assert!(has(&["farmPhase", r#""type":104"#]), "{log}");
    assert!(
        has(&[r#""type":105"#, "lb:50051", r#""fields":["/nix/store/"#]),
        "{log}"
    );
}

#[test]
fn an_input_only_one_worker_has_still_reaches_the_builder() {
    farm_ready();
    let dep = expr("DEP");
    let inp = succeed(
        CLIENT,
        &format!("nix-build --no-out-link {dep} -A input --argstr tag w1only"),
    );
    let inp = inp.trim();
    succeed(
        CLIENT,
        &format!("nix-store --export {inp} > /tmp/shared/inp.closure"),
    );
    succeed(WORKER1, "nix-store --import < /tmp/shared/inp.closure");
    fail(WORKER2, &format!("test -e {inp}"));
    for salt in ["a", "b", "c"] {
        succeed(
            CLIENT,
            &format!(
                "NIX_REMOTE=daemon nix build -L {} --no-link -f {dep} job --argstr tag w1only --argstr salt {salt} >&2",
                hook()
            ),
        );
    }
}
