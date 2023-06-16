{
  base,
  cargo-contract,
  pkgs,
}: let
  script = pkgs.writeShellScript "builder" ''
    set -e

    rustup toolchain install $RUST_VERSION \
      --profile minimal \
      --component rust-src

    if [ "$CARGO_CONTRACT_VERSION" = "${cargo-contract.version}" ]; then
      mkdir -p /root/.cargo/bin
      ln -s ${pkgs.lib.getExe cargo-contract.package} /root/.cargo/bin/cargo-contract
    else
      CARGO_TARGET_DIR=/root/cargo-contract cargo install cargo-contract \
        --git https://github.com/paritytech/cargo-contract \
        --tag v"$CARGO_CONTRACT_VERSION"

      rm -rf /root/cargo-contract
    fi

    mkdir source

    curl "$SOURCE_CODE_URL" \
      -o source.zip

    unzip source.zip \
      -d source

    cd source

    shopt -s globstar
    for i in **/*.rs; do
      curl -f "$API_SERVER_URL"/files/upload/"$BUILD_SESSION_TOKEN" \
        -F "$i"="@$i"
    done

    curl -f "$API_SERVER_URL"/files/seal/"$BUILD_SESSION_TOKEN" \
      -X POST

    CARGO_TARGET_DIR=/root/artifacts cargo contract build \
      --release &> >(${pkgs.lib.getExe pkgs.ansifilter})

    mv /root/artifacts/ink/*.wasm /root/artifacts/ink/main.wasm
    mv /root/artifacts/ink/*.json /root/artifacts/ink/main.json
  '';
in
  pkgs.dockerTools.buildImage {
    name = "ink-builder";
    tag = "latest";

    fromImage = base;

    copyToRoot = pkgs.buildEnv {
      name = "image-root";
      pathsToLink = ["/bin" "/lib"];
      paths = with pkgs; [
        rustup
        gcc
        curl
        unzip
        git
        binaryen
      ];
    };

    config = {
      Env = [
        "CARGO_CONTRACT_VERSION"
        "RUST_VERSION"
        "SOURCE_CODE_URL"
        "BUILD_SESSION_TOKEN"
        "API_SERVER_URL"
      ];

      Volumes = {
        "/root" = {};
      };

      WorkingDir = "/root";

      Cmd = ["${script}"];
    };
  }
