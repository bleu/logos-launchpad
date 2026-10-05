{ logos-module-builder, system }:
let
  lib = logos-module-builder.inputs.nixpkgs.lib;
  withLocale = drv: drv.overrideAttrs (_: { LC_ALL = "C"; });
  localeOverlay = final: prev: {
    stdenv = prev.stdenv // {
      mkDerivation = args:
        let
          drv = prev.stdenv.mkDerivation args;
          name = drv.pname or drv.name;
        in if lib.hasPrefix "logos-" name || lib.hasPrefix "launchpad_core" name
          then withLocale drv
          else drv;
    };
  };
  localeNix = logosNix: logosNix // {
    lib = logosNix.lib // {
      nativeOverlays = logosNix.lib.nativeOverlays ++ [ localeOverlay ];
      forAllTargets = f: logosNix.lib.forAllTargets (args:
        f (args // { pkgs = args.pkgs.extend localeOverlay; }));
    };
  };
  evaluate = flake: overrides:
    let
      inputs = flake.inputs
        // lib.optionalAttrs (flake.inputs ? logos-nix) {
          logos-nix = localeNix flake.inputs.logos-nix;
        }
        // overrides;
      adjusted = flake
        // (import (flake.outPath + "/flake.nix")).outputs (inputs // { self = adjusted; })
        // { inherit inputs; };
    in adjusted;
  generatorLocale = flake: name: flake // {
    packages = flake.packages // {
      ${system} = flake.packages.${system} // {
        ${name} = withLocale flake.packages.${system}.${name};
      };
    };
  };
  inputs = logos-module-builder.inputs;
in evaluate logos-module-builder {
  logos-module = evaluate inputs.logos-module {};
  logos-plugin-qt = evaluate inputs.logos-plugin-qt {};
  logos-qt-sdk = evaluate inputs.logos-qt-sdk {};
  logos-view-module = generatorLocale inputs.logos-view-module "logos-view-generator";
}
