$ErrorActionPreference = "Stop"

Push-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "cargo build hong" }
Pop-Location

wasm-bindgen "$PSScriptRoot\target\wasm32-unknown-unknown\release\mong_web.wasm" `
    --target web --no-typescript --remove-name-section --remove-producers-section `
    --out-dir "$PSScriptRoot\dist"

$wasm = "$PSScriptRoot\dist\mong_web_bg.wasm"
Write-Host ("sau cargo : {0,12:N0} B" -f (Get-Item $wasm).Length)

if (Get-Command wasm-opt -ErrorAction SilentlyContinue) {
    wasm-opt -Oz $wasm -o $wasm
    Write-Host ("sau  wasm-opt : {0,12:N0} B" -f (Get-Item $wasm).Length)
} else {
    Write-Host "wasm-opt chua cai — bo qua (binaryen)"
}

$bytes = [IO.File]::ReadAllBytes($wasm)
$ms = New-Object IO.MemoryStream
# leaveOpen: true — Close() cua GZipStream se dong luon $ms, roi .Length nem.
$gz = New-Object IO.Compression.GZipStream($ms, [IO.Compression.CompressionLevel]::Optimal, $true)
$gz.Write($bytes, 0, $bytes.Length)
$gz.Dispose()          # flush block cuoi; khong Dispose thi thieu vai KB
$gzip = $ms.Length
if ($gzip -le 0) { throw "khong do duoc gzip" }
$ms.Dispose()

$tran = 5MB
Write-Host ("gzip  : {0,12:N0} B  ({1:P0} cua tran {2:N0})" -f $gzip, ($gzip / $tran), $tran)
if ($gzip -gt $tran) { throw "vuot ngan sach bundle (DoD M4)" }
