{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, flake-utils, naersk, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) { inherit system; };
        naersk' = pkgs.callPackage naersk { };
      in {
        packages = rec {
          tcrelay = naersk'.buildPackage { src = ./.; };
          default = tcrelay;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [ rustc cargo clippy ];
        };
      });
}
