{
  description = "Foo Bar";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = {
    self,
    nixpkgs,
  }: let
    systems = ["x86_64-linux"];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs {inherit system;}));
  in {
    nixosModules.default = import ./nix/myousync.nix self;
    # overlays.default = import ./nix/overlay.nix;

    packages = forAllSystems (pkgs: {
      myousync = pkgs.callPackage ./. {};
      default = self.packages.${pkgs.system}.myousync;
    });
  };
}
