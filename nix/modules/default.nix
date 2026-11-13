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
      publicKeys = mkOption {
        type = types.listOf types.str;
        default = [ ];
        example = [ "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=" ];
        description = "NarInfo の署名検証に信頼する Nix 公開鍵。";
      };
      transparencyUrl = mkOption {
        type = types.str;
        default = "http://127.0.0.1:8082/";
        description = "Transparency Log の URL。";
      };
    };

    proxy = {
      enable = mkEnableOption "translog-nix プロキシ";
      listen = mkOption {
        type = types.str;
        default = "127.0.0.1:8081";
        description = "プロキシの待受アドレスとポート。";
      };
      upstream = mkOption {
        type = types.str;
        default = "https://cache.nixos.org/";
        description = "観測対象の上流キャッシュ URL。";
      };
      observerUrl = mkOption {
        type = types.str;
        default = "http://127.0.0.1:8080/";
        description = "観測を依頼する Observer の URL。";
      };
      transparencyUrl = mkOption {
        type = types.str;
        default = "http://127.0.0.1:8082/";
        description = "Transparency Log の URL。";
      };
      publicKeys = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "NarInfo の署名検証に使う公開鍵。";
      };
      logPublicKeyFile = mkOption {
        type = types.str;
        description = "Transparency Log の公開鍵ファイル。";
      };
      syncSeconds = mkOption {
        type = types.ints.positive;
        default = 3600;
        description = "ログ同期の開始間隔（秒）。";
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
          assertion = validUrl log.upstream && validUrl log.transparencyUrl;
          message = "services.translog-nix.logServer の URL は / で終わる HTTP(S) URL である必要があります。";
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
          TRANSLOG_TRANSPARENCY_URL = log.transparencyUrl;
        };
        serviceConfig = serviceSecurity // {
          ExecStart = lib.getExe' cfg.package "translog-log-server";
          Restart = "on-failure";
        };
      };
    })
    (mkIf proxy.enable {
      assertions = [
        {
          assertion = validUrl proxy.observerUrl && validUrl proxy.upstream && validUrl proxy.transparencyUrl;
          message = "services.translog-nix.proxy の URL は / で終わる HTTP(S) URL である必要があります。";
        }
        {
          assertion = proxy.publicKeys != [ ];
          message = "services.translog-nix.proxy.publicKeys に信頼する公開鍵を指定してください。";
        }
      ];
      systemd.services.translog-nix-proxy = {
        description = "translog-nix Nix binary cache proxy";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ];
        wants = [ "network-online.target" ];
        environment = {
          TRANSLOG_LISTEN = proxy.listen;
          TRANSLOG_UPSTREAM = proxy.upstream;
          TRANSLOG_OBSERVER_URL = proxy.observerUrl;
          TRANSLOG_TRANSPARENCY_URL = proxy.transparencyUrl;
          TRANSLOG_PUBLIC_KEYS = lib.concatStringsSep " " proxy.publicKeys;
          TRANSLOG_LOG_PUBLIC_KEY_FILE = proxy.logPublicKeyFile;
          TRANSLOG_SNAPSHOT = "/var/lib/translog-nix-proxy/snapshot.json";
          TRANSLOG_SYNC_COMMAND = lib.getExe' cfg.package "translog-sync";
          TRANSLOG_SYNC_SECONDS = toString proxy.syncSeconds;
        };
        serviceConfig = serviceSecurity // {
          ExecStart = lib.getExe' cfg.package "translog-proxy";
          StateDirectory = "translog-nix-proxy";
          Restart = "on-failure";
        };
      };
    })
  ];
}
