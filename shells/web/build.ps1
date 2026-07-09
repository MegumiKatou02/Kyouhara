# Ba bước tường minh, đo được từng bước. Không wasm-pack (xem docs/m4-ket-thuc.md).
$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot\..\.."

Push-Location $PSScriptRoot
cargo build --release
Pop-Location
wasm-bindgen "$PSScriptRoot\target\wasm32-unknown-unknown\release\mong_web.wasm" `
    --target web --out-dir "$PSScriptRoot\dist"

$wasm = "$PSScriptRoot\dist\mong_web_bg.wasm"
Write-Host ("truoc wasm-opt : {0:N0} B" -f (Get-Item $wasm).Length)
if (Get-Command wasm-opt -ErrorAction SilentlyContinue) {
    wasm-opt -Oz $wasm -o $wasm
    Write-Host ("sau  wasm-opt : {0:N0} B" -f (Get-Item $wasm).Length)
}
