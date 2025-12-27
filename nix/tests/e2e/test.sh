# NOTE: this is intended to be run via Nix, under faketime, as a part of nix flake check

set -euo pipefail

echo "Starting E2E test"
export ADDR="127.0.0.1:8080"
export PGDATA=$(mktemp -d)
export DB_USER=$(whoami)
export DB_NAME="stoopid_test"
export DB_PORT=5432
export DB_URL="postgres://${DB_USER}@localhost:${DB_PORT}/${DB_NAME}"

cleanup() {
  echo "Cleaning up"
  if [[ -n "${SERVER_PID:-}" ]]; then
    echo "Stopping server"
    kill "$SERVER_PID" || true
  fi
  if pg_ctl status > /dev/null; then
    echo "Stopping PostgreSQL"
    pg_ctl stop
  fi
}
trap cleanup EXIT

check_get() {
  local url="http://$ADDR/$1"
  local expected_status="$2"
  local expected_location="$3"

  echo "GET $url should return $expected_status $expected_location"

  local status location
  read -r status location <<< "$(curl -sS -o /dev/null -w "%{http_code} %{redirect_url}" "$url")"

  if [[ "$status" != "$expected_status" ]]; then
    echo "expected HTTP $expected_status, got $status"
    return 1
  fi

  if [[ "$location" != "$expected_location" ]]; then
    echo "expected redirect to $expected_location, got $location"
    return 1
  fi
}

check_put() {
  local url="http://$ADDR/$1"
  local expected_status="$2"
  local body="$3"

  echo "PUT $url should return $expected_status for $body"

  local status
  status="$(curl -sS -o /dev/null -w "%{http_code}" \
    -X PUT \
    -H "Content-Type: application/json" \
    -d "$body" \
    "$url" \
  )"

  if [[ "$status" != "$expected_status" ]]; then
    echo "expected HTTP $expected_status, got $status"
    return 1
  fi
}

set_faketime() {
  echo "Setting faketime to $1"
  echo "$1" > $FAKETIME_TIMESTAMP_FILE
}

set_faketime "2000-01-01 00:00:00"

echo "Initializing PostgreSQL in $PGDATA"
initdb --no-locale -E UTF8
pg_ctl start -l "logfile" --options "-k $PWD" 

echo "Waiting for postgres"
retry --until=success --delay=1 --times=5 -- pg_isready -h localhost -p "$DB_PORT"

echo "Creating database $DB_NAME"
createdb -h localhost -p "$DB_PORT" -U "$DB_USER" "$DB_NAME"
psql -h localhost -p "$DB_PORT" -U "$DB_USER" "$DB_NAME" <<SQL
CREATE TABLE IF NOT EXISTS urls (
  id TEXT PRIMARY KEY NOT NULL,
  long_url TEXT NOT NULL,
  expiration_time_seconds BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_urls_expiration_time_seconds
  ON urls (expiration_time_seconds);
SQL

echo "Starting server"
stoopid-short-server &
SERVER_PID=$!

echo "Waiting for server to start on $ADDR"
retry --until=success --delay=1 --times=5 -- curl -s "$ADDR"

echo "Running test cases"

TEST_ID="validid"
check_get $TEST_ID 404 ""
check_put $TEST_ID 201 '{"url":"https://example.com/", "expiration_timestamp":"2000-01-01T00:00:10Z"}'
check_put $TEST_ID 200 '{"url":"https://example.com/", "expiration_timestamp":"2000-01-01T00:00:10Z"}'
check_put $TEST_ID 409 '{"url":"https://example.com/", "expiration_timestamp":"2000-01-01T00:00:11Z"}'
check_get $TEST_ID 307 "https://example.com/"
set_faketime "2000-01-01 00:00:11"
check_get $TEST_ID 404 ""
check_put $TEST_ID 201 '{"url":"https://example.com/new-url", "expiration_timestamp":"2001-01-01T00:00:00Z"}'
check_get $TEST_ID 307 "https://example.com/new-url"
set_faketime "2002-01-01 00:00:00"
check_get $TEST_ID 404 ""

echo "E2E test successful"
