# Builds XeItems and optionally installs it on a server.
#   .\build.ps1                                    build only
#   .\build.ps1 -Server D:\Documents\Server\plugins build + copy XeItems.wasm there
param([string]$Server)

# No "Stop" error preference: Windows PowerShell 5.1 treats cargo's progress output
# (written to stderr) as errors. Cargo's exit code is checked instead.
Push-Location $PSScriptRoot
try {
    # -j 1: building in parallel can run a small PC out of memory.
    cargo build --release -j 1
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
} finally {
    Pop-Location
}

$wasm = Join-Path $PSScriptRoot "target\wasm32-wasip2\release\xe_items.wasm"
$kb = [math]::Round((Get-Item $wasm).Length / 1KB)
if ($Server) {
    Copy-Item $wasm (Join-Path $Server "XeItems.wasm") -Force
    "Installed XeItems.wasm ($kb KB) in $Server"
} else {
    "Built $wasm ($kb KB)"
}
