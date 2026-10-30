"""A minimal DevTools client, for testing a tool's page in Falkon offscreen (moved into the suite
from dexter-foley's docs/dev/cdp.py, VENDORED.md):
python3 -I ../suite-audio-libs/tools/cdp.py PORT SCRIPT.json OUTDIR

SCRIPT is a list of steps: ["nav", url] | ["eval", js] | ["key", key, "ctrl"?] | ["type", text] |
["shot", name] | ["wait", s] | ["size", width, height, "phone"?] (the window, 1200 by 520 at the
start; "phone": a touch screen, as a phone's)
"""
import base64, json, os, socket, struct, sys, time, urllib.request

port, script, out = int(sys.argv[1]), json.load(open(sys.argv[2])), sys.argv[3]
targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json"))
page = next(t for t in targets if t.get("type") == "page")
url = page["webSocketDebuggerUrl"]
host, path = url[len("ws://"):].split("/", 1)
h, p = host.split(":")
s = socket.create_connection((h, int(p)))
key = base64.b64encode(os.urandom(16)).decode()
s.sendall(f"GET /{path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
          f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
resp = b""
while b"\r\n\r\n" not in resp:
    resp += s.recv(1)
assert b" 101 " in resp, resp


def send(obj):
    data = json.dumps(obj).encode()
    hdr = bytearray([0x81])
    n = len(data)
    if n < 126:
        hdr.append(0x80 | n)
    elif n < 65536:
        hdr.append(0x80 | 126); hdr += struct.pack(">H", n)
    else:
        hdr.append(0x80 | 127); hdr += struct.pack(">Q", n)
    mask = os.urandom(4)
    s.sendall(bytes(hdr) + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))


def exact(n):
    b = b""
    while len(b) < n:
        c = s.recv(n - len(b))
        if not c:
            raise EOF
        b += c
    return b


def recv():
    msg = b""
    while True:
        b0, b1 = exact(2)
        n = b1 & 0x7F
        if n == 126:
            n = struct.unpack(">H", exact(2))[0]
        elif n == 127:
            n = struct.unpack(">Q", exact(8))[0]
        msg += exact(n)
        if b0 & 0x80:
            return json.loads(msg)


ids = [0]


def call(method, **params):
    ids[0] += 1
    send({"id": ids[0], "method": method, "params": params})
    while True:
        m = recv()
        if m.get("id") == ids[0]:
            if "error" in m:
                raise RuntimeError(m["error"])
            return m.get("result", {})


call("Page.enable")
call("Emulation.setDeviceMetricsOverride", width=1200, height=520, deviceScaleFactor=1, mobile=False)
for step in script:
    kind = step[0]
    if kind == "nav":
        call("Page.navigate", url=step[1]); time.sleep(1.5)
    elif kind == "eval":
        r = call("Runtime.evaluate", expression=step[1], returnByValue=True, awaitPromise=True)
        print("eval:", json.dumps(r.get("result", {}).get("value")))
    elif kind == "key":
        # ["key", KEY] or ["key", KEY, "ctrl"|"shift"]
        k = step[1]
        mods = {"ctrl": 2, "shift": 8}.get(step[2] if len(step) > 2 else "", 0)
        codes = {"End": 35, "Home": 36, "ArrowDown": 40, "ArrowUp": 38, " ": 32, "Enter": 13, "Escape": 27,
                 "/": 191, "Tab": 9, "Backspace": 8, ",": 188}
        code = codes.get(k, ord(k.upper()) if len(k) == 1 else 0)
        text = {"Enter": "\r"}.get(k, k if len(k) == 1 else "")
        for t in ("keyDown", "keyUp"):
            call("Input.dispatchKeyEvent", type=t, key=k, windowsVirtualKeyCode=code, modifiers=mods,
                 text=(text if t == "keyDown" and not mods & 2 else ""))
        time.sleep(0.3)
    elif kind == "type":
        call("Input.insertText", text=step[1]); time.sleep(0.6)
    elif kind == "shot":
        r = call("Page.captureScreenshot", format="png")
        open(os.path.join(out, step[1] + ".png"), "wb").write(base64.b64decode(r["data"]))
        print("shot:", step[1])
    elif kind == "size":
        phone = len(step) > 3 and step[3] == "phone"
        call("Emulation.setDeviceMetricsOverride", width=step[1], height=step[2], deviceScaleFactor=1, mobile=phone)
        call("Emulation.setTouchEmulationEnabled", enabled=phone, maxTouchPoints=5 if phone else 1)
        call("Emulation.setEmulatedMedia", features=[{"name": "pointer", "value": "coarse" if phone else "fine"},
                                                      {"name": "hover", "value": "none" if phone else "hover"}])
        time.sleep(0.5)
    elif kind == "wait":
        time.sleep(step[1])
