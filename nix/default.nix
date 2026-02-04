{
  cargo,
  lib,
  pkg-config,
  rustc,
  craneLib,
  ...
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ../Cargo.toml);
in
  craneLib.buildPackage {
    pname = cargoToml.package.name;
    inherit (cargoToml.package) version;

    src = craneLib.cleanCargoSource ../.;

    buildInputs = [
      pkg-config
    ];

    nativeBuildInputs = [
      pkg-config
      cargo
      rustc
    ];

    meta = with lib; {
      description = "Lazygit for Azure Devops";
      homepage = "https://github.com/GappelSolutions/lazyops";
      changelog = "https://github.com/GappelSolutions/lazyops/releases/tag/${version}";
      maintainers = with maintainers; [dashietm];
      mainProgram = "lazyops";
    };
  }
