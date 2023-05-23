{
  base,
  bins,
  pkgs,
}:
pkgs.dockerTools.buildImage {
  name = "api-server";
  tag = "latest";

  fromImage = base;

  config.Entrypoint = ["${bins}/bin/server"];
}
