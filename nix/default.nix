{
  cargo,
  lib,
  pkg-config,
  rustc,
  openssl,
  craneLib,
  stdenv,
  apple-sdk_15 ? null,
  darwinMinVersionHook ? null,
  ...
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ../Cargo.toml);
in
  craneLib.buildPackage {
    pname = cargoToml.package.name;
    inherit (cargoToml.package) version;

    src = craneLib.cleanCargoSource ../.;

    nativeBuildInputs = [
      pkg-config
      cargo
      rustc
    ];

    buildInputs = [
      openssl
    ] ++ lib.optionals stdenv.hostPlatform.isDarwin [
      apple-sdk_15
      (darwinMinVersionHook "10.15")
    ];

    meta = with lib; {
      description = "Lazygit for Azure Devops";
      homepage = "https://github.com/GappelSolutions/lazyops";
      changelog = "https://github.com/GappelSolutions/lazyops/releases/tag/${version}";
      license = licenses.mit;
      maintainers = with maintainers; [dashietm];
      mainProgram = "lazyops";
    };
  }
