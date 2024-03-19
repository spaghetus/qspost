#!/usr/bin/env bash
cargo clean
nix-build -E '(import
 ((import <nixpkgs> {}).fetchFromGitHub {
  owner = "nixos";
  repo = "nixpkgs";
  rev = "c75037bbf9093a2acb617804ee46320d6d1fea5a";
  hash = "sha256-rL5LSYd85kplL5othxK5lmAtjyMOBg390sGBTb3LRMM=";
 }) {}).callPackage ./. {}'
