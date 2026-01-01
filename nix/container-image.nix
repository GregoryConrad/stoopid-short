{
  pkgs,
  package,
  binaryName,
  appName ? binaryName,
  appVersion,
}:
let
  dockerImage = pkgs.dockerTools.buildImage {
    name = appName;
    tag = appVersion;
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
dockerImage
# NOTE: once https://github.com/NixOS/nixpkgs/pull/390624 lands, we can try switching to that.
# If we need similar in the meantime, can use something like this:
# pkgs.runCommand appName
#   {
#     nativeBuildInputs = with pkgs; [
#       skopeo
#     ];
#   }
#   ''
#     skopeo copy docker-archive:${dockerImage} oci-archive:$out --insecure-policy --tmpdir .
#   ''
