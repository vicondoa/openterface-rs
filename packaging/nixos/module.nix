{ self }:

{ config, lib, pkgs, ... }:

let
  cfg = config.programs.openterface-rs;
  inherit (lib) mkEnableOption mkIf mkOption types;

  boolEnv = value: if value then "1" else "0";
  optionalEnv = name: value:
    lib.optionalAttrs (value != null) { ${name} = toString value; };
  optionalBoolEnv = name: value:
    lib.optionalAttrs value { ${name} = "1"; };

  runtimeEnv =
    optionalEnv "RUST_LOG" cfg.logFilter
    // optionalEnv "OPENTERFACE_MOUSE_INTERVAL_MS" cfg.mouseIntervalMs
    // lib.optionalAttrs (!cfg.throttle.enable) { OPENTERFACE_THROTTLE = "0"; }
    // optionalEnv "OPENTERFACE_IDLE_DECODE_MS" cfg.throttle.idleDecodeMs
    // optionalEnv "OPENTERFACE_INPUT_WAKE_MS" cfg.throttle.inputWakeMs
    // optionalEnv "OPENTERFACE_IDLE_WATCHDOG_MS" cfg.throttle.idleWatchdogMs
    // optionalBoolEnv "OPENTERFACE_FULLSCREEN" cfg.fullscreen
    // optionalEnv "OPENTERFACE_USE_LIBDECOR" (boolEnv cfg.useLibdecor)
    // optionalEnv "OPENTERFACE_ENABLE_PASTE" (boolEnv cfg.paste.enable)
    // optionalEnv "OPENTERFACE_PASTE_SHORTCUT" cfg.paste.shortcut
    // optionalEnv "OPENTERFACE_MIDDLE_CLICK_PASTE" cfg.paste.middleClick
    // optionalEnv "OPENTERFACE_PASTE_MAX_CHARS" cfg.paste.maxChars
    // optionalBoolEnv "OPENTERFACE_REQUIRE_GPU" cfg.requireGpu
    // cfg.extraEnvironment;

  configuredPackage =
    if runtimeEnv == { } then cfg.package else
    pkgs.symlinkJoin {
      name = "${cfg.package.pname or "openterface-rs"}-configured";
      paths = [ cfg.package ];
      nativeBuildInputs = [ pkgs.makeWrapper ];
      postBuild = ''
        wrapProgram "$out/bin/openterface-rs" \
          ${lib.concatStringsSep " \\\n  " (lib.mapAttrsToList (name: value:
            "--set ${lib.escapeShellArg name} ${lib.escapeShellArg value}") runtimeEnv)}
      '';
      inherit (cfg.package) meta;
    };
in
{
  options.programs.openterface-rs = {
    enable = mkEnableOption "openterface-rs";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.openterface-rs;
      defaultText = lib.literalExpression "inputs.openterface-rs.packages.${pkgs.stdenv.hostPlatform.system}.openterface-rs";
      description = "openterface-rs package to install.";
    };

    installUdevRules = mkOption {
      type = types.bool;
      default = true;
      description = "Install the bundled udev rules for Openterface devices.";
    };

    createGroup = mkOption {
      type = types.bool;
      default = true;
      description = "Create the fallback `openterface` group for non-seat/headless device access.";
    };

    logFilter = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "info,openterface_gui=debug";
      description = "Default `RUST_LOG` filter for the installed wrapper.";
    };

    mouseIntervalMs = mkOption {
      type = types.nullOr types.ints.positive;
      default = null;
      example = 33;
      description = "Default `OPENTERFACE_MOUSE_INTERVAL_MS`; null leaves the binary default.";
    };

    fullscreen = mkOption {
      type = types.bool;
      default = false;
      description = "Set `OPENTERFACE_FULLSCREEN=1` in the installed wrapper.";
    };

    useLibdecor = mkOption {
      type = types.bool;
      default = true;
      description = "Set `OPENTERFACE_USE_LIBDECOR`; true is recommended for niri/CSD compositors.";
    };

    requireGpu = mkOption {
      type = types.bool;
      default = false;
      description = "Set `OPENTERFACE_REQUIRE_GPU=1`; mostly useful for render tests.";
    };

    throttle = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = "Enable idle MJPEG decode throttling (`OPENTERFACE_THROTTLE`).";
      };

      idleDecodeMs = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        example = 100;
        description = "Default `OPENTERFACE_IDLE_DECODE_MS`; null leaves the binary default.";
      };

      inputWakeMs = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        example = 250;
        description = "Default `OPENTERFACE_INPUT_WAKE_MS`; null leaves the binary default.";
      };

      idleWatchdogMs = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        example = 1000;
        description = "Default `OPENTERFACE_IDLE_WATCHDOG_MS`; null leaves the binary default.";
      };
    };

    paste = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = "Enable focused GUI paste (`OPENTERFACE_ENABLE_PASTE`).";
      };

      shortcut = mkOption {
        type = types.str;
        default = "ctrl-shift-v";
        example = "ctrl-alt-shift-v";
        description = "Default `OPENTERFACE_PASTE_SHORTCUT` modifier+V chord.";
      };

      middleClick = mkOption {
        type = types.enum [ "off" "primary" "clipboard" ];
        default = "off";
        description = "Default `OPENTERFACE_MIDDLE_CLICK_PASTE` behavior.";
      };

      maxChars = mkOption {
        type = types.ints.between 1 65536;
        default = 4096;
        description = "Default `OPENTERFACE_PASTE_MAX_CHARS` normalized-character cap.";
      };
    };

    extraEnvironment = mkOption {
      type = types.attrsOf types.str;
      default = { };
      example = { OPENTERFACE_MOUSE_INTERVAL_MS = "50"; };
      description = "Additional environment variables set on the installed wrapper.";
    };
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [ configuredPackage ];
    services.udev.packages = mkIf cfg.installUdevRules [ cfg.package ];
    users.groups.openterface = mkIf cfg.createGroup { };
  };
}
