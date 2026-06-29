{
  description = "EDCB CtrlCmd Rust client library, CLI, and MCP server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/0.1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, ... }@inputs:

    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forEachSupportedSystem =
        f:
        inputs.nixpkgs.lib.genAttrs supportedSystems (
          system:
          f {
            inherit system;
            pkgs = import inputs.nixpkgs {
              inherit system;
              overlays = [
                inputs.self.overlays.default
              ];
            };
          }
        );
    in
    {
      overlays.default = final: prev: {
        rustToolchain =
          with inputs.fenix.packages.${prev.stdenv.hostPlatform.system};
          combine (
            with stable;
            [
              clippy
              rustc
              cargo
              rustfmt
              rust-src
            ]
          );

        edcb-tools = final.callPackage (
          {
            lib,
            makeRustPlatform,
            rustToolchain,
          }:

          let
            rustPlatform = makeRustPlatform {
              cargo = rustToolchain;
              rustc = rustToolchain;
            };
          in
          rustPlatform.buildRustPackage {
            pname = "edcb-tools";
            version = "0.1.0";
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.lock
                ./Cargo.toml
                ./src
              ];
            };
            cargoLock.lockFile = ./Cargo.lock;

            meta = {
              description = "EDCB CtrlCmd Rust client library, CLI, and MCP server";
              homepage = "https://github.com/yutakobayashidev/edcb-tools";
              license = lib.licenses.mit;
              mainProgram = "edcb";
            };
          }
        ) { };
      };

      packages = forEachSupportedSystem (
        { pkgs, ... }:
        {
          default = pkgs.edcb-tools;
          edcb-tools = pkgs.edcb-tools;
        }
      );

      apps = forEachSupportedSystem (
        { pkgs, ... }:
        let
          edcb = {
            type = "app";
            program = "${pkgs.edcb-tools}/bin/edcb";
          };
          edcb-mcp = {
            type = "app";
            program = "${pkgs.edcb-tools}/bin/edcb-mcp";
          };
        in
        {
          default = edcb;
          inherit edcb edcb-mcp;
        }
      );

      devShells = forEachSupportedSystem (
        { pkgs, system }:
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustToolchain
              openssl
              pkg-config
              cargo-deny
              cargo-edit
              cargo-watch
              rust-analyzer
              self.formatter.${system}
            ];

            env = {
              # Required by rust-analyzer
              RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
            };
          };
        }
      );

      formatter = forEachSupportedSystem (
        { pkgs, ... }:
        pkgs.writeShellApplication {
          name = "format-edcb-tools";
          runtimeInputs = [
            pkgs.nixfmt
          ];
          text = ''
            if [ "$#" -eq 0 ]; then
              nixfmt flake.nix
            else
              nixfmt "$@"
            fi
          '';
        }
      );
    };
}
