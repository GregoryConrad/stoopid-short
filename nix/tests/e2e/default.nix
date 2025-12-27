{
  pkgs,
  package,
}:
pkgs.runCommand "stoopid-short-e2e-test"
  {
    nativeBuildInputs = with pkgs; [
      package
      postgresql_18
      curl
      retry
      jq
    ];
  }
  ''
    export FAKETIME_TIMESTAMP_FILE="$(mktemp)"
    export FAKETIME_NO_CACHE=1

    export DYLD_FORCE_FLAT_NAMESPACE=1
    export DYLD_INSERT_LIBRARIES="${pkgs.libfaketime}/lib/faketime/libfaketime.1.dylib"
    export LD_PRELOAD="${pkgs.libfaketime}/lib/libfaketimeMT.so.1"

    bash ${./test.sh}
    touch $out
  ''
