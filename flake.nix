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
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./crates/json-diff/Cargo.toml; })
            pname
            version
            ;

          # The React + Monaco frontend, built from package-lock.json.
          frontend = pkgs.buildNpmPackage {
            pname = "${pname}-ui";
            inherit version;
            src = lib.fileset.toSource {
              root = ./ui;
              fileset = lib.fileset.unions [
                ./ui/package.json
                ./ui/package-lock.json
                ./ui/index.html
                ./ui/tsconfig.json
                ./ui/vite.config.ts
                ./ui/src
              ];
            };
            npmDeps = pkgs.importNpmLock { npmRoot = ./ui; };
            npmConfigHook = pkgs.importNpmLock.npmConfigHook;
            nodejs = pkgs.nodejs_22;
            doCheck = true;
            checkPhase = ''
              runHook preCheck
              npm test
              runHook postCheck
            '';
            installPhase = ''
              runHook preInstall
              cp -r dist $out
              runHook postInstall
            '';
          };

          # Tauri on Linux: GTK for the window and dialogs, WebKitGTK for the
          # web view.
          linuxLibs = with pkgs; [
            webkitgtk_4_1
            gtk3
            libsoup_3
            glib
            librsvg
          ];

          commonArgs = {
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                (craneLib.fileset.commonCargoSources ./.)
                ./LICENSE
                ./THIRD_PARTY_NOTICES.txt
                ./crates/json-diff/tauri.conf.json
                ./crates/json-diff/capabilities
                ./crates/json-diff/icons
              ];
            };
            strictDeps = true;
            inherit pname version;
            nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.pkg-config ];
            buildInputs = lib.optionals pkgs.stdenv.isLinux linuxLibs;
          };

          # Dependencies are built once and shared by the checks; the package
          # adds `custom-protocol` (bundled frontend), so it has its own.
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          releaseArgs = commonArgs // {
            cargoExtraArgs = "--locked -p json-diff --features custom-protocol";
          };

          json-diff = craneLib.buildPackage (
            releaseArgs
            // {
              cargoArtifacts = craneLib.buildDepsOnly releaseArgs;
              # Tests run in the `tests` check instead.
              doCheck = false;
              # Tauri embeds the frontend from `frontendDist` at compile time.
              preBuild = ''
                mkdir -p ui
                cp -r ${frontend} ui/dist
              '';
              nativeBuildInputs =
                commonArgs.nativeBuildInputs
                ++ lib.optionals pkgs.stdenv.isLinux [
                  pkgs.wrapGAppsHook3
                  pkgs.copyDesktopItems
                ];
              # An entry in the desktop's application launcher.
              desktopItems = lib.optionals pkgs.stdenv.isLinux [
                (pkgs.makeDesktopItem {
                  name = "json-diff";
                  desktopName = "json-diff";
                  comment = "Compare JSON documents and folders side by side";
                  exec = "json-diff %F";
                  icon = "json-diff";
                  categories = [
                    "Development"
                    "Utility"
                  ];
                  mimeTypes = [ "application/json" ];
                })
              ];
              postInstall = lib.optionalString pkgs.stdenv.isLinux ''
                for size in 32 128; do
                  install -Dm644 crates/json-diff/icons/''${size}x''${size}.png \
                    $out/share/icons/hicolor/''${size}x''${size}/apps/json-diff.png
                done
                install -Dm644 crates/json-diff/icons/icon.png \
                  $out/share/icons/hicolor/512x512/apps/json-diff.png
                install -Dm644 -t $out/share/doc/json-diff LICENSE THIRD_PARTY_NOTICES.txt
              '';
              meta = {
                description = "Side-by-side JSON diff with folder comparison and an AI assistant";
                homepage = "https://github.com/ultra-infinity/json-diffrs";
                license = lib.licenses.gpl3Plus;
                mainProgram = "json-diff";
              };
            }
          );

          # Windows, cross-compiled with MinGW. The C runtime is linked
          # statically (see .cargo/config.toml), but on MinGW Tauri's WebView2
          # loader is a DLL, so `bin/` holds json-diff.exe and
          # WebView2Loader.dll, and the MSI installs both. Linux hosts only.
          mingw = pkgs.pkgsCross.mingwW64.stdenv.cc;
          # aws-lc (rig's TLS) includes <sched.h>, which MinGW ships in
          # winpthreads; its static library keeps that out of the DLL list.
          winpthreads = pkgs.pkgsCross.mingwW64.windows.pthreads;
          windowsArgs = {
            inherit (commonArgs) src strictDeps pname;
            inherit version;
            cargoExtraArgs = "--locked -p json-diff --features custom-protocol";
            CARGO_BUILD_TARGET = windowsTarget;
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${mingw}/bin/${mingw.targetPrefix}cc";
            # Replaces the rustflags in .cargo/config.toml, so repeats them.
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-C target-feature=+crt-static -L native=${winpthreads}/lib";
            # C dependencies (aws-lc) are compiled for Windows too.
            TARGET_CC = "${mingw}/bin/${mingw.targetPrefix}cc";
            TARGET_AR = "${mingw}/bin/${mingw.targetPrefix}ar";
            CFLAGS_x86_64_pc_windows_gnu = "-I${winpthreads}/include";
            # windres, for the exe's icon and version resource.
            depsBuildBuild = [ mingw ];
            nativeBuildInputs = [ pkgs.nasm ];
            doCheck = false;
          };
          json-diff-windows = craneLib.buildPackage (
            windowsArgs
            // {
              cargoArtifacts = craneLib.buildDepsOnly windowsArgs;
              preBuild = ''
                mkdir -p ui
                cp -r ${frontend} ui/dist
              '';
              # The loader webview2-com-sys links against, copied from its
              # build output.
              postInstall = ''
                loader=$(find target/${windowsTarget}/release/build -path '*webview2-com-sys-*/out/x64/WebView2Loader.dll' | head -n 1)
                install -Dm644 "$loader" $out/bin/WebView2Loader.dll
              '';
            }
          );
          json-diff-msi =
            pkgs.runCommand "json-diff-${version}-msi"
              { nativeBuildInputs = [ pkgs.msitools ]; }
              ''
                # Store files are dated 1970, before the CAB format's 1980
                # epoch, which Windows would show as 2098. Use a fixed date.
                mkdir -p bin $out
                cp ${json-diff-windows}/bin/json-diff.exe ${json-diff-windows}/bin/WebView2Loader.dll bin/
                cp ${./LICENSE} bin/LICENSE.txt
                cp ${./THIRD_PARTY_NOTICES.txt} bin/THIRD_PARTY_NOTICES.txt
                touch -d 2000-01-01T00:00:00Z bin/*
                wixl --arch x64 \
                  -D Version=${version} \
                  -D CargoTargetBinDir=bin \
                  -o $out/json-diff-${version}-x86_64.msi \
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
            inherit frontend;
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
            inherit json-diff frontend;
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

          # `nix develop`: the toolchain (with std sources), Node for the
          # frontend, the Tauri CLI, and the GTK/WebKit libraries.
          devShells.default = craneLib.devShell {
            checks = self'.checks;
            packages = [
              pkgs.nodejs_22
              pkgs.cargo-tauri
              pkgs.cargo-about
            ];

            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
            # GTK's file dialog reads its settings schemas from here.
            shellHook = lib.optionalString pkgs.stdenv.isLinux ''
              export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:''${XDG_DATA_DIRS:-}
              export LC_ALL=C.UTF-8
            '';
          };
        };
    };
}
