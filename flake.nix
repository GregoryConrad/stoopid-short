{
  description = "A Toy URL Shortener";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      fenix,
      naersk,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        appName = "stoopid-short";
        pkgs = nixpkgs.legacyPackages.${system};
        rust = fenix.packages.${system};
      in
      {
        formatter = pkgs.nixfmt-tree;

        devShells.default = pkgs.mkShell {
          packages = [
            rust.stable.toolchain
          ];

          env = {
            RUST_BACKTRACE = "1";
          };
        };

        packages = {
          default =
            let
              toolchain = rust.stable.toolchain;
              naersk' = pkgs.callPackage naersk {
                cargo = toolchain;
                rustc = toolchain;
              };
            in
            naersk'.buildPackage {
              src = ./.;
            };

          crossArm64 =
            let
              target = "aarch64-unknown-linux-gnu";
              toolchain =
                with rust;
                combine [
                  stable.cargo
                  stable.rustc
                  targets.${target}.stable.rust-std
                ];
              naersk' = pkgs.callPackage naersk {
                cargo = toolchain;
                rustc = toolchain;
              };
              inherit (pkgs.pkgsCross.aarch64-multiplatform.stdenv) cc;
              linker = "${cc}/bin/${cc.targetPrefix}cc";
            in
            naersk'.buildPackage {
              src = ./.;
              CARGO_BUILD_TARGET = target;
              CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = linker;
            };

          imageArm64 = pkgs.dockerTools.buildImage {
            name = appName;
            tag = "latest";
            architecture = "arm64";
            copyToRoot = pkgs.buildEnv {
              name = "image-root";
              paths = [ self.packages.${system}.crossArm64 ];
              pathsToLink = [ "/bin" ];
            };
            config = {
              Cmd = [ "/bin/${appName}" ];
            };
          };
        };
      }
    );
}
