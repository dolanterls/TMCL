$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$metadata = cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $repoRoot "Cargo.toml") | ConvertFrom-Json
$version = $metadata.packages[0].version
$distRoot = Join-Path $repoRoot "dist"
$packageRoot = Join-Path $distRoot "tmcl-$version-windows-x86_64"
$archive = Join-Path $distRoot "tmcl-$version-windows-x86_64.zip"

if (-not (Test-Path -LiteralPath $distRoot)) {
    New-Item -ItemType Directory -Path $distRoot | Out-Null
}
if (Test-Path -LiteralPath $packageRoot) {
    Remove-Item -LiteralPath $packageRoot -Recurse -Force
}
if (Test-Path -LiteralPath $archive) {
    Remove-Item -LiteralPath $archive -Force
}

cargo build --release --target x86_64-pc-windows-msvc --manifest-path (Join-Path $repoRoot "Cargo.toml")
New-Item -ItemType Directory -Path $packageRoot | Out-Null
# `target/` 不在仓库里了（见 .cargo/config.toml），产物根目录以 cargo 为准。
$targetRoot = $metadata.target_directory
Copy-Item -LiteralPath (Join-Path $targetRoot "x86_64-pc-windows-msvc\release\tmcl.exe") -Destination $packageRoot
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination $packageRoot
Copy-Item -LiteralPath (Join-Path $repoRoot "LICENSE") -Destination $packageRoot
Compress-Archive -Path (Join-Path $packageRoot "*") -DestinationPath $archive

Write-Output "Created $archive"
