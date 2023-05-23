{
  pkgs,
  system,
}: let
  images = {
    x86_64-linux = {
      imageDigest = "sha256:c0669ef34cdc14332c0f1ab0c2c01acb91d96014b172f1a76f3a39e63d1f0bda";
      sha256 = "sha256-kqOws3lmYkjb/ZkBc31ILH41vHMNzANl3IJEpT9D7VY=";
    };
  };

  systemImage = images.${system};
in
  pkgs.dockerTools.pullImage {
    inherit (systemImage) imageDigest sha256;
    imageName = "alpine";
    finalImageName = "alpine";
    finalImageTag = "latest";
  }
