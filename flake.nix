{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { flake-utils, naersk, nixpkgs, rust-overlay,... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
      overlays = [ (import rust-overlay) ];
        pkgs = (import nixpkgs) {
          inherit system overlays;
        };


        naersk' = pkgs.callPackage naersk {};

      in {
        # For `nix build` & `nix run`:
        packages.default = naersk'.buildPackage {
          src = ./.;
          nativeBuildInputs = with pkgs; [ rust-bin.beta.latest.default ];
        };

        # For `nix develop`:
        devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
            # X11 Dependencies
            libX11
            libXcursor
            libXrandr
            libXi
            
            # Wayland Dependencies
            wayland
            libxkbcommon
            
            # Graphics
            libGL
          ];

          XKB_CONFIG_ROOT = "${pkgs.xkeyboard_config}/share/X11/xkb";
          shellHook = ''
            export LC_ALL=C.UTF-8
          '';

          # This environment variable forces winit/eframe to know where the libraries are
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
            libX11
            libXcursor
            libXrandr
            libXi
            wayland
            libxkbcommon
            libGL
          ]);
          nativeBuildInputs = with pkgs; [ rust-bin.beta.latest.default rust-analyzer ];
        };
      }
     
    );
}
