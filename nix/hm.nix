self: {
  config,
  pkgs,
  lib,
  ...
}: let
  cfg = config.programs.lazyops;
  defaultPackage = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
in {
  meta.maintainers = with lib.maintainers; [dashietm];
  options.programs.lazyops = with lib; {
    enable = mkEnableOption "lazyops";

    package = mkOption {
      type = with types; nullOr package;
      default = defaultPackage;
      defaultText = lib.literalExpression ''
        lazyops.packages.''${pkgs.stdenv.hostPlatform.system}.default
      '';
      description = mdDoc ''
        Package to run
      '';
    };

    settings = lib.mkOption {
      default = null;
      example = {
        default_project = "myproject";
      };
      type = with lib.types; nullOr (attrsOf anything);
      description = ''
        See https://github.com/GappelSolutions/lazyops/blob/main/config.example.toml for more options.
      '';
    };
  };
  config = lib.mkIf cfg.enable {
    home.packages = lib.optional (cfg.package != null) cfg.package;
    xdg.configFile."lazyops/config.toml" = lib.mkIf (cfg.settings != null) {
      source =
        (pkgs.formats.toml {}).generate "config" cfg.settings;
    };
  };
}
