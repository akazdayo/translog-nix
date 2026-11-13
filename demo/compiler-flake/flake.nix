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
          buildPhase = ''
            cat > hello.c <<'SOURCE'
            #include <stdio.h>
            int main(void) { puts("Hello from the legitimate program!"); return 0; }
            SOURCE
            cc hello.c -o hello
          '';
          installPhase = ''
            mkdir -p "$out/bin"
            cp hello "$out/bin/hello"
          '';
        };
      });
    };
}
