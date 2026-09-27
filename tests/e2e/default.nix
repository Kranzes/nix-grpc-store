{
  stdenv,
  cargo,
  rustc,
  jq,
}:
stdenv.mkDerivation {
  name = "nix-grpc-e2e";
  src = ./.;
  nativeBuildInputs = [
    cargo
    rustc
    jq
  ];
  buildPhase = ''
    export CARGO_HOME=$TMPDIR/cargo
    cargo test --offline --release --no-run --features e2e \
      --message-format=json > $TMPDIR/cargo.json
  '';
  installPhase = ''
    bin=$(jq -r 'select(.reason=="compiler-artifact" and .target.name=="e2e" and .profile.test==true) | .executable // empty' $TMPDIR/cargo.json | tail -1)
    [ -n "$bin" ] || { echo "e2e test binary not found" >&2; exit 1; }
    install -Dm755 "$bin" $out/bin/nix-grpc-e2e
  '';
}
