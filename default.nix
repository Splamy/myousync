{pkgs ? import <nixpkgs> {}}: let
  manifest = (pkgs.lib.importTOML ./myousync/Cargo.toml).package;
in
  with pkgs;
    rustPlatform.buildRustPackage rec {
      pname = manifest.name;
      version = manifest.version;
      cargoLock.lockFile = ./Cargo.lock;
      src = pkgs.lib.cleanSource ./.;
      meta = {
        mainProgram = "myousync";
      };

      buildInputs = [
        openssl
      ];
      nativeBuildInputs = [
        pkg-config
      ];
    }
