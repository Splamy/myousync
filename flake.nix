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
    flake-utils.lib.eachSystem ["x86_64-linux"] (
      system: let
        pkgs = (import nixpkgs) {
          inherit system;
        };
        myousync-ui = pkgs.buildNpmPackage (finalAttrs: {
          pname = "myousync-ui";
          version = "1.0.0";

          src = "${self}/ui";
          # npmRoot = "./ui";
          # npmDepsHash = "sha256-bAkXqFvrYiEMltW21CE7VMeqRzAOeJfztCRLkgWNfIo=";

          npmDeps = pkgs.importNpmLock {
            npmRoot = "${self}/ui";
          };
          npmConfigHook = pkgs.importNpmLock.npmConfigHook;

          # The prepack script runs the build script, which we'd rather do in the build phase.
          npmPackFlags = ["--ignore-scripts" "--legacy-peer-deps"];

          installPhase = ''
            runHook preInstall

            mkdir -p $out
            cp -R ./dist/* $out

            runHook postInstall
          '';

          meta = {
            description = "myousync-ui static pages";
          };
        });

        myousync = pkgs.callPackage ./. {};
      in rec {
        defaultPackage = packages.myousync;

        packages.myousync = myousync;
        packages.myousync-ui = myousync-ui;

        nixosModules.myousync = import ./nix/myousync.nix self;

        devShells.default = pkgs.callPackage ./shell.nix {};
      }
    );
}
