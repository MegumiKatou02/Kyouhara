# Spec MộngScript (DSL) — Mộng Engine (v1, bản nháp M2)

**Trạng thái: ĐÃ DUYỆT** (các quyết định ⚠ chốt 2026-07-08, ghi ở mục 12).
Tài liệu này là hợp đồng cú pháp bề mặt của IR (spec-ir.md); parser (pest)
và formatter phải tuân theo đúng từng quy tắc ở đây, mỗi quy tắc có test
tương ứng trong `mong-script`. Muốn đổi phải qua RFC như spec-ir.

Nguồn gốc: phác thảo ở mục 6 tài liệu thiết kế ("sẽ chốt chi tiết ở M2").
DSL không thêm ngữ nghĩa nào — mọi ngữ nghĩa thuộc spec-ir.md; tài liệu này
chỉ định nghĩa cách viết text và ánh xạ hai chiều DSL ↔ IR.

## 1. Nguyên tắc thiết kế

1. **Đọc như kịch bản sân khấu**: lệnh xuất hiện dày (thoại, dẫn truyện,
   lựa chọn, gán biến, rẽ nhánh) dùng ký hiệu ngắn (`*`, `tên:`, `>`, `~`,
   `?`); lệnh hiếm (điều hướng, trình diễn) dùng từ khoá tiếng Anh trùng tên
   lệnh IR (`jump`, `call`, `wait`, …) — ánh xạ 1:1, khỏi học hai bộ tên.
2. **Round-trip là bất biến số một**: mọi IR hợp lệ in ra DSL rồi parse lại
   phải ra đúng IR đó; format lại một file đã format phải ra byte-giống-hệt.
3. **Diff ổn định hơn đẹp mắt** ⚠: formatter *không* canh cột (khác hình
   minh hoạ ở mục 6 tài liệu thiết kế — canh cột làm sửa một dòng chạm lây
   các dòng hàng xóm, phá tiêu chí "diff sạch để dùng Git").
4. DSL chỉ biểu diễn **nodes**. Metadata dự án (title, locales, variables,
   startNode, characters, scenes) vẫn nằm trong `project.json` — DSL không
   nhân bản chúng.

## 2. File và cấu trúc node

- Đuôi file: `.mongscript`, UTF-8, xuống dòng LF (formatter ép LF).
- Một file = một dãy node. Node bắt đầu bằng directive `@node`:

```
@node mo_dau
@title Mở đầu
@scene quan_ca_phe

  <các lệnh, indent 2 space>
```

- `@node <id>` — bắt buộc, mở node mới. `id` là định danh
  (`[A-Za-z_][A-Za-z0-9_]*`).
- `@title <text đến hết dòng>` — tuỳ chọn, map vào `Node.title`
  (vắng → chuỗi rỗng).
- `@scene <id>` — tuỳ chọn, map vào `Node.scene`.
- Thân node: mỗi lệnh một dòng, indent 2 space (formatter ép; parser
  khoan dung — indent chỉ mang tính trình bày, không mang ngữ nghĩa,
  trừ block `?` dùng ngoặc nhọn nên cũng không cần indent-nghĩa).

## 3. Các lệnh — cú pháp và ánh xạ IR

### 3.1 Dẫn truyện và thoại → `say`

```
* Nắng chiều đổ nghiêng qua ô cửa kính.            #~ mo_dau.l1
lan (vui, left): Ơ… Minh? Lâu lắm rồi mới gặp cậu.  #~ mo_dau.l2
lan (vui, left, sfx=ding, exit): …                  #~ mo_dau.l3
```

- `*` = không speaker (người dẫn truyện) → `say {speaker: None}`.
      Dòng `*` cũng nhận `( )` opts như dialogue (`* (sfx=ding) Văn bản…`)
      vì IR cho phép `say {speaker: None, opts}` — vd. sfx đệm cho một
      nhịp dẫn truyện.
- `tên:` = speaker là id nhân vật. Ngoặc tròn tuỳ chọn chứa opts:
  hai vị trí đầu là `pose`, `pos` (positional); sau đó là mục có tên
  `sfx=<id>` và cờ `exit`. Thứ tự mục có tên: formatter luôn in
  `pose, pos, sfx=…, exit` — thiếu mục nào bỏ qua mục đó.
- Văn bản chạy đến hết dòng (trừ comment key `#~ …` ở đuôi, xem mục 6).
- Ánh xạ: văn bản hiển thị **không nằm trong IR** — IR chỉ giữ
  `string_key`; văn bản đi vào bảng chuỗi của `defaultLocale`
  (xem mục 6 về key và mục 9 về pipeline).

### 3.2 Lựa chọn → `choice`

Các dòng `>` **liên tiếp** gộp thành một lệnh `choice` (dòng trống hoặc lệnh
khác cắt nhóm — hai nhóm `>` rời nhau là hai lệnh choice, giữ đúng ngữ nghĩa
IR "lệnh sau choice trong cùng block là bất khả đạt", lint sẽ cảnh báo).

```
> Cười và chào Lan trước -> bat_chuyen { thien_cam += 1 }   #~ mo_dau.c1
> Giả vờ mải đọc sách -> lang_tranh                          #~ mo_dau.c2
> Đứng dậy ra về [ met >= 3 ]                                #~ mo_dau.c3
```

Một arm, theo thứ tự cố định: `> <label>` rồi tuỳ chọn `[ cond ]`, rồi
tuỳ chọn `-> <target>`, rồi tuỳ chọn `{ effects }`. ⚠ Formatter in theo thứ
tự chuẩn `label [cond] -> target {effects}`; parser chấp nhận `[ ]`/`{ }`
đứng sau `->` lẫn trước (khoan dung khi đọc, chuẩn hoá khi ghi).

- **Vắng `->`** = `target: None` = kết thúc truyện khi chọn (đúng ngữ nghĩa
  spec-ir). Lint cảnh báo mức Warning để tác giả không quên vô ý.
- `[ cond ]`: một điều kiện theo cú pháp mục 4. IR v1 chỉ có một `Cond` mỗi
  arm — DSL không cho `&&`/`||` (đúng phạm vi IR, không thêm).
- `{ effects }`: danh sách phép ghi cách nhau `;`, mỗi phép theo cú pháp
  Effect ở mục 4: `{ thien_cam += 1; da_gap = true }`.

### 3.3 Gán biến → `set` / `set_expr`

Sigil `~`:

```
~ diem = 0              → set  {assign}
~ diem += 1             → set  {add}      (vế phải là literal Int)
~ diem -= 2             → set  {sub}
~ !da_gap               → set  {toggle}
~ tong = diem * 2 + du  → set_expr        (vế phải là biểu thức)
~ diem += thuong        → set_expr: diem = diem + thuong
```

Quy tắc chọn lệnh IR (xác định, để round-trip ổn định): vế phải là **literal
đơn** → `set` với op tương ứng; có bất kỳ toán tử/biến nào → `set_expr`
(`+=`/`-=` khai triển thành `var = var + (expr)`). Chiều ngược (IR → DSL): 
`set` in đúng dạng ngắn; `set_expr` dạng
`var = var ± rhs` in thành `var ±= rhs` khi khớp mẫu **trừ khi** rhs là
literal Int (khi đó phải in dạng dài `var = var ± n` — dạng ngắn sẽ bị
parse ngược thành `set {add/sub}`); còn lại in nguyên biểu thức.
`Neg` luôn in `-( … )` (nghịch đảo quy ước "`-5` là literal âm", mục 4).

### 3.4 Rẽ nhánh → `if`

```
? thien_cam >= 1 {
  lan: Tớ biết cậu sẽ nhận lời mà.   #~ loi_moi.l3
} : {
  * Lan hơi cụp mắt.                  #~ loi_moi.l4
}
```

`? <cond> { … }` với `: { … }` tuỳ chọn cho nhánh else. Lồng tuỳ ý.
Ngoặc nhọn bắt buộc (kể cả một lệnh) — đổi lấy grammar đơn giản và
formatter không có ca đặc biệt.

### 3.5 Điều hướng và nhãn — từ khoá, 1:1 với IR

```
jump ket_dep          → jump {target}
call canh_mua         → call {target}
return                → return
label cho_lai         → label {name}
goto cho_lai          → goto {label}
end                   → end
```

### 3.6 Trình diễn và tiện ích — từ khoá, 1:1 với IR

```
scene san_thuong fade     → scene {scene, transition: "fade"}   (transition tuỳ chọn)
show lan vui left         → show {char, pose, pos}
hide lan                  → hide {char}
wait 500                  → wait {ms}
sfx chuong                → sfx {id}
bgm mua_dem               → bgm {id}
bgm                       → bgm {id: None}   (tắt nhạc)
rand may 1 6              → rand {var, min, max}
ext rung {"manh": 3}      → ext {cmd, args}  (args là một object JSON nguyên văn)
```

`ext` giữ args dạng JSON inline: plugin tự định nghĩa schema, DSL không đoán —
formatter in lại JSON chuẩn hoá (serde_json, key theo thứ tự khai báo trong IR).

## 4. Biểu thức và điều kiện (cú pháp text cho `Expr`/`Cond`)

Đây là phần spec-ir.md uỷ quyền cho M2 ("cú pháp text là việc của DSL").

- **Literal**: Int (`-?[0-9]+`), Bool (`true`/`false`), Str (`"…"` với escape
  `\"` `\\` `\n`).
- **Expr** (chỉ dùng trong `set_expr`): `+ - * / %`, một ngôi `-`, ngoặc
  `( )`. Ưu tiên chuẩn: `- (một ngôi)` > `* / %` > `+ -`; kết hợp trái.
  `%` map vào `rem`. Chỉ số học Int — đúng phạm vi IR.
- **Cond** (trong `[ ]` của choice và sau `?`): `var >= v`, `var <= v`,
  `var == v`, `var != v` — đúng bốn op `ge/le/eq/ne` của IR. **Không có**
  `>` `<` `&&` `||` (IR không có thì DSL không có; thêm sau qua RFC).
- **Effect** (trong `{ }` của choice): `var = <literal>`, `var += <Int>`,
  `var -= <Int>`, `!var` — đúng bốn op `assign/add/sub/toggle`. Trong
  effects của choice, vế phải bắt buộc literal (IR `Effect.value` là
  `Value`, không phải `Expr`).

## 5. Comment và escape

- `#` mở comment đến hết dòng (trừ `#~` là key, mục 6). Trong *văn bản*
  thoại/label, `#` phải viết `\#`; formatter tự escape khi in.
- Văn bản **dẫn truyện** mở đầu bằng `(` phải viết `\(` để không bị đọc
      nhầm thành opts của dòng `*` (vd. kịch bản kiểu "(sigh) He left.");
      formatter tự escape. Chỉ có nghĩa ở đầu văn bản; thoại có speaker
      không cần (opts đứng trước dấu `:`).
- Comment tự do được **bảo toàn qua round-trip**: parser gắn comment vào
  lệnh cùng dòng (trailing) hoặc lệnh ngay sau (leading); formatter in lại
  đúng vị trí. Đây là điều kiện để tác giả dám format toàn dự án.
  (Ghi chú triển khai: comment sống trong cây cú pháp phía mong-script,
  **không** vào IR — IR không đổi.)

## 6. Key bảng chuỗi — `#~` ⚠

Hiện thân trong DSL của cơ chế key đã chốt ở mục 4 tài liệu thiết kế
(tự sinh một lần, bền vững, `@key` khi cần tên riêng):

```
lan: Ơ… Minh?                    #~ mo_dau.l2      ← key tự sinh, formatter quản
> Nhận lời -> ket_dep            #~ loi_moi.c1
lan: Cảm ơn cậu.                 #~ cam_on_1       ← tác giả tự đặt (= @key)
```

- Mỗi dòng mang văn bản dịch được (say, arm của choice) có đuôi `#~ <key>`.
- Dòng **mới chưa có** `#~`: lần format/compile đầu tiên sinh key mới
  (dạng `<node_id>.l<n>` / `.c<n>` với `n` = bộ đếm chưa dùng trong node —
  key là **định danh mờ**, không mang nghĩa vị trí; dòng chuyển node khác
  vẫn giữ key cũ).
- Key **sống cùng dòng** trong file text → đảo thứ tự, sửa nội dung, cắt dán
  giữa node đều không mất key — thoả "không đổi khi sửa nội dung/đảo thứ tự"
  kể cả khi tác giả sửa bằng editor text bất kỳ, không cần sidecar.
- Tác giả sửa key thành tên riêng = cơ chế `@key ten_rieng` của spec.
- Lint: key trùng trong toàn dự án = Error; key mồ côi trong bảng chuỗi
  (không dòng nào tham chiếu) = Warning.
- ⚠ **Nới câu chữ mục 4**: "editor và DSL ẩn key" hiểu là *tác giả không
  phải quản lý key* (formatter tự sinh/tự dọn), không phải *key vô hình
  trong file text* — vô hình + bền vững + sửa-bằng-tool-bất-kỳ là bộ ba
  không thể đồng thời. Editor trực quan (M6) vẫn ẩn key hoàn toàn đúng
  câu chữ. Cần bạn duyệt cách hiểu này.

## 7. Pipeline biên dịch và bảng chuỗi

`parse(.mongscript, bảng chuỗi defaultLocale) → (IR, bảng chuỗi cập nhật)`:

1. Parse ra cây cú pháp (giữ span + comment).
2. Sinh key cho dòng thiếu `#~` (mục 6) — bước duy nhất *ghi ngược* vào
   file nguồn, gộp chung với format.
3. Văn bản trên dòng → entry `key → text` của `defaultLocale`; IR chỉ giữ key.
4. Locale khác không bị đụng: key mới xuất hiện ở locale khác dưới dạng
   thiếu — việc của mong-i18n (fallback về defaultLocale) và của quy trình
   dịch (xliff/po, ngoài phạm vi M2).

Chiều ngược `print(IR, bảng chuỗi defaultLocale) → .mongscript` tra key ra
văn bản; key không có trong bảng chuỗi → lỗi (không in file thiếu thoại).

## 8. Formatter — quy tắc chuẩn hoá (điều kiện "diff ổn định")

1. Indent 2 space trong thân node; thân nhánh `?` thêm 2 space mỗi mức.
2. **Không canh cột**: đúng một space quanh mọi token (`->`, `[ ]`, `{ }`,
   toán tử); `#~` cách văn bản đúng 2 space.
3. Một dòng trống giữa hai node; một dòng trống giữa header (`@node/@title/
   @scene`) và thân; không dòng trống thừa liên tiếp (tối đa 1 trong thân).
4. Thứ tự trường cố định: opts của say (`pose, pos, sfx=…, exit`); arm của
   choice (`label [cond] -> target {effects}`); effects giữ thứ tự khai báo
   (IR áp effects theo thứ tự — không được sort).
5. Node in theo thứ tự trong `Story` (thứ tự là dữ liệu, không sort).
6. LF, không space cuối dòng, file kết thúc bằng đúng một LF.
7. Chuẩn idempotent: `fmt(fmt(x)) == fmt(x)` — có property test.

## 9. Bất biến round-trip và chiến lược test

| # | Bất biến | Test |
|---|---|---|
| 1 | `parse(print(ir)) == ir` với mọi IR hợp lệ | property test (proptest sinh Story ngẫu nhiên hợp lệ) + golden trên demo |
| 2 | `fmt` idempotent trên mọi input parse được | property + golden |
| 3 | DSL → IR ≡ JSON → IR trên demo "Quán cà phê" | golden: hai đường nạp, so `Story` bằng `==` |
| 4 | Key bền vững: đảo node/đảo dòng/sửa text rồi re-compile → key không đổi | unit test theo kịch bản sửa |
| 5 | Comment bảo toàn qua fmt | golden |
| 6 | Lỗi cú pháp báo đúng dòng/cột, không panic | unit + đưa parser vào fuzz-lite hiện có |

DoD M2 đối chiếu: demo viết lại bằng DSL (bất biến 3), diff ổn định (mục 8 +
bất biến 2), lint đủ luật prototype v4 (checklist riêng — việc của bước 5
trong kế hoạch M2, không thuộc tài liệu này).

## 10. Grammar pest — khung

```pest
file       =  { SOI ~ node* ~ EOI }
node       =  { node_hdr ~ title_hdr? ~ scene_hdr? ~ stmt* }
node_hdr   =  { "@node" ~ ident }
title_hdr  =  { "@title" ~ text_line }
scene_hdr  =  { "@scene" ~ ident }

stmt       = _{ narrate | dialogue | choice_arm | set_stmt | if_stmt | kw_stmt }
narrate    =  { "*" ~ text ~ key_tag? }
dialogue   =  { ident ~ say_opts? ~ ":" ~ text ~ key_tag? }
say_opts   =  { "(" ~ opt_item ~ ("," ~ opt_item)* ~ ")" }
choice_arm =  { ">" ~ text ~ key_tag? ~ cond_tag? ~ arrow_tag? ~ effects_tag? }
set_stmt   =  { "~" ~ ("!" ~ ident | ident ~ set_op ~ expr) }
if_stmt    =  { "?" ~ cond ~ block ~ (":" ~ block)? }
kw_stmt    =  { jump | call | ret | label_s | goto_s | scene_s | show_s
              | hide_s | wait_s | sfx_s | bgm_s | rand_s | ext_s | end_s }

cond       =  { ident ~ cond_op ~ value }
cond_op    =  { ">=" | "<=" | "==" | "!=" }
expr       =  { term ~ (add_op ~ term)* }          // ưu tiên xử lý bằng PrattParser
key_tag    =  { "#~" ~ ident_dotted }
COMMENT    =  { "#" ~ !"~" ~ (!NEWLINE ~ ANY)* }   // giữ lại, không silent-drop
```

(Khung minh hoạ hình dạng grammar; file `mongscript.pest` thật là sản phẩm
của bước 2, mỗi rule có test.)

## 11. Tác động lên format dữ liệu

- IR và `.mongpack`: **không đổi** — DSL là frontend mới, cùng IR,
  formatVersion giữ nguyên. M2 không cần migration nào.
- `project.json` (mục 4 tài liệu thiết kế) **chưa tồn tại** — M0–M1 mới
  hiện thực phần `Story` (fixture `demo-story.json`). Không khai sinh nó
  ở M2 chỉ để chứa `scripts[]`; nó ra đời ở M3 cùng Character/Scene/Asset,
  và trường `scripts[]` nằm trong schema ngay từ bản đầu.
- Quy ước M2 (tạm, cho tới khi có project.json): `mong-cli run/lint` nhận
  thẳng file `.mongscript` (song song JSON dự án và `.mongpack` như hiện
  tại, phân biệt qua đuôi file); bảng chuỗi là sidecar
  `<tên>.strings.<locale>.json` cạnh file script — bước compile ghi chuỗi
  `defaultLocale` vào đó (mục 7), mong-i18n nạp các locale còn lại và
  fallback. Cờ `--strings` của M1 bị loại bỏ.
- Metadata Story mà DSL không biểu diễn (title, locales, variables, start):
  thêm directive cấp file ở đầu `.mongscript`, trước node đầu tiên —
  `@story <title đến hết dòng>`, `@locale vi [en ja]` (đầu tiên là default),
  `@var <tên> = <literal>` (mỗi biến một dòng), `@start <node_id>` (vắng →
  node đầu tiên trong file). Chỉ là cách chở các trường `Story` sẵn có
  sang text — không thêm khái niệm mới vào IR.

## 12. Biên bản quyết định (chốt 2026-07-08)

1. **Chốt** — key inline `#~`; "DSL ẩn key" hiểu là tác giả không phải
   quản lý key (mục 6).
2. **Chốt** — formatter không canh cột (mục 1.3, 8.2).
3. **Chốt** — toàn bộ từ khoá DSL dùng tiếng Anh chuẩn, ký hiệu toán
   học/điều hướng tự nhiên, hướng tới thị trường quốc tế (mục 1.1, 3, 4).
   Pose/pos/id là *nội dung của tác giả*, DSL chuyển tiếp nguyên vẹn —
   viết tiếng gì tuỳ dự án.
4. **Chốt** — arm vắng `->` = `target: None`, VM kết thúc mạch truyện hợp
   lệ; lint bắt buộc phát Warning để tránh kết thúc đột ngột do gõ thiếu
   (mục 3.2).
5. **Chốt** — cặp `{ }` bắt buộc cho mọi block của `?` (mục 3.4).
6. **Chốt (điều chỉnh)** — không đổi format nào ở M2; project.json hoãn
   tới M3, M2 dùng `.mongscript` trực tiếp + sidecar strings (mục 11).
