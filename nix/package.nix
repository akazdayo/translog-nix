{
  lib,
  rustPlatform,
  cmake,
  pkg-config,
}:
rustPlatform.buildRustPackage {
  pname = "translog-nix";
  version = "0.1.0";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  nativeBuildInputs = [
    cmake
    pkg-config
  ];

  # The integration test starts local HTTP servers; Nix build sandboxes disallow
  # network sockets. Run the workspace tests separately outside the sandbox.
  doCheck = false;

  meta = {
    description = "Observe and pin Nix binary cache NarInfo";
    mainProgram = "translog-proxy";
    platforms = lib.platforms.linux;
  };
}
