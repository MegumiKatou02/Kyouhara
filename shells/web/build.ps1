# Build shell web: cargo -> wasm-bindgen -> do gzip, chan tran 5 MB (DoD M4).
#
# Khong wasm-pack: no ap bo cuc npm khong can thiet va giau mat buoc do size.
# Xem docs/m4-ket-thuc.md.

$ErrorActionPreference = "Stop"

Push-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "cargo build hong" }
Pop-Location

wasm-bindgen "$PSScriptRoot\target\wasm32-unknown-unknown\release\mong_web.wasm" `
    --target web --out-dir "$PSScriptRoot\dist"
if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen hong" }

$wasm = "$PSScriptRoot\dist\mong_web_bg.wasm"
Write-Host ("sau cargo     : {0,12:N0} B" -f (Get-Item $wasm).Length)

# wasm-opt: CO CAI cung khong chay. Do ngay 2026-07-10:
#   khong wasm-opt : 3.909.604 tho -> 1.320.348 gzip
#   -Oz            : 3.457.034 tho -> 1.351.300 gzip  (+31 KB)
#   -Os            : 3.510.906 tho -> 1.349.567 gzip  (+29 KB)
# Cat 11% byte tho nhung xao tron code, deflate an it di. Thu nguoi choi tai
# la gzip. Do lai khi ngan sach chat (hien con 74% tran), va do bang gzip:
#   wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int `
#       $wasm -o "$wasm.opt"
# (hai co --enable bat buoc: rustc bat bulk-memory mac dinh cho wasm32, con
#  wasm-opt gia dinh wasm MVP 2017 va tu choi memory.copy.)

# Do dung thu trinh duyet tai. Bang .NET, khong can cai gi.
$bytes = [IO.File]::ReadAllBytes($wasm)
$ms = New-Object IO.MemoryStream
# leaveOpen: true — Close() cua GZipStream dong luon $ms roi .Length nem.
$gz = New-Object IO.Compression.GZipStream($ms, [IO.Compression.CompressionLevel]::Optimal, $true)
$gz.Write($bytes, 0, $bytes.Length)
$gz.Dispose()   # flush block cuoi; khong Dispose thi thieu vai KB
$gzip = $ms.Length
$ms.Dispose()
if ($gzip -le 0) { throw "khong do duoc gzip" }

$tran = 5MB
Write-Host ("gzip          : {0,12:N0} B  ({1:P0} cua tran {2:N0})" -f $gzip, ($gzip / $tran), $tran)
if ($gzip -gt $tran) { throw "vuot ngan sach bundle (DoD M4)" }
