use crate::fixtures::*;
use crate::harness::*;

#[test]
fn idle_exit_and_socket_activation() {
    succeed(MACHINE, "systemctl stop nix-grpc-daemon.service");
    assert_eq!(unit_state(MACHINE, "nix-grpc-daemon.service"), "inactive");
    let out = succeed(MACHINE, &format!("nix store info --json --store '{STORE}'"));
    assert!(out.contains(r#""url":"grpc://127.0.0.1:50051"#), "{out}");
    assert_eq!(unit_state(MACHINE, "nix-grpc-daemon.service"), "active");
    wait_until_succeeds(
        MACHINE,
        "test \"$(systemctl show -P ActiveState nix-grpc-daemon.service)\" = inactive",
        30,
    );
    assert_journal(MACHINE, "nix-grpc-daemon", "event=idle_exit");
    succeed(MACHINE, &format!("nix store info --store '{STORE}'"));
}

#[test]
fn nix_daemon_restart_does_not_fail_the_next_rpc() {
    let p = hello_path();
    succeed(
        MACHINE,
        &format!("echo warm > /tmp/warm && nix store add --store '{STORE}' /tmp/warm"),
    );
    assert_journal(
        MACHINE,
        "nix-daemon",
        "accepted connection from pid .*nix-grpc-daemon",
    );
    // SIGKILL, like a crashed container: no orderly close of the pooled connections.
    succeed(MACHINE, "systemctl kill -s KILL nix-daemon.service");
    wait_until_succeeds(MACHINE, "! systemctl is-active nix-daemon.service", 30);
    succeed(MACHINE, "systemctl start nix-daemon.socket");
    succeed(MACHINE, &format!("nix path-info --store '{STORE}' '{p}'"));
    succeed(
        MACHINE,
        &format!("echo again > /tmp/warm && nix store add --store '{STORE}' /tmp/warm"),
    );
}

#[test]
fn renewed_server_certificate_is_served_without_restart() {
    wait_for_unit(MACHINE, "nix-grpc-daemon-mtls.service");
    wait_for_open_port(MACHINE, 50052);
    let d = cert_dir();
    succeed(
        MACHINE,
        &format!(
            "cd {d} && \
             openssl req -newkey rsa:2048 -nodes -keyout new.key -out new.csr -subj /CN=localhost 2>/dev/null && \
             openssl x509 -req -in new.csr -days 1 -CA ca.pem -CAkey ca.key -set_serial 0x$RANDOM \
             -extfile <(printf 'subjectAltName=DNS:localhost,DNS:renewed.example') -out new.pem && \
             mv new.key server.key && mv new.pem server.pem"
        ),
    );
    wait_until_succeeds(
        MACHINE,
        "openssl s_client -connect localhost:50052 </dev/null 2>/dev/null \
         | openssl x509 -noout -ext subjectAltName | grep -c renewed.example",
        30,
    );
    succeed(
        MACHINE,
        &format!("nix store info --json --store '{}'", cert_store("client")),
    );
}

#[test]
fn default_client_cert_lookup() {
    wait_for_unit(MACHINE, "nix-grpc-daemon-mtls.service");
    wait_for_open_port(MACHINE, 50052);
    let d = cert_dir();
    succeed(
        MACHINE,
        &format!(
            "install -d /var/lib/nix-grpc-store && \
             install -m 0644 {d}/client.pem /var/lib/nix-grpc-store/client.crt && \
             install -m 0600 {d}/client.key /var/lib/nix-grpc-store/client.key"
        ),
    );
    let out = run_t(
        MACHINE,
        &format!("nix store add --store '{}' /etc/hello.nix", anon_store()),
        120,
    );
    succeed(MACHINE, "rm -r /var/lib/nix-grpc-store");
    assert_eq!(out.code, 0, "{}", out.combined());
}
