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
        lib = pkgs.lib;

        frontend = pkgs.buildNpmPackage (finalAttrs: {
          pname = "mfron";
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
          #
          # NODE_OPTIONS = "--openssl-legacy-provider";

          installPhase = ''
            runHook preInstall

            mkdir -p $out
            cp -R ./dist/* $out
            # cp -R ./ $out

            runHook postInstall
          '';

          meta = {
            description = "arst";
          };
        });

        myousync-ui = frontend;
        myousync = pkgs.callPackage ./. {};
      in rec {
        defaultPackage = packages.myousync;

        packages.myousync = myousync;
        packages.myousync-ui = myousync-ui;

        nixosModules.myousync = import ./nix/myousync.nix self;

        devShells.default = pkgs.callPackage ./shell.nix {};
      }
    );

  # outputs = {
  #   self,
  #   nixpkgs,
  # }: let
  #   systems = ["x86_64-linux"];
  #   forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs {inherit system;}));
  # in {
  #   nixosModules.myousync = import ./nix/myousync.nix self;
  #   # overlays.default = import ./nix/overlay.nix;
  #
  #   packages = forAllSystems (pkgs: {
  #     myousync-ui = pkgs.callPackage ./nix/default-ui.nix {};
  #     myousync = pkgs.callPackage ./. {};
  #
  #     default = self.packages.${pkgs.stdenv.hostPlatform.system}.myousync;
  #   });
  #
  #   devShells = forAllSystems (pkgs: {
  #     default = pkgs.callPackage ./shell.nix {};
  #   });
  # };
}
