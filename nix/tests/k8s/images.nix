# NOTE: to get the imageDigest + hash, you can do the following:
# nix shell nixpkgs#nix-prefetch-docker
# nix-prefetch-docker --image-name nginx --image-tag latest --arch arm64 --os linux
{ pkgs }:
let
  arch = builtins.elemAt (pkgs.lib.splitString "-" pkgs.stdenv.hostPlatform.system) 0;
in
{
  aarch64 = [
    (pkgs.dockerTools.pullImage {
      imageName = "ghcr.io/cloudnative-pg/cloudnative-pg";
      imageDigest = "sha256:34198e85b6e6dd81471cb1c3ee222ca5231b685220e7ae38a634d35ed4826a40";
      hash = "sha256-FF0HkWVREeEeeEsCLEF9S5vc82maWwQswd/38gt+eBA=";
      finalImageTag = "1.28.0";
      arch = "arm64";
    })
    (pkgs.dockerTools.pullImage {
      imageName = "ghcr.io/cloudnative-pg/postgresql";
      imageDigest = "sha256:a0cce97009fafd8e626f9eefade0fb610a9e95747200c9faccecef53b42d7bbe";
      hash = "sha256-sqgyOEcI21kPN4akmknWnpnJZT7s+U8HxkX1tUuTHFo=";
      finalImageTag = "18";
      arch = "arm64";
    })
    (pkgs.dockerTools.pullImage {
      imageName = "nginx";
      imageDigest = "sha256:ca871a86d45a3ec6864dc45f014b11fe626145569ef0e74deaffc95a3b15b430";
      hash = "sha256-7J8mlzcOWyqencuuAiPzUWEU2FHecd27UNFPUS31FaM=";
      finalImageTag = "latest";
      arch = "arm64";
    })
  ];
  x86_64 = [
    (pkgs.dockerTools.pullImage {
      imageName = "ghcr.io/cloudnative-pg/cloudnative-pg";
      imageDigest = "sha256:34198e85b6e6dd81471cb1c3ee222ca5231b685220e7ae38a634d35ed4826a40";
      hash = "sha256-xgZUWm5QdDsyjQwHQ4DWH1bpGSYUM0z17XKjR9WErHc=";
      finalImageTag = "1.28.0";
      arch = "amd64";
    })
    (pkgs.dockerTools.pullImage {
      imageName = "ghcr.io/cloudnative-pg/postgresql";
      imageDigest = "sha256:a0cce97009fafd8e626f9eefade0fb610a9e95747200c9faccecef53b42d7bbe";
      hash = "sha256-dJpC8MRJjcixys+TeSqJUXkM7rZWWO0hngBISIRQN/8=";
      finalImageTag = "18";
      arch = "amd64";
    })
    (pkgs.dockerTools.pullImage {
      imageName = "nginx";
      imageDigest = "sha256:ca871a86d45a3ec6864dc45f014b11fe626145569ef0e74deaffc95a3b15b430";
      hash = "sha256-0KqSDVmK8SUURIDtT4zSeMCx4ErAAxg10v5No6WWH4M=";
      finalImageTag = "latest";
      arch = "amd64";
    })
    (pkgs.dockerTools.pullImage {
      imageName = "rancher/mirrored-pause";
      imageDigest = "sha256:74c4244427b7312c5b901fe0f67cbc53683d06f4f24c6faee65d4182bf0fa893";
      hash = "sha256-IbuPXoalV8gKCZGMteRzkeG65o4GCu3G+UX+lVLAo2I=";
      finalImageTag = "3.6";
      arch = "amd64";
    })
  ];
}
."${arch}"
