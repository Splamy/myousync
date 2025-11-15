{
  description = "Myousync";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };
  outputs = {
    self,
    nixpkgs,
  }: let
    systems = ["x86_64-linux"];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs {inherit system;}));
  in {
    nixosModules.myousync = import ./nix/myousync.nix self;
    # overlays.default = import ./nix/overlay.nix;

    packages = forAllSystems (pkgs: {
      myousync = pkgs.callPackage ./. {};
      default = self.packages.${pkgs.stdenv.hostPlatform}.myousync;
    });

    devShells = forAllSystems (pkgs: {
      default = pkgs.callPackage ./shell.nix {};
    });
  };
}
