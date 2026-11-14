{ lib, rustPlatform, buildGoModule, symlinkJoin, cmake, pkg-config, aws-lc }:
let
  rust = rustPlatform.buildRustPackage {
    pname = "translog-nix-rust";
    version = "0.1.0";
    src = lib.fileset.toSource {
      root = ../.;
      fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../crates ];
    };
    cargoLock.lockFile = ../Cargo.lock;
    nativeBuildInputs = [ cmake pkg-config ];
    buildInputs = [ aws-lc ];
    doCheck = false;
  };
  tessera = buildGoModule {
    pname = "translog-tessera";
    version = "0.1.0";
    src = ../transparency-log;
    vendorHash = "sha256-Tysq/TCsaJw9QTCoVFjIuL2XxQFf4S2rMwZO63AgLEk=";
    subPackages = [ "." "cmd/sync" ];
    postInstall = ''
      mv "$out/bin/transparency-log" "$out/bin/translog-transparency"
      mv "$out/bin/sync" "$out/bin/translog-sync"
    '';
    doCheck = false;
  };
in symlinkJoin {
  name = "translog-nix-0.1.0";
  paths = [ rust tessera ];
  meta = {
    description = "Observe Nix binary caches with a transparency log";
    mainProgram = "translog-proxy";
    platforms = lib.platforms.linux;
  };
}
