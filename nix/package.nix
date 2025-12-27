{
  craneLib,
  craneArgs,
}:
craneLib.buildPackage craneArgs
// {
  # NOTE: if we need to add build/runtime dependencies, see:
  # https://github.com/ipetkov/crane/blob/7d8ec2c71771937ab99790b45e6d9b93d15d9379/examples/cross-rust-overlay/flake.nix#L65
}
