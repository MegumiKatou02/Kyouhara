# Mộng Engine — Tài liệu thiết kế kỹ thuật

**Phiên bản:** 0.2 (đã chốt 4 quyết định mở — sẵn sàng khởi động M0)
**Trạng thái:** duyệt lần cuối rồi bắt đầu M0
**Nguồn gốc:** đúc kết từ prototype Mộng Studio v1–v4 (HTML/JS)

---

## 1. Tổng quan và mục tiêu

Mộng Engine là engine visual novel viết bằng Rust, nhắm ba nền tảng web, desktop và mobile từ một codebase duy nhất. Engine sinh ra để giải quyết những điểm yếu đã xác định ở các engine hiện có: Ren'Py có curve học khó, debug khổ, build nặng vài trăm MB và localization là thứ gắn thêm sau; các engine thương mại thì đóng, khó mở rộng. Mộng Engine chọn hướng ngược lại ở bốn trục: plugin-first (mọi hành vi có thể mở rộng), localization-first (đa ngôn ngữ nằm trong data model từ ngày đầu), debug-first (rollback, time-travel, lint cốt truyện là tính năng lõi chứ không phải tiện ích), và export nhẹ (mục tiêu web build dưới 5 MB chưa tính assets).

Phạm vi phiên bản 1.0 không bao gồm: 3D, video playback, chỉnh sửa cộng tác thời gian thực (để v2), console port, và trình soạn chạy trên mobile. Những thứ này được ghi nhận để kiến trúc không chặn đường về sau, nhưng không nằm trong lộ trình hiện tại.

## 2. Bài học từ prototype v1–v4

Prototype HTML đã chứng minh trải nghiệm người dùng của từng tính năng. Bảng dưới ánh xạ mỗi tính năng đã kiểm chứng sang module chịu trách nhiệm trong engine thật, kèm những gì phải làm khác đi:

| Tính năng đã kiểm chứng ở prototype | Module engine | Khác biệt khi làm thật |
|---|---|---|
| Node editor phân nhánh, kéo thả, zoom/pan | `editor/` (Tauri) | Tái sử dụng UI prototype, thay data layer bằng gọi vào core qua IPC |
| Runtime thoại: typewriter, lựa chọn, điều kiện, biến | `mong-core` | Chuyển từ interpreter JS ad-hoc sang máy ảo chạy IR có kiểm thử |
| Rollback + time-travel theo snapshot | `mong-core` | Snapshot ring buffer, serialize bằng `serde`, xác định (deterministic) |
| Save/load slot | `mong-core` + shell | Core sinh dữ liệu save có version; shell lo chỗ ghi (file/localStorage) |
| Plugin JS: hook + filter, cô lập lỗi | `mong-plugin` | Chuyển sang ngôn ngữ nhúng sandbox (mục 7); giữ nguyên bảng hook |
| Đa ngôn ngữ fallback về bản gốc | `mong-i18n` | Thêm plural rules, RTL, font fallback theo ngôn ngữ |
| Sprite nhiều biểu cảm, 3 vị trí sân khấu, dim người không nói | `mong-render` | Sprite ghép layer (thân + mặt) thay vì mỗi biểu cảm một ảnh full |
| BGM theo cảnh, SFX theo dòng | `mong-audio` | Crossfade, điều khiển volume theo bus (bgm/sfx/voice) |
| Quét lỗi cốt truyện (nhánh mồ côi, biến chưa khai báo, soft-lock) | `mong-cli lint` + editor | Chạy trên IR nên chính xác tuyệt đối, tích hợp CI được |
| Xuất game một file HTML | `mong-cli export` | Web: WASM + mongpack; desktop: binary; mobile: project template |

Bài học quan trọng nhất không phải tính năng mà là ranh giới: prototype cho thấy phần "nội dung" (node, thoại, lựa chọn) và phần "thực thi" (runtime) tách được sạch qua một định dạng dữ liệu trung gian. Toàn bộ kiến trúc bên dưới xây trên ranh giới đó.

## 3. Kiến trúc tổng thể

Nguyên tắc số một: **lõi engine là Rust thuần, không biết gì về platform**. Mọi thứ dính đến cửa sổ, input, file system, âm thanh đầu ra đều đi qua trait do shell cung cấp. Nhờ vậy `mong-core` chạy được ở mọi nơi kể cả trong test không có màn hình.

```
mong/
├── Cargo.toml                 # workspace root
├── crates/
│   ├── mong-core/             # máy ảo cốt truyện: state, IR executor,
│   │                          #   snapshot/rollback, save, biến, điều kiện
│   ├── mong-script/           # parser MộngScript (DSL) → IR;
│   │                          #   cũng nhận JSON từ editor → IR
│   ├── mong-assets/           # định dạng .mongpack, loader, cache,
│   │                          #   hot-reload watcher (desktop dev)
│   ├── mong-render/           # wgpu: sprite batching, transition,
│   │                          #   text shaping (cosmic-text)
│   ├── mong-audio/            # kira: bus bgm/sfx/voice, crossfade
│   ├── mong-plugin/           # host script nhúng + registry hook
│   ├── mong-i18n/             # locale, fallback, plural, font map
│   └── mong-runtime/          # ghép tất cả: vòng lặp, event, input
├── shells/
│   ├── common/               # 
│   ├── desktop/               # winit; Windows/macOS/Linux
│   ├── web/                   # wasm-bindgen; WebGPU, fallback WebGL2
│   ├── android/               # cargo-ndk + shell Kotlin mỏng
│   └── ios/                   # staticlib FFI + shell Swift mỏng
├── editor/                    # Mộng Studio: Tauri (Rust backend
│                              #   + UI web kế thừa prototype)
├── tools/
│   └── mong-cli/              # new / dev / lint / pack / export
└── docs/                      # tài liệu này + spec chi tiết
```

Chiều phụ thuộc chỉ đi một hướng: shells → mong-runtime → các crate lõi → không gì cả. `mong-core` đặc biệt không được phụ thuộc render/audio — nó chỉ phát ra "lệnh trình diễn" (presentation commands) mà runtime dịch sang hình và tiếng. Đây là điều kiện để có text-mode runner ở milestone M1: chạy cả cốt truyện trong terminal, không cần GPU, phục vụ test tự động.

## 4. Spec định dạng dữ liệu

Có hai định dạng, mục đích khác nhau. **`.mong` (dự án)** là thứ tác giả chỉnh sửa: một thư mục gồm `project.json` (kế thừa trực tiếp schema prototype v4, thêm trường `formatVersion`) và thư mục `assets/` chứa file rời — không còn nhúng base64 như prototype, giải quyết vấn đề file phình. **`.mongpack` (phân phối)** là thứ người chơi nhận: một file duy nhất gồm header, IR đã biên dịch, bảng chuỗi theo locale, và assets đã nén (zstd), có checksum từng entry.

Các thực thể chính và trường bắt buộc (đây là hợp đồng, đổi phải tăng `formatVersion` và viết migration):

| Thực thể | Trường | Ghi chú |
|---|---|---|
| Project | `formatVersion, title, defaultLocale, locales[], variables{}, startNode` | `variables` là giá trị khởi tạo, kiểu `i64 \| bool \| string` |
| Node | `id, title, scene, body[]` | `body` là dãy lệnh IR nguồn (mục 5), thay cho `lines + mode + next` của prototype — tổng quát hơn |
| Character | `id, name(dịch được), color, layers[]` | nằm trong `manifest.json`, không trong `project.json` (RFC-001) | `layers`: base + biểu cảm + trang phục ghép chồng; **đã chốt:** phía tác giả là PNG rời theo quy ước thư mục (`assets/characters/<id>/<layer>/<tên>.png`), `mong-cli pack` tự đóng texture atlas + metadata; mỗi layer có trường `kind` chừa đường cho mesh/skeletal sau này |
| Scene | `id, name, bg, bgm, ambience?` | như trên |
| Asset | `id, path, kind(image/audio/font), hash` | path tương đối trong `assets/` |
| Plugin | `id, name, enabled, lang, source` | đóng gói kèm mongpack |
| String | mọi text hiển thị đều là `{key}` trỏ vào bảng chuỗi theo locale | **đã chốt:** key tự sinh một lần khi chuỗi ra đời, lưu bền trong dự án, không đổi khi sửa nội dung/đảo thứ tự; editor và DSL ẩn key; tác giả gắn `@key ten_rieng` khi plugin cần tham chiếu; xuất/nhập xliff/po cho translator |

Quy tắc tương thích: runtime đọc được mongpack có `formatVersion` cũ hơn trong cùng major; editor mở `.mong` cũ thì tự migrate và báo. Save file của người chơi ghi kèm `formatVersion` + hash cốt truyện — nếu cốt truyện đã đổi, runtime thử khớp theo `node id` và cảnh báo thay vì crash.

`manifest.json` là file thứ ba của dự án (cạnh `project.json`, `assets/`),
mang `format_version` **riêng** (hiện = 2), độc lập với `format_version` của
IR. Đóng vào mongpack dưới entry `Meta`. Lý do tách: mong-core không được
biết bg/sprite là gì, nên metadata trình diễn không được nằm chung với Story.

Văn bản của metadata (tên nhân vật, tên cảnh) là **key** trỏ vào
`manifest.strings[locale]`, không phải văn bản thẳng — tên nhân vật phải dịch
được (localization-first). Miền key này tách khỏi bảng chuỗi nội dung sinh từ
DSL: `mong-cli fmt` quản miền nội dung và không bao giờ đụng manifest. Hai
bảng hợp nhất lúc tra cứu qua `Catalog::merge_table`; key nội dung thắng khi
trùng, và lint L022 bắt trùng từ lúc soạn.

Quy tắc tương thích chưa hiện thực; xem mongpack-entries.md §5.1.

## 5. Máy ảo cốt truyện (mong-core)

Trái tim của engine là một máy ảo nhỏ chạy tập lệnh trung gian (IR). Cả editor trực quan lẫn DSL text đều biên dịch về IR này — nhờ đó tranh luận "viết bằng form hay bằng text" (mục 6 nhóm 2 của prototype) biến mất: hai mặt của cùng một đồng xu.

Tập lệnh IR khởi điểm, đủ phủ toàn bộ prototype v4:

```
say(speaker?, string_key, {pose?, pos?, sfx?, exit?})   # một dòng thoại
show(char, pose, pos) / hide(char)                      # điều khiển sân khấu
scene(scene_id, transition?)                            # đổi cảnh (tự dọn sân khấu, đổi bgm)
choice([{string_key, label, cond?, effects[]}])         # chờ người chơi chọn
jump(label) / call(label) / return                      # điều hướng; call cho cảnh dùng lại
set(var, op, value)                                     # =, +=, -=, toggle
if(cond) ... else ... end                               # rẽ nhánh trong node
wait(ms) / sfx(id) / bgm(id?)                           # trình diễn
ext(plugin_cmd, args)                                   # lệnh do plugin đăng ký
end                                                     # kết thúc truyện
```

Vòng đời runtime là máy trạng thái tường minh: `Idle → Running → (AwaitAdvance | AwaitChoice | Waiting) → Running → ... → Ended`. `Running` thực thi IR cho đến khi gặp lệnh cần chờ (say chờ click, choice chờ chọn, wait chờ timer). Mỗi lần dừng chờ, core phát một `PresentationEvent` (hiện thoại X, hiện lựa chọn Y...) — runtime/renderer chỉ việc vẽ theo, còn text-mode runner thì in ra terminal.

Rollback kế thừa thiết kế snapshot của prototype nhưng làm chặt: thực thi phải **xác định** (không đọc thời gian thật, không random ngoài PRNG có seed trong state), snapshot = `{pc, call_stack, vars, rng_seed}` lưu vào ring buffer (mặc định 400 mục, cấu hình được). Sân khấu (nền, sprite, transition) **không** nằm trong snapshot của core —
core không có khái niệm sân khấu. `mong-runtime` giữ một ngăn xếp `Stage`
song song, đẩy/rút 1:1 với mỗi lần VM dừng (RFC-001). Save slot = snapshot + metadata, serialize bằng `serde` với version. Toàn bộ mục này phải đạt 100% test coverage bằng golden test: cùng mongpack + cùng chuỗi input → cùng chuỗi event, chạy trên cả native lẫn WASM.

## 6. MộngScript (DSL) — thiết kế một lần, dùng hai nơi

DSL phục vụ người viết quen gõ text; nó chỉ là cú pháp bề mặt của IR. Nguyên tắc: đọc được như kịch bản sân khấu, mọi thứ dịch được nằm ngoài code. Phác thảo ngữ pháp (sẽ chốt chi tiết ở M2 bằng grammar `pest`):

```
@node mo_dau
@scene quan_ca_phe

  * Nắng chiều đổ nghiêng qua ô cửa kính.        # * = người dẫn truyện
  lan (vui, trái): Ơ… Minh? Lâu lắm rồi mới gặp cậu.
  > Cười và chào Lan trước       -> bat_chuyen   { thien_cam += 1 }
  > Giả vờ mải đọc sách          -> lang_tranh

@node loi_moi
  lan: Cậu… có muốn lên xem không?
  > Nhận lời                    -> ket_dep       [ thien_cam >= 1 ]
  > Từ chối khéo                -> ket_thuong
```

Editor trực quan và DSL round-trip qua IR: mở file DSL trong editor thấy node graph; sửa graph, lưu lại vẫn ra DSL diff sạch (định dạng chuẩn hoá, thứ tự ổn định) để dùng Git bình thường. Đây là lợi thế cạnh tranh trực tiếp với Ren'Py: vừa có visual editor vừa version-control được.

## 7. Hệ plugin hai tầng

Tầng một, **plugin nội dung**: viết bằng ngôn ngữ nhúng có sandbox, đóng gói trong mongpack, chạy trên mọi nền tảng — tương đương plugin JS của prototype. Bảng hook giữ nguyên tên đã kiểm chứng: `on_game_start, on_node_enter, on_line_show, on_type, on_choice_picked, on_game_end`, cộng filter `text` và API `ctx` (get/set biến, goto, play_sfx, thao tác overlay). Lỗi plugin được cô lập: bắt ở host, báo lên editor, không sập game — hành vi này đã đúng ở prototype và giữ nguyên.

Tầng hai, **plugin native**: crate Rust gắn lúc biên dịch (feature flags), cho nhu cầu nặng như minigame nhúng, codec riêng, tích hợp nền tảng. Chỉ dành cho người build engine từ source, không phân phối qua mongpack.

Ngôn ngữ nhúng cho tầng một **đã chốt: `rhai`** cho v1. Ba lý do: rhai thuần Rust nên plugin chạy được trên mọi nền tảng kể cả web ngay từ ngày đầu — hệ sinh thái chỉ hình thành khi plugin chạy ở mọi nơi; cú pháp rhai gần JavaScript, khớp với cộng đồng làm VN web-native (ba plugin mẫu của prototype là JS, port gần như cơ học); và thứ giữ chân cộng đồng là hợp đồng API ổn định chứ không phải ngôn ngữ. Ràng buộc thiết kế đi kèm: hợp đồng hook/ctx định nghĩa bằng dữ liệu (serde values), **không API nào được lộ chi tiết riêng của rhai** — để sau 1.0 cắm thêm backend Lua hoặc backend WASM đa ngôn ngữ mà plugin cũ không đổi khái niệm.

## 8. Render và chữ

`mong-render` dùng `wgpu` (Vulkan/Metal/DX12/WebGPU). **Đã chốt:** WebGL2/GLES3 là sàn bắt buộc — renderer lõi không dùng compute shader, texture tối đa 4096, sRGB; hiệu ứng nâng cao đi qua capability check, có WebGPU thì đẹp hơn, không có vẫn chạy đúng. Nhu cầu VN đơn giản về hình — sprite 2D, background, transition (fade/dissolve/slide), particle nhẹ — nên renderer là một sprite batcher + vài shader transition, không kéo cả game engine tổng quát vào (không dùng Bevy cho runtime; giữ dependency mỏng để web build nhỏ).

Chữ là phần khó nhất và là nơi Ren'Py hay lộ khuyết điểm với tiếng Việt: dùng `cosmic-text` để shaping đúng dấu tiếng Việt, CJK, và RTL; font fallback khai báo theo locale trong `mong-i18n`; typewriter chạy theo grapheme cluster chứ không theo byte (chữ "ế" không bao giờ hiện nửa chừng thành "e"). Sprite nhân vật ghép layer (thân + mặt + trang phục) như đã ghi ở mục 4, giảm khối lượng vẽ cho artist theo cấp số nhân.

## 9. Âm thanh

`kira` làm backend: ba bus mặc định `bgm / sfx / voice` với volume riêng; BGM crossfade khi đổi cảnh (prototype mới chỉ cắt cứng); loop point cho nhạc có intro. Trên web, audio chỉ khởi động sau cử chỉ người dùng đầu tiên — runtime xếp hàng lệnh phát cho tới lúc đó (prototype đã xử lý bằng catch, làm thật thì tường minh).

## 10. Nền tảng và ma trận build

| | Desktop | Web | Android | iOS |
|---|---|---|---|---|
| Cửa sổ/input | winit | canvas + wasm-bindgen | NDK ANativeWindow | CAMetalLayer qua FFI |
| Graphics | Vulkan/Metal/DX12 | WebGPU → WebGL2 | Vulkan/GLES | Metal |
| Lưu save | file trong config dir | localStorage/OPFS | app storage | app storage |
| Đóng gói | binary đơn | wasm + js + mongpack | AAB template | Xcode template |
| CI | 3 OS matrix | wasm-pack + headless test | cargo-ndk | macOS runner |

Shell mobile cố ý mỏng: chỉ tạo surface, chuyển input/lifecycle, gọi vào `mong-runtime` qua FFI — mô hình Signal app đã chứng minh ổn định trong production.

## 11. Editor và hot reload

Mộng Studio thật = Tauri app: backend Rust dùng thẳng `mong-script` + `mong-core` (lint, preview, compile là cùng một code với CLI — không bao giờ lệch), frontend kế thừa UI prototype v4 đã kiểm chứng. Chế độ dev: editor mở một kênh WebSocket tới runtime đang chạy; sửa một node → editor biên dịch lại riêng node đó → gửi patch IR → runtime thay node trong chỗ, nếu đang đứng trong node đó thì replay từ đầu node với state hiện tại. Người viết thấy thay đổi trong dưới một giây mà không mất tiến trình chơi thử — đây là "live preview thật" mà Ren'Py không có.

## 12. Bảng quyết định công nghệ và câu hỏi mở

| Hạng mục | Chọn | Lý do | Phương án dự phòng | Ghi chú |
|---|---|---|---|---|
| Ngôn ngữ lõi | Rust | native + WASM một codebase, an toàn bộ nhớ | — |
| Graphics | wgpu | phủ 4 backend, cộng đồng mạnh | fallback WebGL2 có sẵn trong wgpu |
| Text shaping | cosmic-text | shaping + fallback thuần Rust | rustybuzz trực tiếp |
| Audio | kira  | thiết kế cho game, web ổn | rodio | web dùng WebAudio sink riêng; AudioSink là trait vì vậy |
| Parser DSL | pest | grammar tách file, dễ đọc dễ test | nom |
| Script plugin | **rhai (đã chốt)** | thuần Rust chạy WASM ngay; cú pháp gần JS hợp cộng đồng web-native; ABI trung lập | thêm backend Lua/WASM sau 1.0, không phá hợp đồng |
| Editor shell | Tauri | tái dùng UI web, backend Rust chung code | egui (viết lại UI toàn bộ) |
| Nén mongpack | zstd | tỉ lệ/tốc độ tốt | lz4 nếu ưu tiên tốc độ nạp |

Bốn câu hỏi mở của bản 0.1 **đã được chốt** theo nguyên tắc chung "rào cản gia nhập thấp nhất cho người viết, artist và người viết plugin; độ phức tạp giấu vào bước build": (1) plugin dùng rhai với ABI trung lập ngôn ngữ (chi tiết ở mục 7); (2) key bảng chuỗi tự sinh ổn định, ẩn khỏi tác giả, `@key` khi cần đặt tên (mục 4); (3) sprite layer: tác giả dùng PNG rời theo quy ước thư mục, atlas sinh tự động lúc pack (mục 4); (4) WebGL2 là sàn, lõi không compute shader (mục 8). Các quyết định này khoá hợp đồng cho M0–M5; muốn đổi phải qua RFC và tăng formatVersion.

## 13. Lộ trình milestone

Mỗi milestone có "định nghĩa hoàn thành" (DoD) kiểm chứng được; không sang mốc sau khi mốc trước chưa xanh CI.

| Mốc | Nội dung | DoD |
|---|---|---|
| M0 | Chốt spec IR + format; dựng workspace, CI 3 OS + wasm | Tài liệu này duyệt xong; `cargo test` xanh trên CI; mongpack đọc/ghi round-trip |
| M1 | `mong-core` hoàn chỉnh + **text-mode runner** | Truyện demo "Quán cà phê" (port từ prototype) chơi được trong terminal; golden test rollback/save; fuzz input không panic |
| M2 | `mong-script`: DSL ↔ IR ↔ JSON round-trip | Demo viết lại bằng DSL, diff ổn định; lint bắt đủ các lỗi mà prototype v4 bắt |
| M3 | Render + audio, chạy desktop | Demo chạy 60fps cửa sổ desktop; tiếng Việt hiển thị đúng grapheme; transition fade |
| M4 | Web WASM | Cùng demo chạy Chrome/Firefox/Safari; bundle < 5 MB gzip chưa tính assets |
| M5 | Plugin host | Port 3 plugin mẫu của prototype (chèn biến, rung, gõ chữ) chạy cả desktop lẫn web |
| M6 | Editor Tauri + hot reload | Sửa thoại thấy thay đổi < 1s không restart; xuất mongpack từ editor |
| M7 | Shell Android + iOS | Demo cài và chơi trên thiết bị thật; save/load hoạt động |

Ước lượng thô cho một người làm bán thời gian: M0–M1 là nền móng quyết định, đáng dành 3–4 tuần không vội; M2 2 tuần; M3 3–4 tuần (text shaping ngốn thời gian); M4 1–2 tuần nếu M3 kỷ luật; M5 2 tuần; M6 3 tuần; M7 2–3 tuần. Tổng cỡ 4–5 tháng lịch — con số thực tế cho một engine nghiêm túc, và mỗi mốc đều ra được thứ chạy được để giữ động lực.

## 14. Rủi ro chính và cách giảm

Rủi ro lớn nhất là **phạm vi phình** — engine là hố đen thời gian; phòng bằng DoD cứng cho từng mốc và danh sách "không làm ở v1" ở mục 1. Thứ hai là **WebGPU trên Safari** chưa phủ hết — phòng bằng fallback WebGL2 bắt buộc từ M3, test Safari trong CI của M4. Thứ ba là **scripting trên WASM** — phòng bằng quyết định rhai-trước và API host trung lập. Thứ tư là **text shaping tiếng Việt/CJK** nhiều ca hiểm — phòng bằng bộ test chuỗi hiểm (dấu chồng, emoji, RTL trộn) viết ngay ở M3, và text-mode runner của M1 giúp mọi bug chữ chỉ nằm ở tầng render, không lẫn vào logic.

---

*Bước tiếp theo: khởi động M0 — dựng workspace skeleton, CI, và viết spec IR chi tiết (mỗi lệnh một trang: cú pháp, ngữ nghĩa, ví dụ, test case).*
