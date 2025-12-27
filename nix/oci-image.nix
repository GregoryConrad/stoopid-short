# NOTE: once https://github.com/NixOS/nixpkgs/pull/390624 lands, we can switch to that.
{
  pkgs,
  package,
  architecture,
  binaryName,
  appName ? binaryName,
  appVersion,
}:
let
  dockerImage = pkgs.dockerTools.buildImage {
    name = appName;
    tag = appVersion;
    inherit architecture;
    copyToRoot = pkgs.buildEnv {
      name = "image-root";
      paths = [ package ];
      pathsToLink = [ "/bin" ];
    };
    config = {
      Cmd = [ "/bin/${binaryName}" ];
    };
  };
in
pkgs.runCommand appName
  {
    nativeBuildInputs = with pkgs; [
      skopeo
    ];
  }
  ''
    skopeo copy docker-archive:${dockerImage} oci-archive:$out --insecure-policy --tmpdir .
  ''
