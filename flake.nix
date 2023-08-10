{
  inputs = {
    nixpkgs = {
      type = "github";
      owner = "nixos";
      repo = "nixpkgs";
      ref = "nixos-unstable";
    };

    crane = {
      type = "github";
      owner = "ipetkov";
      repo = "crane";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        rust-overlay.follows = "rust-overlay";
        flake-utils.follows = "flake-utils";
      };
    };

    rust-overlay = {
      type = "github";
      owner = "oxalica";
      repo = "rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    nix-filter = {
      type = "github";
      owner = "numtide";
      repo = "nix-filter";
    };

    flake-utils = {
      type = "github";
      owner = "numtide";
      repo = "flake-utils";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
    nix-filter,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;

          overlays = [(import rust-overlay)];
        };

        src = nix-filter.lib.filter {
          root = ./.;

          include = [
            nix-filter.lib.isDirectory

            # Mirror https://github.com/ipetkov/crane/blob/master/lib/filterCargoSources.nix
            (nix-filter.lib.matchExt "rs")
            (nix-filter.lib.matchExt "toml")
            ./Cargo.lock

            # Preserve generated .scale files
            (nix-filter.lib.matchExt "scale")
          ];
        };

        rustToolchain = pkgs.rust-bin.nightly."2023-05-22".default.override {
          extensions = [
            "rustc"
            "cargo"
            "clippy"
            "rustfmt"
            "rust-src"
          ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          inherit src;

          buildInputs = with pkgs; [
            openssl
            e2fsprogs
            util-linux
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly (commonArgs
          // {
            pname = "cargo-deps";
          });

        base = import ./nix/base.nix {inherit pkgs system;};
      in {
        devShell = pkgs.mkShell {
          packages = [rustToolchain];
          inputsFrom = [commonArgs];
        };

        apps.loadDockerImages = flake-utils.lib.mkApp {
          drv = pkgs.writeShellScriptBin "load-docker-images" (let
            mkStages = stages:
              pkgs.lib.concatMapStringsSep
              "\n" (stage: "docker load < ${self.packages.${system}.docker.build-stages.${stage}}")
              stages;
          in
            mkStages (builtins.attrNames self.packages.${system}.docker.build-stages));
        };

        packages = {
          default = craneLib.buildPackage (commonArgs
            // {
              inherit cargoArtifacts;

              pname = "bins";
            });

          docker = {
            build-stages = import ./nix/build-stages.nix {inherit base pkgs;};
            server = import ./nix/server.nix {
              inherit base pkgs;
              bins = self.packages.${system}.default;
            };
          };
        };

        formatter = pkgs.alejandra;
      }
    );
}
