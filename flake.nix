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
        linuxArm64Pkgs = import nixpkgs {
          crossSystem = "aarch64-linux";
          localSystem = system;
          overlays = [ (import rust-overlay) ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);
        craneCommonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
        };
        craneCommonArgsWithDepCache = craneCommonArgs // {
          cargoArtifacts = craneLib.buildDepsOnly craneCommonArgs;
        };
      in
      {
        formatter = pkgs.nixfmt-tree;

        apps = rec {
          default = server;
          server = {
            type = "app";
            program = "${self.packages.${system}.default}/bin/stoopid-short-server";
            meta.description = "The stoopid short web server";
          };
          url-gc = {
            type = "app";
            program = "${self.packages.${system}.default}/bin/url-gc";
            meta.description = "The stoopid short expired URL garbage collection";
          };
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rust-bin.stable.latest.complete
            devenv
            kubernetes-helm
            helm-ls
          ];

          env = {
            RUST_BACKTRACE = "1";
          };
        };

        packages = {
          default = pkgs.callPackage ./nix/package.nix {
            inherit craneLib;
            craneArgs = craneCommonArgsWithDepCache;
          };

          docs = craneLib.cargoDoc (
            craneCommonArgsWithDepCache
            // {
              env.RUSTDOCFLAGS = "--deny warnings";
            }
          );

          ociArm64Server = pkgs.callPackage ./nix/oci-image.nix {
            architecture = "arm64";
            binaryName = "stoopid-short-server";
            appVersion = (craneLib.crateNameFromCargoToml craneCommonArgs).version;
            package = linuxArm64Pkgs.callPackage ./nix/package.nix {
              craneLib = (crane.mkLib linuxArm64Pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);
              craneArgs = craneCommonArgs;
            };
          };
          ociArm64UrlGc = pkgs.callPackage ./nix/oci-image.nix {
            architecture = "arm64";
            binaryName = "url-gc";
            appVersion = (craneLib.crateNameFromCargoToml craneCommonArgs).version;
            package = linuxArm64Pkgs.callPackage ./nix/package.nix {
              craneLib = (crane.mkLib linuxArm64Pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);
              craneArgs = craneCommonArgs;
            };
          };
        };

        checks = {
          format = craneLib.cargoFmt craneCommonArgs;

          lint = craneLib.cargoClippy (
            craneCommonArgsWithDepCache
            // {
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          docs = self.packages.${system}.docs;

          test = craneLib.cargoTest craneCommonArgsWithDepCache;

          e2e-test = pkgs.callPackage ./nix/tests/e2e {
            package = self.packages.${system}.default;
          };
        };
      }
    );
}
