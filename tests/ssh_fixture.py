"""Loopback-only SSH/SFTP fixture. Uses only generated test files and an ephemeral host key."""
import argparse
import base64
import errno
import hashlib
import io
import json
import logging
import os
from pathlib import Path
import posixpath
import socket
import stat
import sys
import threading
import time

PROJECT_ROOT = Path(__file__).resolve().parent.parent
import paramiko
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

parser = argparse.ArgumentParser(description="Generated localhost-only SSH/SFTP test server")
parser.add_argument("--work-dir", default=str(PROJECT_ROOT / "test-work"))
parser.add_argument("--port-file")
args = parser.parse_args()
work = Path(args.work_dir).resolve()
allowed_work = (PROJECT_ROOT / "test-work").resolve()
if not work.is_relative_to(allowed_work):
    raise SystemExit("Fixture files must stay under the project's test-work/ directory")
port_file = Path(args.port_file).resolve() if args.port_file else work / "ssh-fixture-status.json"
if not port_file.is_relative_to(work):
    raise SystemExit("Fixture status must stay under its test-work/ directory")
work.mkdir(parents=True, exist_ok=True)
logging.basicConfig(filename=str(work / "ssh-fixture-debug.log"), level=logging.DEBUG)
root = work / "ssh-fixture-files"
root.mkdir(parents=True, exist_ok=True)
port_file.parent.mkdir(parents=True, exist_ok=True)
stats_file = port_file.with_name("ssh-fixture-stats.json")
stop_file = port_file.with_name("ssh-fixture-stop")
stop_file.unlink(missing_ok=True)
stats = {"auth_attempts": 0, "accepted_auth": 0, "publickey_attempts": 0, "pty": [], "resize": [], "connections": 0}
stats_lock = threading.Lock()


def record(key, value=None):
    with stats_lock:
        if value is None:
            stats[key] += 1
        else:
            stats[key].append(value)
        stats_file.write_text(json.dumps(stats), encoding="utf-8")


def resolve(remote):
    if "\0" in remote or "\\" in remote:
        raise OSError(errno.EACCES, "Invalid remote path")
    normal = posixpath.normpath("/" + remote.lstrip("/"))
    candidate = (root / normal.lstrip("/")).resolve()
    if not candidate.is_relative_to(root.resolve()):
        raise OSError(errno.EACCES, "Path outside fixture")
    return candidate


def status_error(error):
    return paramiko.SFTPServer.convert_errno(error.errno or errno.EIO)


class Handle(paramiko.SFTPHandle):
    def stat(self):
        try:
            return paramiko.SFTPAttributes.from_stat(os.fstat(self.readfile.fileno() if hasattr(self, "readfile") else self.writefile.fileno()))
        except OSError as exc:
            return status_error(exc)


class Files(paramiko.SFTPServerInterface):
    def canonicalize(self, path):
        try:
            candidate = resolve(path)
            return "/" + candidate.relative_to(root.resolve()).as_posix().removeprefix(".")
        except OSError:
            return "/"

    def list_folder(self, path):
        try:
            result = []
            for entry in resolve(path).iterdir():
                attr = paramiko.SFTPAttributes.from_stat(entry.lstat())
                attr.filename = entry.name
                result.append(attr)
            return result
        except OSError as exc:
            return status_error(exc)

    def stat(self, path):
        try:
            return paramiko.SFTPAttributes.from_stat(resolve(path).stat())
        except OSError as exc:
            return status_error(exc)

    def lstat(self, path):
        if path.endswith("/fixture-link"):
            attr = paramiko.SFTPAttributes()
            attr.st_mode = stat.S_IFLNK | 0o777
            attr.st_size = 7
            return attr
        return self.stat(path)

    def open(self, path, flags, attr):
        try:
            fd = os.open(resolve(path), flags | getattr(os, "O_BINARY", 0), attr.st_mode if attr.st_mode is not None else 0o600)
            mode = "r+b" if flags & os.O_RDWR else "wb" if flags & os.O_WRONLY else "rb"
            stream = os.fdopen(fd, mode)
            handle = Handle(flags)
            if flags & os.O_RDWR:
                handle.readfile = stream
                handle.writefile = stream
            elif flags & os.O_WRONLY:
                handle.writefile = stream
            else:
                handle.readfile = stream
            return handle
        except OSError as exc:
            return status_error(exc)

    def remove(self, path):
        try:
            resolve(path).unlink()
            return paramiko.SFTP_OK
        except OSError as exc:
            return status_error(exc)

    def rename(self, oldpath, newpath):
        try:
            source, target = resolve(oldpath), resolve(newpath)
            if target.exists():
                return paramiko.SFTP_FAILURE
            source.rename(target)
            return paramiko.SFTP_OK
        except OSError as exc:
            return status_error(exc)

    def mkdir(self, path, attr):
        try:
            resolve(path).mkdir(mode=attr.st_mode if attr.st_mode is not None else 0o755)
            return paramiko.SFTP_OK
        except OSError as exc:
            return status_error(exc)

    def rmdir(self, path):
        try:
            resolve(path).rmdir()
            return paramiko.SFTP_OK
        except OSError as exc:
            return status_error(exc)



def relay_stream(channel, peer):
    """Fixture relay: only channels previously approved for loopback endpoints."""
    channel.settimeout(0.2)
    peer.settimeout(0.2)

    def copy(source, target):
        copied = 0
        try:
            while True:
                try:
                    data = source.recv(32768)
                except socket.timeout:
                    continue
                if not data:
                    break
                target.sendall(data)
                copied += len(data)
        except (EOFError, OSError, paramiko.SSHException):
            pass
        finally:
            logging.getLogger("fixture").debug("Relay %s to %s finished %d bytes", type(source).__name__, type(target).__name__, copied)
            try:
                target.shutdown(socket.SHUT_WR)
            except (OSError, EOFError):
                pass

    first = threading.Thread(target=copy, args=(channel, peer), daemon=True)
    first.start()
    copy(peer, channel)
    first.join(timeout=3)
    channel.close()
    peer.close()


class Server(paramiko.ServerInterface):
    def __init__(self):
        self.shell = threading.Event()
        self.transport = None
        self.direct_destinations = {}
        self.remote_listeners = {}
        self.forward_lock = threading.Lock()

    def get_allowed_auths(self, username):
        return "password,publickey"

    def check_auth_password(self, username, password):
        record("auth_attempts")
        if username == "testuser" and password == "fixture-password":
            record("accepted_auth")
            return paramiko.AUTH_SUCCESSFUL
        return paramiko.AUTH_FAILED

    def check_auth_publickey(self, username, key):
        record("auth_attempts")
        record("publickey_attempts")
        if username == "testuser" and key.asbytes() == client_public_key:
            record("accepted_auth")
            return paramiko.AUTH_SUCCESSFUL
        return paramiko.AUTH_FAILED

    def check_channel_request(self, kind, chanid):
        return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED


    def check_channel_direct_tcpip_request(self, chanid, origin, destination):
        if destination[0] != "127.0.0.1" or not 1024 < destination[1] <= 65535:
            return paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED
        with self.forward_lock:
            self.direct_destinations[chanid] = destination
        return paramiko.OPEN_SUCCEEDED

    def check_port_forward_request(self, address, port):
        if address not in ("127.0.0.1", "localhost") or not 1024 < port <= 65535:
            return False
        try:
            remote_listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            remote_listener.bind(("127.0.0.1", port))
            remote_listener.listen(8)
            remote_listener.settimeout(0.2)
        except OSError:
            remote_listener.close()
            return False
        with self.forward_lock:
            if len(self.remote_listeners) >= 4:
                remote_listener.close()
                return False
            self.remote_listeners[port] = remote_listener

        def accept_forwarded():
            while self.transport and self.transport.is_active():
                try:
                    peer, origin = remote_listener.accept()
                except socket.timeout:
                    continue
                except OSError:
                    break
                try:
                    channel = self.transport.open_forwarded_tcpip_channel(origin, ("127.0.0.1", port))
                    threading.Thread(target=relay_stream, args=(channel, peer), daemon=True).start()
                except (OSError, EOFError, paramiko.SSHException):
                    peer.close()
            remote_listener.close()

        threading.Thread(target=accept_forwarded, daemon=True).start()
        return port

    def cancel_port_forward_request(self, address, port):
        with self.forward_lock:
            listener = self.remote_listeners.pop(port, None)
        if listener:
            listener.close()

    def close_forwarding(self):
        with self.forward_lock:
            listeners = list(self.remote_listeners.values())
            self.remote_listeners.clear()
        for listener in listeners:
            listener.close()

    def check_channel_pty_request(self, channel, term, width, height, pixelwidth, pixelheight, modes):
        record("pty", [width, height])
        return True

    def check_channel_window_change_request(self, channel, width, height, pixelwidth, pixelheight):
        record("resize", [width, height])
        return True

    def check_channel_shell_request(self, channel):
        threading.Thread(target=serve_shell, args=(channel,), daemon=True).start()
        return True


def serve_shell(channel):
    try:
        channel.sendall("\x1b[32mNEXUS-FIXTURE ready — été ✓\x1b[0m\r\n$ ".encode())
        pending = bytearray()
        skip_lf = False
        while not channel.closed:
            chunk = channel.recv(65536)
            if not chunk:
                break
            for byte in chunk:
                if byte == 3:
                    pending.clear()
                    channel.sendall(b"^C\r\n$ ")
                elif byte in (10, 13):
                    if byte == 10 and skip_lf:
                        skip_lf = False
                        continue
                    skip_lf = byte == 13
                    line = pending.decode("utf-8", errors="replace")
                    pending.clear()
                    if line == "exit":
                        channel.sendall(b"\r\nBye\r\n")
                        channel.send_exit_status(0)
                        channel.close()
                        return
                    channel.sendall(("\r\nECHO:" + line + "\r\n$ ").encode())
                else:
                    pending.append(byte)
    except (EOFError, OSError):
        pass


def generated_openssh_private():
    return Ed25519PrivateKey.generate().private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.OpenSSH,
        serialization.NoEncryption(),
    )


# The host private key never leaves memory. The separate client key exists only
# to exercise the application's Unicode Windows path and Ed25519 authentication.
host_key = paramiko.Ed25519Key.from_private_key(io.StringIO(generated_openssh_private().decode("ascii")))
client_private = generated_openssh_private()
client_key = paramiko.Ed25519Key.from_private_key(io.StringIO(client_private.decode("ascii")))
client_public_key = client_key.asbytes()
client_key_path = port_file.with_name("ssh-fixture-clé-ed25519")
client_key_path.write_bytes(client_private)
del client_private, client_key
fingerprint = "SHA256:" + base64.b64encode(hashlib.sha256(host_key.asbytes()).digest()).decode().rstrip("=")
listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
listener.bind(("127.0.0.1", 0))
listener.listen(10)
listener.settimeout(0.5)
port_file.write_text(json.dumps({"pid": os.getpid(), "host": "127.0.0.1", "port": listener.getsockname()[1], "fingerprint": fingerprint, "host_key_algorithm": "ssh-ed25519", "private_key_path": str(client_key_path)}), encoding="utf-8")
stats_file.write_text(json.dumps(stats), encoding="utf-8")


def handle_client(sock):
    transport = None
    try:
        record("connections")
        transport = paramiko.Transport(sock)
        # Paramiko 5.1 copies its KEX proposal before stripping GEX when no
        # modulus pack exists. Set an explicit strong list before negotiation
        # so advertised and internally selected algorithms are identical.
        security = transport.get_security_options()
        strong_kex = ("curve25519-sha256@libssh.org", "ecdh-sha2-nistp256", "ecdh-sha2-nistp384", "ecdh-sha2-nistp521", "diffie-hellman-group16-sha512", "diffie-hellman-group18-sha512", "diffie-hellman-group14-sha256")
        security.kex = tuple(name for name in strong_kex if name in security.kex)
        transport.add_server_key(host_key)
        transport.set_subsystem_handler("sftp", paramiko.SFTPServer, Files)
        server = Server()
        server.transport = transport
        transport.start_server(server=server)
        # Retain accepted channels: otherwise Python closes them before the handler runs.
        channels = []
        while transport.is_active():
            channel = transport.accept(0.2)
            if channel is not None:
                with server.forward_lock:
                    destination = server.direct_destinations.pop(channel.get_id(), None)
                if destination:
                    try:
                        peer = socket.create_connection(destination, timeout=2)
                        threading.Thread(target=relay_stream, args=(channel, peer), daemon=True).start()
                    except OSError:
                        channel.close()
                channels.append(channel)
            channels[:] = [channel for channel in channels if not channel.closed]
    except (EOFError, OSError, paramiko.SSHException):
        logging.getLogger("fixture").exception("Fixture transport failed")
    finally:
        if transport:
            transport.close()
        if "server" in locals():
            server.close_forwarding()
        sock.close()


try:
    while not stop_file.exists():
        try:
            client, address = listener.accept()
        except socket.timeout:
            continue
        if address[0] != "127.0.0.1":
            client.close()
            continue
        threading.Thread(target=handle_client, args=(client,), daemon=True).start()
except KeyboardInterrupt:
    pass
finally:
    listener.close()
    client_key_path.unlink(missing_ok=True)

