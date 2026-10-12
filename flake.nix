{
  description = "json-diff: VS Code style side-by-side JSON diff with a rig-powered assistant";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        {
          system,
          pkgs,
          lib,
          self',
          ...
        }:
        let
          # rust-src is the standard library's source: rust-analyzer needs it
          # to resolve and navigate `std`.
          toolchain = pkgs.rust-bin.beta.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
          };
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;

          # winit and glow load these with dlopen at runtime.
          runtimeLibs = lib.optionals pkgs.stdenv.isLinux (
            with pkgs;
            [
              libX11
              libXcursor
              libXrandr
              libXi
              wayland
              libxkbcommon
              libGL
            ]
          );

          commonArgs = {
            src = craneLib.cleanCargoSource ./.;
            strictDeps = true;
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./crates/json-diff/Cargo.toml; }) pname version;
          };

          # Dependencies are built once and shared by the package and checks.
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          json-diff = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoExtraArgs = "--locked -p json-diff";
              # Tests run in the `tests` check instead.
              doCheck = false;
              nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.makeWrapper ];
              postInstall = lib.optionalString pkgs.stdenv.isLinux ''
                wrapProgram $out/bin/json-diff \
                  --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
              '';
              meta.mainProgram = "json-diff";
            }
          );
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };

          packages.default = json-diff;

          apps.default = {
            type = "app";
            program = lib.getExe json-diff;
          };

          checks = {
            inherit json-diff;
            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- --deny warnings";
              }
            );
            tests = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });
            fmt = craneLib.cargoFmt { inherit (commonArgs) src pname version; };
          };

          # `nix develop`: the toolchain (with std sources), the check inputs,
          # and the runtime libraries for `cargo run`.
          devShells.default = craneLib.devShell {
            checks = self'.checks;
            packages = runtimeLibs;

            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
            XKB_CONFIG_ROOT = "${pkgs.xkeyboard_config}/share/X11/xkb";
            LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
            shellHook = ''
              export LC_ALL=C.UTF-8
            '';
          };
        };
    };
}
