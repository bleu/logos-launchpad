{
  description = "Basecamp QML and Rust status packaging check";

  inputs.logos-module-builder.url = "github:logos-co/logos-module-builder/4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1";

  outputs = inputs@{ logos-module-builder, ... }:
    let
      inherit (logos-module-builder.inputs.nixpkgs) lib;
      backendSource = builtins.path {
        path = ./backend;
        name = "launchpad-status-backend-source";
        filter = path: type: !(builtins.elem (builtins.baseNameOf path) [
          "target" "generated" "logos-rust-sdk-src"
        ]);
      };
    in {
      packages = lib.genAttrs [ "aarch64-darwin" ] (system:
        let
          pkgs = logos-module-builder.inputs.nixpkgs.legacyPackages.${system};
          # Avoid the pinned Bash locale crash in macOS build hooks.
          builder = import ./build-locale.nix { inherit logos-module-builder system; };
          # The builder copies only rust-lib. Stage its local dependency inside it.
          stagedBackend = pkgs.runCommand "launchpad-status-backend" {} ''
            mkdir -p $out
            cp -r ${backendSource}/. $out/
            chmod -R u+w $out
            cp -r ${backendSource}/status $out/rust-lib/status
            substituteInPlace $out/rust-lib/status/Cargo.toml \
              --replace-fail '[workspace]' ""
            substituteInPlace $out/rust-lib/Cargo.toml \
              --replace-fail 'path = "../status"' 'path = "status"'
          '';
          backend = builder.lib.mkLogosModule {
            src = stagedBackend;
            configFile = ./backend/metadata.json;
            flakeInputs = inputs;
          };
          app = logos-module-builder.lib.mkLogosQmlModule {
            src = ./app;
            configFile = ./app/metadata.json;
            flakeInputs = inputs // { launchpad_status_core = backend; };
          };
        in {
          backend-lgx = backend.packages.${system}.lgx-portable;
          app-lgx = app.packages.${system}.lgx-portable;
          backend-lidl = backend.packages.${system}.lidl;
          backend-generate = backend.packages.${system}.generate;
          rust-sdk-src = logos-module-builder.packages.${system}.rust-sdk-src;
        });
    };
}
