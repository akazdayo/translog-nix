{
  config,
  lib,
  pkgs,
  ...
}:
let
  inherit (lib)
    mkEnableOption
    mkIf
    mkMerge
    mkOption
    types
    ;
  cfg = config.services.translog-nix;
  log = cfg.logServer;
  proxy = cfg.proxy;
  validUrl = value: builtins.match "https?://[^?#]+/" value != null;
  serviceSecurity = {
    DynamicUser = true;
    NoNewPrivileges = true;
    PrivateTmp = true;
    ProtectSystem = "strict";
    ProtectHome = true;
  };
in
{
  options.services.translog-nix = {
    package = mkOption {
      type = types.package;
      default = pkgs.callPackage ../package.nix { };
      defaultText = lib.literalExpression "pkgs.callPackage ./nix/package.nix { }";
      description = "ログサーバーとプロキシを含むパッケージ。";
    };

    logServer = {
      enable = mkEnableOption "translog-nix ログサーバー";
      listen = mkOption {
        type = types.str;
        default = "127.0.0.1:8080";
        description = "ログサーバーの待受アドレスとポート。";
      };
      upstream = mkOption {
        type = types.str;
        default = "https://cache.nixos.org/";
        description = "観測対象のキャッシュ URL。末尾に / が必要。";
      };
      cacheId = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "キャッシュの識別子。null の場合は上流 URL を使う。";
      };
      publicKeys = mkOption {
        type = types.listOf types.str;
        default = [ ];
        example = [ "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=" ];
        description = "NarInfo の署名検証に信頼する Nix 公開鍵。";
      };
      reobserveSeconds = mkOption {
        type = types.ints.positive;
        default = 3600;
        description = "再観測の開始間隔（秒）。";
      };
    };

    proxy = {
      enable = mkEnableOption "translog-nix プロキシ";
      listen = mkOption {
        type = types.str;
        default = "127.0.0.1:8081";
        description = "プロキシの待受アドレスとポート。";
      };
      logUrl = mkOption {
        type = types.str;
        default = "http://127.0.0.1:8080/";
        description = "利用するログサーバーの URL。末尾に / が必要。";
      };
    };
  };

  config = mkMerge [
    (mkIf log.enable {
      assertions = [
        {
          assertion = log.publicKeys != [ ];
          message = "services.translog-nix.logServer.publicKeys に信頼する公開鍵を指定してください。";
        }
        {
          assertion = validUrl log.upstream;
          message = "services.translog-nix.logServer.upstream は / で終わる HTTP(S) URL である必要があります。";
        }
      ];
      systemd.services.translog-nix-log-server = {
        description = "translog-nix binary cache observation log";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ];
        wants = [ "network-online.target" ];
        environment = {
          TRANSLOG_LISTEN = log.listen;
          TRANSLOG_UPSTREAM = log.upstream;
          TRANSLOG_PUBLIC_KEYS = lib.concatStringsSep " " log.publicKeys;
          TRANSLOG_DATABASE = "/var/lib/translog-nix/translog.sqlite";
          TRANSLOG_REOBSERVE_SECONDS = toString log.reobserveSeconds;
        }
        // lib.optionalAttrs (log.cacheId != null) { TRANSLOG_CACHE_ID = log.cacheId; };
        serviceConfig = serviceSecurity // {
          ExecStart = lib.getExe' cfg.package "translog-log-server";
          StateDirectory = "translog-nix";
          Restart = "on-failure";
        };
      };
    })
    (mkIf proxy.enable {
      assertions = [
        {
          assertion = validUrl proxy.logUrl;
          message = "services.translog-nix.proxy.logUrl は / で終わる HTTP(S) URL である必要があります。";
        }
      ];
      systemd.services.translog-nix-proxy = {
        description = "translog-nix Nix binary cache proxy";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ];
        wants = [ "network-online.target" ];
        environment = {
          TRANSLOG_LISTEN = proxy.listen;
          TRANSLOG_LOG_URL = proxy.logUrl;
        };
        serviceConfig = serviceSecurity // {
          ExecStart = lib.getExe' cfg.package "translog-proxy";
          Restart = "on-failure";
        };
      };
    })
  ];
}
