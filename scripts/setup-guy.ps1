[CmdletBinding()]
param(
    [string]$Destination,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$adapterUri = 'https://github.com/gexgd0419/NaturalVoiceSAPIAdapter/releases/download/v0.2.9/NaturalVoiceSAPIAdapter_v0.2.9_x86_x64.zip'
$adapterSha256 = '7129D8675925E5A141ADDD820CE55B2EA4AF0708C901E3A3E8E225E1CE15B4CE'
$voiceUri = 'https://download.microsoft.com/download/b/0/8/b08754b2-f8aa-4cde-b2de-baad2cf49fdc/Voice.en-US.cab'
$voiceSha256 = '33BD4650BD715D080867447D651F8EE15FB122C8ADE7454032FE468E085C7A23'

$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($Destination)) {
    $Destination = Join-Path $projectRoot 'runtime\natural'
}
$destinationPath = [System.IO.Path]::GetFullPath($Destination)
$manifestPath = Join-Path $projectRoot 'assets\natural-voice-adapter.manifest'

function Get-VerifiedDownload {
    param(
        [Parameter(Mandatory)] [string]$Uri,
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string]$Sha256
    )

    Invoke-WebRequest -Uri $Uri -OutFile $Path
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash
    if ($actual -ne $Sha256) {
        throw "Download hash mismatch for $Uri"
    }
}

$requiredFiles = @(
    (Join-Path $destinationPath 'adapter.manifest'),
    (Join-Path $destinationPath 'NaturalVoiceSAPIAdapter.dll'),
    (Join-Path $destinationPath 'NarratorVoices\Guy\MSTTSLocEnUS.dat')
)
if (-not $Force -and ($requiredFiles | Where-Object { -not (Test-Path -LiteralPath $_) }).Count -eq 0) {
    Write-Host "Microsoft Guy is already ready at $destinationPath"
    exit 0
}
if ((Test-Path -LiteralPath $destinationPath) -and -not $Force) {
    throw "The destination exists but is incomplete. Rerun with -Force to replace it."
}
if (Get-Process -Name Voxi -ErrorAction SilentlyContinue) {
    throw 'Exit Voxi before installing or replacing Microsoft Guy.'
}
if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'The bundled Voxi setup currently supports only 64-bit Windows.'
}

$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("Voxi-Guy-" + [guid]::NewGuid().ToString('N'))
$adapterZip = Join-Path $workDir 'adapter.zip'
$adapterDir = Join-Path $workDir 'adapter'
$voiceCab = Join-Path $workDir 'voice.cab'
$cabDir = Join-Path $workDir 'cab'
$voiceArchiveDir = Join-Path $workDir 'voice-archive'
$stagingDir = Join-Path $workDir 'natural'
$guyDir = Join-Path $stagingDir 'NarratorVoices\Guy'

New-Item -ItemType Directory -Path $adapterDir, $cabDir, $voiceArchiveDir, $guyDir | Out-Null

try {
    Write-Host 'Downloading the NaturalVoiceSAPIAdapter runtime...'
    Get-VerifiedDownload -Uri $adapterUri -Path $adapterZip -Sha256 $adapterSha256
    Expand-Archive -LiteralPath $adapterZip -DestinationPath $adapterDir
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $stagingDir 'adapter.manifest')
    Copy-Item -Path (Join-Path $adapterDir 'x64\*.dll') -Destination $stagingDir

    Write-Host 'Downloading the official Microsoft US English voice package...'
    Get-VerifiedDownload -Uri $voiceUri -Path $voiceCab -Sha256 $voiceSha256
    & "$env:SystemRoot\System32\expand.exe" -F:* $voiceCab $cabDir | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw 'Microsoft voice CAB extraction failed.'
    }

    $embeddedArchive = Get-ChildItem -File -LiteralPath $cabDir | Select-Object -First 1
    if (-not $embeddedArchive) {
        throw 'The Microsoft voice CAB did not contain its expected archive.'
    }
    & "$env:SystemRoot\System32\tar.exe" -xf $embeddedArchive.FullName -C $voiceArchiveDir
    if ($LASTEXITCODE -ne 0) {
        throw 'Microsoft voice archive extraction failed.'
    }

    $guyPackage = Get-ChildItem -File -Recurse -Filter '*.msix' -LiteralPath $voiceArchiveDir |
        Where-Object { $_.FullName -like '*\Guy_en-US\*' } |
        Select-Object -First 1
    if (-not $guyPackage) {
        throw 'Microsoft Guy was not found in the official voice package.'
    }
    & "$env:SystemRoot\System32\tar.exe" -xf $guyPackage.FullName -C $guyDir
    if ($LASTEXITCODE -ne 0) {
        throw 'Microsoft Guy extraction failed.'
    }

    foreach ($required in @(
        (Join-Path $stagingDir 'NaturalVoiceSAPIAdapter.dll'),
        (Join-Path $guyDir 'MSTTSLocEnUS.dat')
    )) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Setup output is missing $required"
        }
    }

    if (Test-Path -LiteralPath $destinationPath) {
        Add-Type -AssemblyName Microsoft.VisualBasic
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory(
            $destinationPath,
            [Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs,
            [Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin
        )
    }
    $destinationParent = Split-Path -Parent $destinationPath
    if (-not (Test-Path -LiteralPath $destinationParent)) {
        New-Item -ItemType Directory -Path $destinationParent | Out-Null
    }
    Move-Item -LiteralPath $stagingDir -Destination $destinationPath
    Write-Host "Microsoft Guy is ready at $destinationPath"
}
finally {
    if (Test-Path -LiteralPath $workDir) {
        [System.IO.Directory]::Delete($workDir, $true)
    }
}
