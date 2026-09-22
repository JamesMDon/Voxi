#Requires -Version 7.0
$ErrorActionPreference = 'Stop'

$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$target = 'x86_64-pc-windows-msvc'
Push-Location -LiteralPath $projectRoot
try {
    $metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Unable to read Cargo metadata.' }
    $version = ($metadata.packages | Where-Object name -eq 'Voxi').version
    if (-not $version) { throw 'Voxi package version is missing.' }

    cargo build --release --locked --target $target
    if ($LASTEXITCODE -ne 0) { throw 'Voxi release build failed.' }
} finally {
    Pop-Location
}

$executable = Join-Path $metadata.target_directory "$target/release/Voxi.exe"
$distPath = Join-Path $projectRoot 'dist'
$packagePath = Join-Path $distPath "voxi-$version-windows-x64.zip"
$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$stagingPath = Join-Path $tempRoot ("voxi-package-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $distPath -Force | Out-Null
New-Item -ItemType Directory -Path $stagingPath | Out-Null

try {
    Copy-Item -LiteralPath $executable -Destination $stagingPath
    foreach ($file in @('README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination $stagingPath
    }
    foreach ($directory in @('scripts', 'assets', 'licenses')) {
        New-Item -ItemType Directory -Path (Join-Path $stagingPath $directory) | Out-Null
    }
    foreach ($file in @(
        'scripts/setup-guy.ps1',
        'assets/natural-voice-adapter.manifest',
        'assets/voxi-readme.svg',
        'licenses/NaturalVoiceSAPIAdapter.txt'
    )) {
        Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination (Join-Path $stagingPath $file)
    }
    Compress-Archive -Path (Join-Path $stagingPath '*') -DestinationPath $packagePath -CompressionLevel Optimal -Force
} finally {
    $resolvedStaging = (Resolve-Path -LiteralPath $stagingPath).Path
    $resolvedTemp = (Resolve-Path -LiteralPath $tempRoot).Path
    if ((Split-Path -Parent $resolvedStaging) -ne $resolvedTemp.TrimEnd('\', '/')) {
        throw "Refusing to remove staging path outside the temporary directory: $resolvedStaging"
    }
    Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
}

$hash = Get-FileHash -LiteralPath $packagePath -Algorithm SHA256
"$($hash.Hash.ToLowerInvariant())  $([System.IO.Path]::GetFileName($packagePath))" |
    Set-Content -LiteralPath "$packagePath.sha256" -Encoding ascii
Write-Host "Packaged $packagePath"
