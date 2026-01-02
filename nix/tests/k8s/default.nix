{
  testers,
  serverImage,
  urlGcImage,
}:
testers.runNixOSTest {
  name = "k8s";

  defaults =
    { pkgs, config, ... }:
    {
      virtualisation.diskSize = 5 * 1024;
      virtualisation.memorySize = 5 * 1024;

      networking.firewall.allowedTCPPorts = [
        6443 # k3s: required so that pods can reach the API server (running on port 6443 by default)
        2379 # k3s, etcd clients: required if using a "High Availability Embedded etcd" configuration
        2380 # k3s, etcd peers: required if using a "High Availability Embedded etcd" configuration
      ];
      networking.firewall.allowedUDPPorts = [
        8472 # k3s, flannel: required if using multi-node for inter-node networking
      ];

      services.k3s = {
        enable = true;
        token = "super-secret-token";
        disable = [ "metrics-server" ];
        extraFlags = [
          # NOTE: we need to use eth1 since we are in an integration test, where:
          # - eth0 is reserved for the NixOS test driver
          # - eth1 is reserved for inter-node communication
          "--flannel-iface eth1"
        ];

        images = [
          serverImage
          urlGcImage
        ]
        ++ import ./images.nix { inherit pkgs; };
      };
    };

  nodes.node1 =
    { pkgs, nodes, ... }:
    {
      services.k3s = {
        clusterInit = true;
        nodeIP = nodes.node1.networking.primaryIPAddress;
        autoDeployCharts = {
          cloudnative-pg = {
            name = "cloudnative-pg";
            repo = "https://cloudnative-pg.github.io/charts";
            version = "0.27.0";
            hash = "sha256-ObGgzQzGuWT4VvuMgZzFiI8U+YX/JM868lZpZnrFBGw=";
            targetNamespace = "cnpg-system";
            createNamespace = true;
          };
          stoopid-short = {
            package = pkgs.callPackage ../../helm-chart.nix {
              chartDir = ../../../charts/stoopid-short;
            };
            targetNamespace = "stoopid-short";
            createNamespace = true;
            values = {
              nginx.imagePullPolicy = "Never"; # NOTE: we must use local image for air-gapped tests
              # NOTE: only 1 instance of each component to:
              # - limit resource usage
              # - make debugging (in case of failure) a little bit easier
              nginx.replicas = 1;
              server.replicas = 1;
              postgres.instances = 1;
            };
          };
        };
      };
    };

  nodes.node2 =
    { pkgs, nodes, ... }:
    {
      services.k3s = {
        nodeIP = nodes.node2.networking.primaryIPAddress;
        serverAddr = "https://${nodes.node1.services.k3s.nodeIP}:6443";
      };
    };

  # TODO dump k3s info with a easy tag every 30 seconds of all running pods + their description/logs

  testScript = ''
    port = 30080
    test_node = node2

    import json
    from datetime import datetime, timedelta

    node1.start()
    node2.start()
    test_node.wait_until_succeeds(f'curl -sf localhost:{port}/health')

    expected_url = 'https://example.com/'
    expiration_time = datetime.now() + timedelta(days=1)
    expiration_timestamp = f'{expiration_time.isoformat()}Z'

    post_body = json.dumps({
      'url': expected_url,
      'expiration_timestamp': expiration_timestamp
    })
    post_response = test_node.succeed(
      f'curl -sS -X POST -H "Content-Type: application/json" -d \'{post_body}\' localhost:{port}'
    )
    print(f'Received response {post_response}')
    short_url_id = json.loads(post_response)['shortened_url_id']

    actual_url = test_node.succeed(
      f'curl -sS -o /dev/null -w "%{{redirect_url}}" "localhost:{port}/{short_url_id}"'
    )
    assert actual_url == expected_url, f"{actual_url} != {expected_url}"
  '';
}
