import socket
import sys
import time


def send(sock, line):
    sock.sendall((line + "\r\n").encode())


def main():
    host, port, nick, target = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
    sock = socket.create_connection((host, port), 5)
    sock.settimeout(0.4)
    send(sock, "CAP LS 302")
    send(sock, "NICK " + nick)
    send(sock, "USER " + nick + " 0 * :peer")
    buf = b""
    registered = False
    saw_ls = False
    deadline = time.time() + 12
    while time.time() < deadline and not registered:
        try:
            chunk = sock.recv(4096)
            if not chunk:
                break
            buf += chunk
            text = chunk.decode("utf-8", "replace")
            for line in text.split("\r\n"):
                if line.startswith("PING"):
                    send(sock, "PONG " + line[5:])
            whole = buf.decode("utf-8", "replace")
            if (not saw_ls) and "CAP" in whole and " LS " in whole:
                send(sock, "CAP END")
                saw_ls = True
            if b" 001 " in buf:
                registered = True
        except socket.timeout:
            continue
    if not registered:
        print("PEER_FAILED_REGISTER", flush=True)
        sys.exit(1)
    print("PEER_REGISTERED", flush=True)
    send(sock, "PRIVMSG " + target + " :dm ping from peer")
    print("PEER_SENT_DM", flush=True)
    deadline = time.time() + 25
    while time.time() < deadline:
        try:
            chunk = sock.recv(4096)
            if not chunk:
                break
            buf += chunk
            chunk_text = chunk.decode("utf-8", "replace")
            for line in chunk_text.split("\r\n"):
                if line.startswith("PING"):
                    send(sock, "PONG " + line[5:])
            text = buf.decode("utf-8", "replace")
            if "dm reply from ui" in text:
                print("PEER_GOT_REPLY", flush=True)
                break
        except socket.timeout:
            continue
    sock.close()


if __name__ == "__main__":
    main()
