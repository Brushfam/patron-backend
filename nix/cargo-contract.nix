{
  craneLib,
  pkgs,
  sha256,
  version,
}: {
  inherit version;

  package = craneLib.buildPackage {
    inherit version;

    pname = "cargo-contract";

    src = pkgs.fetchFromGitHub {
      inherit sha256;

      owner = "paritytech";
      repo = "cargo-contract";
      rev = "v${version}";
    };

    nativeBuildInputs = with pkgs; [
      cmake
    ];

    doCheck = false;
  };
}
