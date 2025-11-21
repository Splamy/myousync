{pkgs ? import <nixpkgs> {}}: let
in
  pkgs.buildNpmPackage (finalAttrs: {
    pname = "myousync-ui";
    version = "1.0.0";

    src = ./../../ui/.;
    npmDeps = pkgs.importNpmLock {
      npmRoot = ./../../ui/.;
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
  })
