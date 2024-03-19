{ rustPlatform, lib, clang, pkg-config, openssl }:
let cargo-toml = (builtins.fromTOML (builtins.readFile ./Cargo.toml)); in rustPlatform.buildRustPackage rec {
  pname = cargo-toml.package.name;
  version = cargo-toml.package.version;

  src = ./.;

  cargoLock = { lockFile = ./Cargo.lock; };

  nativeBuildInputs = [
    pkg-config
    clang
  ];

  buildInputs = [
    openssl
  ];
}
