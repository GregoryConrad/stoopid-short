{ pkgs, ... }:
{
  cachix.enable = false;

  env.DB_URL = "postgres://user:password@localhost:5432/app";
  services.postgres = {
    enable = true;
    package = pkgs.postgresql_18;

    port = 5432;
    listen_addresses = "localhost";

    initialDatabases = [
      {
        name = "app";
        user = "user";
        pass = "password";

        initialSQL = ''
          CREATE TABLE IF NOT EXISTS urls (
            id TEXT PRIMARY KEY NOT NULL,
            long_url TEXT NOT NULL,
            expiration_time_seconds BIGINT NOT NULL
          );

          CREATE INDEX IF NOT EXISTS idx_urls_expiration_time_seconds
            ON urls (expiration_time_seconds);

          GRANT SELECT, INSERT, UPDATE, DELETE
            ON urls
            TO "user";
        '';
      }
    ];
  };
}
