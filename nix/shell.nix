{pkgs ? import <nixpkgs> {}}:
pkgs.mkShell {
  # Get dependencies from the main package
  inputsFrom = [
    (pkgs.callPackage ./backend/. {})
    (pkgs.callPackage ./frontend/. {})
  ];
  # Additional tooling
  buildInputs = with pkgs; [
    rustc
    rust-analyzer # LSP Server
    rustfmt # Formatter
    clippy # Linter
  ];
}
