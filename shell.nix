{pkgs ? import <nixpkgs> {}}:
pkgs.mkShell {
  # Get dependencies from the main package
  inputsFrom = [(pkgs.callPackage ./default.nix {})];
  # Additional tooling
  buildInputs = with pkgs; [
    rustc
    rust-analyzer # LSP Server
    rustfmt # Formatter
    clippy # Linter

    bun # Frontend
    bun2nix.packages.${system}.default
  ];
}
