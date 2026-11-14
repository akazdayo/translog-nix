{
  description = "A small, legitimate C program for the compiler substitution demo";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { nixpkgs, ... }:
    let
      systems = [ "aarch64-linux" "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAllSystems (pkgs: {
        compiler = pkgs.stdenv.cc;
        default = pkgs.stdenv.mkDerivation {
          pname = "compiler-demo";
          version = "1";
          meta.mainProgram = "hello";
          dontUnpack = true;
          src = ./hello.c;
          buildPhase = ''
            cc "$src" -o hello
          '';
          installPhase = ''
            mkdir -p "$out/bin"
            cp hello "$out/bin/hello"
          '';
        };
      });
    };
}
