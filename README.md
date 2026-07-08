# Mộng Engine

Engine visual novel viết bằng Rust — web / desktop / mobile từ một codebase.
Xem `docs/thiet-ke-mong-engine.md` (tài liệu thiết kế) và `docs/spec-ir.md` (spec tập lệnh IR).

Trạng thái: **M0** — workspace skeleton, IR + máy ảo tối thiểu (`mong-core`),
định dạng gói `.mongpack` v0 (`mong-assets`), nạp & validate cốt truyện (`mong-script`).

```
cargo test --workspace          # toàn bộ test M0
```

Ghi chú M0: codec nén của mongpack v0 là DEFLATE (thuần Rust, chạy được trên WASM
ngay từ đầu); header có trường codec nên zstd sẽ được thêm làm codec 2 sau mà
không phá định dạng.
