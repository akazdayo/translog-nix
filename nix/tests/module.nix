{ nixpkgs }:
let
  pkgs = import nixpkgs { };
  lib = pkgs.lib;
  evaluate =
    extra:
    (import (nixpkgs + "/nixos/lib/eval-config.nix") {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        ../modules
        {
          system.stateVersion = "25.11";
          services.translog-nix.package = pkgs.hello;
        }
        extra
      ];
    }).config;
  proxyOnly = evaluate {
    services.translog-nix.proxy = {
      enable = true;
      observerUrl = "https://log.example/";
      publicKeys = [ "example:key" ];
      logPublicKeyFile = "/tmp/log-public-key";
    };
  };
  both = evaluate {
    services.translog-nix.logServer = {
      enable = true;
      upstream = "https://cache.example/";
      publicKeys = [ "example:key" ];
    };
    services.translog-nix.proxy = { enable = true; publicKeys = [ "example:key" ]; logPublicKeyFile = "/tmp/log-public-key"; };
  };
  noKeys = evaluate { services.translog-nix.logServer.enable = true; };
  invalidUrl = evaluate {
    services.translog-nix.proxy = {
      enable = true;
      observerUrl = "https://example";
      publicKeys = [ "example:key" ];
      logPublicKeyFile = "/tmp/log-public-key";
    };
  };
  failed =
    cfg:
    map (x: x.message) (
      builtins.filter (x: !x.assertion && lib.hasPrefix "services.translog-nix" x.message) cfg.assertions
    );
in
assert !(proxyOnly.systemd.services ? translog-nix-log-server);
assert
  proxyOnly.systemd.services.translog-nix-proxy.environment.TRANSLOG_OBSERVER_URL
  == "https://log.example/";
assert both.systemd.services.translog-nix-log-server.environment.TRANSLOG_UPSTREAM == "https://cache.example/";
assert
  both.systemd.services.translog-nix-log-server.environment.TRANSLOG_PUBLIC_KEYS == "example:key";
assert both.systemd.services.translog-nix-proxy.environment.TRANSLOG_SYNC_SECONDS == "3600";
assert both.systemd.services.translog-nix-proxy.serviceConfig.StateDirectory == "translog-nix-proxy";
assert both.systemd.services.translog-nix-log-server.serviceConfig.DynamicUser;
assert failed both == [ ];
assert builtins.length (failed noKeys) == 1;
assert builtins.length (failed invalidUrl) == 1;
true
