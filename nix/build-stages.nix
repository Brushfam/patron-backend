{
  base,
  pkgs,
}: let
  mkStageImage = stage: script:
    pkgs.dockerTools.buildImage {
      name = "stage-${stage}";
      tag = "latest";

      fromImage = base;

      config = {
        Env = [
          "BUILD_SESSION_TOKEN"
          "SOURCE_CODE_URL"
          "API_SERVER_URL"
        ];

        WorkingDir = "/contract";

        Cmd = ["${pkgs.writeShellScript "stage-${stage}-script" script}"];
      };
    };
in {
  unarchive = mkStageImage "unarchive" (let
    inherit (pkgs) coreutils;

    curl = pkgs.lib.getExe pkgs.curl;
    unzip = pkgs.lib.getExe pkgs.unzip;
  in ''
    set -e

    dst=$(${coreutils}/bin/mktemp)

    ${curl} "$SOURCE_CODE_URL" \
      -o $dst

    ${unzip} $dst

    shopt -s globstar
    for i in **/*.rs; do
      ${curl} -f "$API_SERVER_URL"/files/upload/"$BUILD_SESSION_TOKEN" \
        -F "$i"="@$i"
    done

    ${curl} -f "$API_SERVER_URL"/files/seal/"$BUILD_SESSION_TOKEN" \
      -X POST
  '');

  move = mkStageImage "move" (let
    mkMove = extension: ''
      find /contract/target/ink \
        -maxdepth 2 \
        -type f \
        -name "*.${extension}" \
        -exec mv {} /contract/target/ink/main.${extension} \;
    '';
  in
    pkgs.lib.concatStringsSep "\n" (map mkMove ["wasm" "json"]));
}
