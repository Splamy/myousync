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

        node_modules = pkgs.stdenv.mkDerivation (finalAttrs: {
          pname = "frontend-node_modules";
          version = "1.0.0";
          outputHash = "lTcffeW3oILrW7LQGHTfID+nzVl2hAatywfSNkFeXfw=";
          outputHashAlgo = "sha256";
          outputHashMode = "recursive";

          src = "${self}/ui";

          nativeBuildInputs = with pkgs; [
            bun
          ];
          dontConfigure = true;
          dontFixup = true;

          buildPhase = ''
            runHook preBuild

            bun install --no-progress --frozen-lockfile

            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall

            mkdir -p $out
            cp -R ./node_modules $out

            runHook postInstall
          '';
        });

        build-frontend = book_events:
          pkgs.runCommand "build-qint-frontend" {
            nativeBuildInputs = with pkgs; [bun nodejs];
            src = ./ui;
          } ''
            cp -r "$src/." .
            ln -s ${node_modules}/node_modules ./

            chmod -R +w .

            pwd
            ls -al

            node node_modules/.bin/svelte-kit sync

            pwd
            ls -al
            # bun run build
            bun --bun node_modules/.bin/rsbuild build

            mv dist $out
          '';

        frontend = build-frontend "";

        # myousync-ui = pkgs.callPackage ./nix/default-ui.nix {};
        myousync-ui = frontend;
        myousync = pkgs.callPackage ./. {};
      in rec {
        defaultPackage = packages.myousync-ui;

        packages.myousync = myousync;
        packages.myousync-ui = myousync-ui;

        nixosModules.myousync = import ./nix/myousync.nix self;
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
