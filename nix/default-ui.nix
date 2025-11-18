{
  mkBunDerivation,
  pkgs ? import <nixpkgs> {},
}: let
in
  mkBunDerivation {
    pname = "bun2nix-example";
    version = "1.0.0";

    src = ./ui/.;

    bunNix = ./ui/bun.nix;

    index = "index.ts";
  }
