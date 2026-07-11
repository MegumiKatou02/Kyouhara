#!/usr/bin/env python3
#
# chưa nối vào CI — treo ở tầng WebDriver-không-đọc-được-JS, xem m5-ket-thuc.md
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
                _dump(sid)          # in cả stage + console khi có lỗi
                return 1
            time.sleep(1)

        # Chỉ tới đây khi ĐÃ hết deadline — giờ mới in chẩn đoán, MỘT lần.
        print(f"HONG: qua {TIMEOUT_S}s khong thay __mong_ready")
        _dump(sid)
        return 1
    finally:
        call("DELETE", f"/session/{sid}")


def _dump(sid):
    print("__mong_error =", js(sid, "return window.__mong_error || null;"))
    print("__mong_stage =", js(sid,
        "return window.__mong_stage || '(khong co)';"))
    logs = js(sid, "return (window.__mong_log || []).join('\\n');")
    print(f"--- console ({len(logs)} ky tu) ---")
    print(logs if logs else "(rong)")

if __name__ == "__main__":
    sys.exit(main())
