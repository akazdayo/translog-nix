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
      logUrl = "https://log.example/";
    };
  };
  both = evaluate {
    services.translog-nix.logServer = {
      enable = true;
      upstream = "https://cache.example/";
      cacheId = "example";
      publicKeys = [ "example:key" ];
      reobserveSeconds = 12;
    };
    services.translog-nix.proxy.enable = true;
  };
  noKeys = evaluate { services.translog-nix.logServer.enable = true; };
  invalidUrl = evaluate {
    services.translog-nix.proxy = {
      enable = true;
      logUrl = "https://example";
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
  proxyOnly.systemd.services.translog-nix-proxy.environment.TRANSLOG_LOG_URL
  == "https://log.example/";
assert both.systemd.services.translog-nix-log-server.environment.TRANSLOG_CACHE_ID == "example";
assert
  both.systemd.services.translog-nix-log-server.environment.TRANSLOG_PUBLIC_KEYS == "example:key";
assert both.systemd.services.translog-nix-log-server.environment.TRANSLOG_REOBSERVE_SECONDS == "12";
assert both.systemd.services.translog-nix-log-server.serviceConfig.StateDirectory == "translog-nix";
assert both.systemd.services.translog-nix-log-server.serviceConfig.DynamicUser;
assert failed both == [ ];
assert builtins.length (failed noKeys) == 1;
assert builtins.length (failed invalidUrl) == 1;
true
