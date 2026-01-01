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
        craneCommonArgsWithDepCache = craneCommonArgs // {
          cargoArtifacts = craneLib.buildDepsOnly craneCommonArgs;
        };

        arch = builtins.elemAt (pkgs.lib.splitString "-" system) 0;
        linuxPkgs = import nixpkgs {
          crossSystem = "${arch}-linux";
          localSystem = system;
          overlays = [ (import rust-overlay) ];
        };
        linuxPackageForImage = linuxPkgs.callPackage ./nix/package.nix {
          craneLib = (crane.mkLib linuxPkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);
          craneArgs = craneCommonArgs;
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
          urlGc = {
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

          serverImage = pkgs.callPackage ./nix/container-image.nix {
            binaryName = "stoopid-short-server";
            package = linuxPackageForImage;
            appVersion = (craneLib.crateNameFromCargoToml craneCommonArgs).version;
          };
          urlGcImage = pkgs.callPackage ./nix/container-image.nix {
            binaryName = "url-gc";
            package = linuxPackageForImage;
            appVersion = (craneLib.crateNameFromCargoToml craneCommonArgs).version;
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

          e2e = pkgs.callPackage ./nix/tests/e2e {
            package = self.packages.${system}.default;
          };

          k8s = pkgs.callPackage ./nix/tests/k8s {
            serverImage = self.packages.${system}.serverImage;
            urlGcImage = self.packages.${system}.urlGcImage;
          };
        };
      }
    );
}
