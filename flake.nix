{
  description = "Foo Bar";
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
    nixosModules.myousync = import ./myousync.nix;

    packages = forAllSystems (pkgs: {
      myousync = pkgs.callPackage ./. {};
      default = self.packages.${pkgs.system}.myousync;
    });
  };
}
