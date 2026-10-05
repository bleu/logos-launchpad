{
  description = "Launchpad admin, configured-sales and shared core packages";

  inputs.logos-module-builder.url = "github:logos-co/logos-module-builder/4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1";

  outputs = inputs@{ logos-module-builder, ... }:
    let
      inherit (logos-module-builder.inputs.nixpkgs) lib;
      coreSource = builtins.path {
        path = ./core;
        name = "launchpad-core-source";
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
          stagedCore = pkgs.runCommand "launchpad-core" {} ''
            mkdir -p $out
            cp -r ${coreSource}/. $out/
            chmod -R u+w $out
            cp -r ${coreSource}/logic $out/rust-lib/logic
            substituteInPlace $out/rust-lib/logic/Cargo.toml \
              --replace-fail '[workspace]' ""
            substituteInPlace $out/rust-lib/Cargo.toml \
              --replace-fail 'path = "../logic"' 'path = "logic"'
          '';
          core = builder.lib.mkLogosModule {
            src = stagedCore;
            configFile = ./core/metadata.json;
            flakeInputs = inputs;
          };
          mkApp = src: logos-module-builder.lib.mkLogosQmlModule {
            inherit src;
            configFile = src + "/metadata.json";
            flakeInputs = inputs // { launchpad_core = core; };
          };
          admin = mkApp ./apps/admin;
          configuredSales = mkApp ./apps/configured-sales;
        in {
          default = pkgs.symlinkJoin {
            name = "launchpad-packages";
            paths = [
              core.packages.${system}.lgx-portable
              admin.packages.${system}.lgx-portable
              configuredSales.packages.${system}.lgx-portable
            ];
          };
          core-lgx = core.packages.${system}.lgx-portable;
          admin-lgx = admin.packages.${system}.lgx-portable;
          configured-sales-lgx = configuredSales.packages.${system}.lgx-portable;
          core-lidl = core.packages.${system}.lidl;
          core-generate = core.packages.${system}.generate;
          rust-sdk-src = logos-module-builder.packages.${system}.rust-sdk-src;
        });
    };
}
