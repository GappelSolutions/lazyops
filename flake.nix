{
  description = "Lazygit for Azure Devops";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    flake-parts,
    crane,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux"];

      perSystem = {
        self',
        pkgs,
        ...
      }: let
        craneLib = crane.mkLib pkgs;
      in {
        devShells.default = pkgs.mkShell {
          inputsFrom = builtins.attrValues self'.packages;
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rust-analyzer
            rustfmt
          ];
        };
        packages = rec {
          lazyops = pkgs.callPackage ./nix/default.nix {inherit inputs craneLib;};
          default = lazyops;
        };
      };
      flake = _: rec {
        nixosModules.home-manager = homeManagerModules.default;
        homeManagerModules = rec {
          lazyops = import ./nix/hm.nix inputs.self;
          default = lazyops;
        };
      };
    };
}
