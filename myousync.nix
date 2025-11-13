{
  config,
  lib,
  pkgs,
  ...
}:
with lib; let
  cfg = config.services.myousync;
in {
  options.services.myousync = {
    enable = mkEnableOption "myousync";

    package = mkOption {
      type = types.package;
      default = pkgs.myousync;
      defaultText = literalExpression "pkgs.myousync";
      description = "myousync package to use.";
    };

    extraConfig = mkOption {
      default = "";
      example = ''
        foo bar
      '';
      type = types.lines;
      description = ''
        Extra settings for foo.
      '';
    };
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [cfg.package];
  };
}
