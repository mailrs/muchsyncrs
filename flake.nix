{
  description = "The muchsyncrs project";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-24.11";
    unstable-nixpkgs.url = "nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs:
    inputs.flake-utils.lib.eachSystem [ "x86_64-linux" ] (
      system:
      let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            (_: _: { } // inputs.self.packages."${system}")
            (import inputs.rust-overlay)
          ];
        };

        nightlyRustTarget = pkgs.rust-bin.selectLatestNightlyWith (
          toolchain:
          pkgs.rust-bin.fromRustupToolchain {
            channel = "nightly-2024-12-16";
            components = [ "rustfmt" ];
          }
        );

        nightlyCraneLib = (inputs.crane.mkLib pkgs).overrideToolchain nightlyRustTarget;

        rustTarget = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustTarget;

        tomlInfo = craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; };
        inherit (tomlInfo) version;

        pname = "muchsyncrs";

        src =
          let
            nixFilter = path: _type: !pkgs.lib.hasSuffix ".nix" path;
            extraFiles =
              path: _type:
              !(builtins.any (n: pkgs.lib.hasSuffix n path) [
                ".github"
                ".sh"
              ]);
            filterPath =
              path: type:
              builtins.all (f: f path type) [
                nixFilter
                extraFiles
                pkgs.lib.cleanSourceFilter
              ];
          in
          pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = filterPath;
          };

        buildInputs = [
        ];

        nativeBuildInputs = [
          pkgs.notmuch
        ];

        cargoArtifacts = craneLib.buildDepsOnly {
          inherit src pname buildInputs;
        };

        muchsyncrs = craneLib.buildPackage {
          inherit
            cargoArtifacts
            src
            pname
            version
            buildInputs
            nativeBuildInputs
            ;
          cargoExtraArgs = "--all-features -p muchsyncrs";
        };

        muchsyncrs-doc = craneLib.cargoDoc {
          inherit
            cargoArtifacts
            src
            pname
            version
            buildInputs
            ;
          cargoExtraArgs = "--document-private-items -p muchsyncrs";
        };

        rustfmt' = pkgs.writeShellScriptBin "rustfmt" ''
          exec "${nightlyRustTarget}/bin/rustfmt" "$@"
        '';

        customCargoMultiplexer = pkgs.writeShellScriptBin "cargo" ''
          case "$1" in
            +nightly)
              shift
              export PATH="${nightlyRustTarget}/bin/:''$PATH"
              exec ${nightlyRustTarget}/bin/cargo "$@"
              ;;
            *)
              exec ${rustTarget}/bin/cargo "$@"
          esac
        '';

        treefmt = inputs.treefmt-nix.lib.evalModule pkgs ./nix/treefmt.nix;
      in
      rec {
        formatter = treefmt.config.build.wrapper;

        checks = {
          inherit muchsyncrs;

          muchsyncrs-clippy = craneLib.cargoClippy {
            inherit cargoArtifacts src pname;
            cargoClippyExtraArgs = "--benches --examples --tests --all-features -- --deny warnings";
          };

          muchsyncrs-clippy-no-features = craneLib.cargoClippy {
            inherit cargoArtifacts src pname;
            cargoClippyExtraArgs = "--benches --examples --tests --no-default-features -- --deny warnings";
          };

          muchsyncrs-fmt = nightlyCraneLib.cargoFmt {
            inherit src pname;
          };

          muchsyncrs-tests = craneLib.cargoNextest {
            inherit
              cargoArtifacts
              src
              pname
              buildInputs
              ;
            nativeBuildInputs = nativeBuildInputs ++ [
              pkgs.coreutils
            ];
          };

          formatting = treefmt.config.build.check inputs.self;
        };

        packages = {
          default = packages.muchsyncrs;
          inherit muchsyncrs muchsyncrs-doc;
        };

        devShells.default = pkgs.mkShell {
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.notmuch
          ];

          nativeBuildInputs = [
            customCargoMultiplexer
            rustfmt'
            rustTarget

            pkgs.notmuch
            pkgs.cargo-insta
            pkgs.cargo-deny
            pkgs.gitlint
            pkgs.statix
          ];
        };
      }
    );
}
