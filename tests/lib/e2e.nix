# Python for a NixOS testScript: runs the Rust tests on the driver host.
{ pkgs, e2eTests }:
''
  import os, subprocess

  def run_e2e(nodes, extra_env, *args):
      env = dict(os.environ)
      env["NGS_SSH"] = "${pkgs.openssh}/bin/ssh"
      env["NGS_SSH_CONFIG"] = "${pkgs.systemd}/lib/systemd/ssh_config.d/20-systemd-ssh-proxy.conf"
      for name, node in nodes.items():
          env[f"NGS_{name.upper()}_SOCK"] = str(node.vsock_host)
      env.update(extra_env)
      subprocess.run(["${e2eTests}/bin/nix-grpc-e2e", *args, "--nocapture"], env=env, check=True)
''
