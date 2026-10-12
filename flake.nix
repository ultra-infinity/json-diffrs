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
          windowsTarget = "x86_64-pc-windows-gnu";
          # rust-src is the standard library's source: rust-analyzer needs it
          # to resolve and navigate `std`. The Windows target's std is for
          # the `windows` and `msi` packages.
          toolchain = pkgs.rust-bin.beta.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
            targets = [ windowsTarget ];
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
              nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux [
                pkgs.makeWrapper
                pkgs.copyDesktopItems
              ];
              # An entry in the desktop's application launcher.
              desktopItems = lib.optionals pkgs.stdenv.isLinux [
                (pkgs.makeDesktopItem {
                  name = "json-diff";
                  desktopName = "json-diff";
                  comment = "Compare JSON documents and folders side by side";
                  exec = "json-diff %F";
                  icon = "text-x-generic";
                  categories = [
                    "Development"
                    "Utility"
                  ];
                  mimeTypes = [ "application/json" ];
                })
              ];
              postInstall = lib.optionalString pkgs.stdenv.isLinux ''
                wrapProgram $out/bin/json-diff \
                  --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
              '';
              meta = {
                description = "Side-by-side JSON diff with folder comparison and an AI assistant";
                homepage = "https://github.com/ultra-infinity/json-diffrs";
                license = lib.licenses.gpl3Plus;
                mainProgram = "json-diff";
              };
            }
          );

          # Windows: cross-compiled with MinGW into one static json-diff.exe
          # (see .cargo/config.toml), then wrapped in an MSI by msitools'
          # wixl. Built from Linux hosts only.
          mingw = pkgs.pkgsCross.mingwW64.stdenv.cc;
          # aws-lc (rig's TLS) includes <sched.h>, which MinGW ships in
          # winpthreads; its static library keeps the exe free of DLLs.
          winpthreads = pkgs.pkgsCross.mingwW64.windows.pthreads;
          windowsArgs = commonArgs // {
            CARGO_BUILD_TARGET = windowsTarget;
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${mingw}/bin/${mingw.targetPrefix}cc";
            # Replaces the rustflags in .cargo/config.toml, so repeats them.
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-C target-feature=+crt-static -L native=${winpthreads}/lib";
            # C dependencies (aws-lc, oniguruma) are compiled for Windows too.
            TARGET_CC = "${mingw}/bin/${mingw.targetPrefix}cc";
            TARGET_AR = "${mingw}/bin/${mingw.targetPrefix}ar";
            CFLAGS_x86_64_pc_windows_gnu = "-I${winpthreads}/include";
            depsBuildBuild = [ mingw ];
            nativeBuildInputs = [ pkgs.nasm ];
            doCheck = false;
          };
          json-diff-windows = craneLib.buildPackage (
            windowsArgs
            // {
              cargoArtifacts = craneLib.buildDepsOnly windowsArgs;
              cargoExtraArgs = "--locked -p json-diff";
            }
          );
          json-diff-msi =
            pkgs.runCommand "json-diff-${commonArgs.version}-msi"
              { nativeBuildInputs = [ pkgs.msitools ]; }
              ''
                # Store files are dated 1970, before the CAB format's 1980
                # epoch, which Windows would show as 2098. Use a fixed date.
                mkdir -p bin $out
                cp ${json-diff-windows}/bin/json-diff.exe bin/
                touch -d 2000-01-01T00:00:00Z bin/json-diff.exe
                wixl --arch x64 \
                  -D Version=${commonArgs.version} \
                  -D CargoTargetBinDir=bin \
                  -o $out/json-diff-${commonArgs.version}-x86_64.msi \
                  ${./crates/json-diff/wix/main.wxs}
              '';
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };

          packages = {
            default = json-diff;
          }
          // lib.optionalAttrs pkgs.stdenv.isLinux {
            windows = json-diff-windows;
            msi = json-diff-msi;
          };

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
