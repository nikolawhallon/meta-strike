{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  packages = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy

    # Optional extras:
    # rust-analyzer
    pkg-config
    openssl
  ];

  # Optional: useful defaults
  RUST_BACKTRACE = "1";
}
