{
  description = "Myousync";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = (import nixpkgs) {
          inherit system;
        };
        myousync-ui = pkgs.callPackage ./nix/frontend/. {};
        myousync = pkgs.callPackage ./nix/backend/. {};
      in rec {
        defaultPackage = packages.myousync;

        packages.myousync = myousync;
        packages.myousync-ui = myousync-ui;

        devShells.default = pkgs.callPackage ./nix/shell.nix {};
      }
    )
    // {
      nixosModules.myousync = import ./nix/myousync.nix self;
    };
}
