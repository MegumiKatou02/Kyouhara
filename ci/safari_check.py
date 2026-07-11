#!/usr/bin/env python3
"""Safari smoke test: mo trang demo, cho __mong_ready do Rust dat trong
frame() dau tien (xem shells/common/state.rs). Thoat 0 = Safari ve duoc
frame that; thoat 1 kem __mong_error neu co."""
import json
import sys
import time
import urllib.request

WD = "http://localhost:4444"
PAGE = "http://localhost:8080/shells/web/index.html"
TIMEOUT_S = 60


def call(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        WD + path, data=data, method=method,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["value"]


def js(sid, script):
    return call("POST", f"/session/{sid}/execute/sync",
                {"script": script, "args": []})


def main():
    sid = call("POST", "/session",
               {"capabilities": {"alwaysMatch": {"browserName": "safari"}}})["sessionId"]
    try:
        call("POST", f"/session/{sid}/url", {"url": PAGE})
        deadline = time.monotonic() + TIMEOUT_S
        while time.monotonic() < deadline:
            if js(sid, "return window.__mong_ready === true;"):
                print("OK: Safari ve duoc frame dau tien")
                return 0
            err = js(sid, "return window.__mong_error || null;")
            if err:
                print(f"HONG: {err}")
                return 1
            time.sleep(1)
        print(f"HONG: qua {TIMEOUT_S}s khong thay __mong_ready")
        print("__mong_error =", js(sid, "return window.__mong_error || null;"))
        # Kéo mọi thứ đã log ra để biết frame() kẹt ở đâu (adapter? panic?).
        logs = js(sid, """
            return (window.__mong_log || []).join('\\n');
        """)
        if logs:
            print("--- console ---")
            print(logs)
        return 1
    finally:
        call("DELETE", f"/session/{sid}")


if __name__ == "__main__":
    sys.exit(main())
