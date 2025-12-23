{
  description = "A Toy URL Shortener";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      crane,
      rust-overlay,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);
        craneCommonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
        };
        cargoArtifacts = craneLib.buildDepsOnly craneCommonArgs;
      in
      {
        formatter = pkgs.nixfmt-tree;

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rust-bin.stable.latest.complete
            devenv
          ];

          env = {
            RUST_BACKTRACE = "1";
          };
        };

        packages = {
          default = craneLib.buildPackage (
            craneCommonArgs
            // {
              inherit cargoArtifacts;
            }
          );

          docs = craneLib.cargoDoc (
            craneCommonArgs
            // {
              inherit cargoArtifacts;
              env.RUSTDOCFLAGS = "--deny warnings";
            }
          );

          dockerImageArm64 =
            let
              crossPkgs = import nixpkgs {
                crossSystem = "aarch64-linux";
                localSystem = system;
                overlays = [ (import rust-overlay) ];
              };
              crossCraneLib = (crane.mkLib crossPkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);

              crateInfo = crossCraneLib.crateNameFromCargoToml craneCommonArgs;
              appName = crateInfo.pname;
              appVersion = crateInfo.version;

              # NOTE: if we need to make this more complicated (like add dependencies), see:
              # https://github.com/ipetkov/crane/blob/master/examples/cross-rust-overlay/flake.nix
              builtPackage = crossCraneLib.buildPackage craneCommonArgs;
            in
            pkgs.dockerTools.buildImage {
              name = appName;
              tag = appVersion;
              architecture = "arm64";
              copyToRoot = pkgs.buildEnv {
                name = "image-root";
                paths = [ builtPackage ];
                pathsToLink = [ "/bin" ];
              };
              config = {
                Cmd = [ "/bin/${appName}" ];
              };
            };
        };

        checks = {
          format = craneLib.cargoFmt craneCommonArgs;

          lint = craneLib.cargoClippy (
            craneCommonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          test = craneLib.cargoTest (
            craneCommonArgs
            // {
              inherit cargoArtifacts;
            }
          );

          docs = self.packages.${system}.docs;

          e2e-test =
            pkgs.runCommand "stoopid-short-e2e-test"
              {
                nativeBuildInputs = with pkgs; [
                  self.packages.${system}.default
                  postgresql_18
                  curl
                  retry
                ];
              }
              ''
                export FAKETIME_TIMESTAMP_FILE="$(mktemp)"
                export FAKETIME_NO_CACHE=1

                export DYLD_FORCE_FLAT_NAMESPACE=1
                export DYLD_INSERT_LIBRARIES="${pkgs.libfaketime}/lib/faketime/libfaketime.1.dylib"
                export LD_PRELOAD="${pkgs.libfaketime}/lib/libfaketimeMT.so.1"

                bash ${./tests/e2e.sh}
                touch $out
              '';
        };
      }
    );
}
