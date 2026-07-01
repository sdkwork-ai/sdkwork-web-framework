# Release-profile pipeline p99 benchmark (maturity §3.1: p99 < 0.5ms).
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
Set-Location (Split-Path $PSScriptRoot -Parent)
Write-Host "Running release pipeline benchmark..."
cargo test --release -p sdkwork-web-architecture-tests --test pipeline_benchmark
exit $LASTEXITCODE
