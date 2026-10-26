"""Portable process acceptance for Lift SQL pools and HTTP notes.

python scripts/verify_notes.py [--skip-build] [--postgres]
--postgres also requires LIFT_TEST_POSTGRES_URL to a DEDICATED DISPOSABLE DB:
the notes fixture replaces/drops its notes table. No external Python packages.
"""
import argparse
import contextlib
import http.client
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = ROOT / "examples" / "notes-service"
DATABASE = ROOT / "examples" / "database"
SUFFIX = ".exe" if os.name == "nt" else ""


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def database_env(driver, url):
    return dict(os.environ, LIFT_DB_DRIVER=driver, LIFT_DB_URL=url,
                LIFT_DB_MIN_IDLE="1", LIFT_DB_MAX_CONNECTIONS="4", LIFT_DB_ACQUIRE_TIMEOUT_MS="1000")


def request(port, method, path, body=None):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    try:
        connection.request(method, path, body=body)
        response = connection.getresponse()
        return response.status, response.read().decode("utf-8")
    finally:
        connection.close()


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


@contextlib.contextmanager
def server(directory, driver, url):
    port = free_port()
    env = dict(database_env(driver, url), LIFT_HTTP_ADDR=f"127.0.0.1:{port}")
    with tempfile.TemporaryFile() as log:
        process = subprocess.Popen([EXAMPLE / f"main{SUFFIX}"], cwd=directory, env=env,
                                   stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    log.seek(0)
                    raise AssertionError(f"server exited: {log.read().decode(errors='replace')}")
                try:
                    if request(port, "GET", "/health") == (200, "ok"):
                        yield port
                        return
                except OSError:
                    pass
                time.sleep(0.05)
            raise AssertionError("HTTP server did not become ready")
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)


def fixture(directory, driver, url, mode):
    result = subprocess.run([DATABASE / f"notes-fixture{SUFFIX}"], cwd=directory,
                            env=dict(database_env(driver, url), LIFT_TEST_FIXTURE_MODE=mode),
                            capture_output=True, timeout=20)
    if result.returncode != 0:
        raise AssertionError(
            f"database fixture {mode} failed ({result.returncode}): "
            f"{result.stdout.decode(errors='replace')}\n{result.stderr.decode(errors='replace')}"
        )


def verify_backend(directory, driver, url):
    text = "Nota persistente '); DROP TABLE notes; -- 🙂 " + uuid.uuid4().hex
    with server(directory, driver, url) as port:
        assert request(port, "GET", "/health") == (200, "ok")
        status, created = request(port, "POST", "/notes", text.encode())
        assert status == 201 and created.startswith("created note "), (status, created)
        note_id = int(created.removeprefix("created note "))
        marker = f"{note_id}\t{text}"
        status, body = request(port, "GET", "/notes")
        assert status == 200 and marker in body, (status, body)
        assert request(port, "POST", "/notes", b" \t\n")[0] == 400
        assert request(port, "POST", "/notes", b"\xff")[0] == 400
        assert request(port, "POST", "/notes", b"x" * 16384)[0] == 201
        assert request(port, "POST", "/notes", b"x" * 16385)[0] == 413
        assert request(port, "GET", "/missing")[0] == 404
        assert request(port, "PUT", "/health")[0] == 405
    with server(directory, driver, url) as port:
        status, body = request(port, "GET", "/notes")
        assert status == 200 and marker in body
    # Keep the listener open throughout the competing bind attempt.
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        port = listener.getsockname()[1]
        env = dict(database_env(driver, url), LIFT_HTTP_ADDR=f"127.0.0.1:{port}")
        result = subprocess.run([EXAMPLE / f"main{SUFFIX}"], cwd=directory, env=env,
                                capture_output=True, timeout=20)
        assert result.returncode != 0 and b"could not bind" in result.stdout, result
    fixture(directory, driver, url, "bad")
    try:
        with server(directory, driver, url) as port:
            assert request(port, "GET", "/notes") == (500, "invalid notes database row")
            assert request(port, "POST", "/notes", b"test") == (500, "notes database is unavailable")
    finally:
        fixture(directory, driver, url, "clean")
    contract = subprocess.run([DATABASE / f"contract{SUFFIX}"], cwd=directory,
                              env=database_env(driver, url), capture_output=True, timeout=30)
    assert contract.returncode == 0 and b"PASS: general SQL" in contract.stdout, (contract.returncode, contract.stdout, contract.stderr)
    print(f"PASS {driver}: HTTP/restart/errors/limit/bind; general SQL/pool/typed values/getters/commit/rollback/drop")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--postgres", action="store_true")
    args = parser.parse_args()
    postgres_url = os.environ.get("LIFT_TEST_POSTGRES_URL")
    if args.postgres and not postgres_url:
        parser.error("--postgres requires LIFT_TEST_POSTGRES_URL to a dedicated disposable test database")
    if not args.skip_build:
        run("cargo", "build", "--locked", "-p", "lift_cli")
        compiler = ROOT / "target" / "debug" / f"lift{SUFFIX}"
        for source in (EXAMPLE / "main.lift", EXAMPLE / "resources.lift", DATABASE / "contract.lift", DATABASE / "notes-fixture.lift"):
            run(str(compiler), "build", str(source))
    with tempfile.TemporaryDirectory(prefix="lift-notes-acceptance-") as temporary:
        directory = Path(temporary)
        verify_backend(directory, "sqlite", "sqlite:" + str(directory / "notes.sqlite3"))
        if args.postgres:
            verify_backend(directory, "postgres", postgres_url)
        resources = subprocess.run([EXAMPLE / f"resources{SUFFIX}"], cwd=directory, check=True, capture_output=True)
        output = resources.stdout.decode("utf-8")
        assert "committed Lift resource" in output and "rolled back" not in output and "dropped transaction" not in output, output
    print("PASS: legacy SQLite resource protocol retained")


if __name__ == "__main__":
    main()
