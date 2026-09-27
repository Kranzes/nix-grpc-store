use std::sync::OnceLock;

use crate::harness::{MACHINE, env, succeed};

pub const STORE: &str = "grpc://127.0.0.1:50051?insecure=1";

pub fn cert_dir() -> String {
    env("NGS_CERT_DIR")
}

pub fn cert_store(name: &str) -> String {
    let d = cert_dir();
    format!(
        "grpc://localhost:50052?ca-cert={d}/ca.pem&client-cert={d}/{name}.pem&client-key={d}/{name}.key"
    )
}

pub fn anon_store() -> String {
    format!("grpc://localhost:50052?ca-cert={}/ca.pem", cert_dir())
}

pub fn hello_path() -> &'static str {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| {
        succeed(
            MACHINE,
            &format!(
                "nix build --store '{STORE}' --impure -f /etc/hello.nix --no-link --print-out-paths"
            ),
        )
        .trim()
        .to_string()
    })
}

pub fn workdir(name: &str) -> String {
    let dir = format!("/root/{name}");
    succeed(MACHINE, &format!("mkdir -p {dir}"));
    dir
}

pub fn deny_file(dir: &str) -> String {
    let path = format!("{dir}/denyfile");
    succeed(
        MACHINE,
        &format!("head -c 64 /dev/urandom | base64 > {path}"),
    );
    path
}

pub fn drv_expr(name: &str, script: &str) -> String {
    format!(
        r#"derivation {{ name = "{name}"; system = builtins.currentSystem; builder = "/bin/sh"; args = [ "-c" "{script}" ]; }}"#
    )
}
