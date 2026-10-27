# Builds the browser version into dist/web: the shared UI from app/ui, the web-only files from
# web/www and the WebAssembly from this crate. Needs the wasm32-unknown-unknown target and
# wasm-bindgen-cli at the same version as the wasm-bindgen crate in Cargo.lock.
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root "dist\web"

cargo build --profile web --target wasm32-unknown-unknown -p exhibit-web --manifest-path (Join-Path $root "Cargo.toml")
if ($LASTEXITCODE) { exit $LASTEXITCODE }

if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory $out | Out-Null
wasm-bindgen --target web --no-typescript --out-dir (Join-Path $out "pkg") `
  (Join-Path $root "target\wasm32-unknown-unknown\web\exhibit_web.wasm")
if ($LASTEXITCODE) { exit $LASTEXITCODE }

Copy-Item (Join-Path $root "app\ui\*") $out -Recurse
Copy-Item (Join-Path $PSScriptRoot "www\*") $out -Recurse

# Only the browser build is installable, so only it links the manifest.
$index = Join-Path $out "index.html"
$html = [IO.File]::ReadAllText($index)
$html = $html.Replace('<link rel="stylesheet" href="styles.css" />',
  "<link rel=`"stylesheet`" href=`"styles.css`" />`n  <link rel=`"manifest`" href=`"manifest.webmanifest`" />`n  <meta name=`"viewport`" content=`"width=device-width, initial-scale=1`" />")
[IO.File]::WriteAllText($index, $html)
Write-Host "Built $out"
