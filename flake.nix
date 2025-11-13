{
  description = "Foo Bar";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };
  outputs = {
    self,
    nixpkgs,
  }: let
    supportedSystems = ["x86_64-linux"];
    forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    pkgsFor = nixpkgs.legacyPackages;
  in {
    nixosModules.myousync = import ./myousync.nix;

    packages = forAllSystems (system: {
      myousync = pkgsFor.${system}.callPackage ./. {};
      default = self.packages.${system}.myousync;
    });
  };
}
