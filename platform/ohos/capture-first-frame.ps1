[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Hap,
    [Parameter(Mandatory)][string]$Target,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Hdc = 'hdc'
)
$ErrorActionPreference = 'Stop'
$hapPath = (Resolve-Path -LiteralPath $Hap).Path
$bundle = 'org.histeries.game'
$hapDigest = (Get-FileHash -LiteralPath $hapPath -Algorithm SHA256).Hash
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($hapPath)
try {
    $entries = @($archive.Entries | Where-Object FullName -ceq 'module.json')
    if ($entries.Count -ne 1) { throw 'Expected one compiled module.json in the HAP.' }
    $reader = [IO.StreamReader]::new($entries[0].Open())
    try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    if ($manifest.app.bundleName -cne $bundle -or $manifest.module.name -cne 'entry' -or
        $manifest.module.mainElement -cne 'EntryAbility' -or
        'EntryAbility' -cnotin $manifest.module.abilities.name -or
        '2in1' -cnotin $manifest.module.deviceTypes) {
        throw 'Expected the Histeries PC entry module and EntryAbility in the supplied HAP.'
    }
} finally { $archive.Dispose() }

function Invoke-Device([string[]]$Arguments) {
    $reply = (& $Hdc -t $Target @Arguments 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $reply -match '(?im)^\s*\[Fail\]') {
        throw "HDC $($Arguments -join ' ') failed: $reply"
    }
    $reply
}

function Read-Parameter([string]$Name) {
    $value = Invoke-Device @('shell', 'param', 'get', $Name)
    if ([string]::IsNullOrWhiteSpace($value) -or $value -match '(?i)not found|not exist') {
        throw "Device did not return $Name"
    }
    $value
}

function Get-BundleProcessId {
    $reply = Invoke-Device @('shell', "pidof $bundle || echo ABSENT")
    if ($reply -eq 'ABSENT') { return $null }
    if ($reply -notmatch '^[1-9][0-9]*$') {
        throw "Could not identify a single process for the board: $reply"
    }
    $reply
}

function Find-SubmittedFrame([string[]]$Lines) {
    $adapterLine = $null
    $submission = $null
    foreach ($line in $Lines) {
        if ($line -match 'Vulkan adapter:\s*(.+)$') {
            $adapterLine = $line
            $adapterName = $Matches[1]
            $submission = $null
        } elseif ($adapterLine -and $line -match 'Vulkan frame submitted for presentation ([1-9][0-9]*)x([1-9][0-9]*)') {
            $submission = [pscustomobject]@{
                adapter = $adapterName
                adapterLine = $adapterLine
                frameLine = $line
                width = [int]$Matches[1]
                height = [int]$Matches[2]
            }
        }
    }
    $submission
}

$deviceType = Read-Parameter 'const.product.devicetype'
if ($deviceType -notin @('2in1', 'pc')) {
    throw "Expected a PC/2-in-1 HDC target. Device reported '$deviceType'."
}
$model = Read-Parameter 'const.product.model'
$version = Read-Parameter 'const.product.software.version'
$directory = (New-Item -ItemType Directory -Path $OutputDirectory -ErrorAction Stop).FullName
$beforeInstall = Get-BundleProcessId
# Keep the validated Windows artifact readable by HDC and deny replacement during transfer.
$installInput = [IO.File]::Open($hapPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
    if ((Get-FileHash -InputStream $installInput -Algorithm SHA256).Hash -ne $hapDigest) {
        throw 'The selected HAP changed before installation.'
    }
    $install = Invoke-Device @('install', '-r', $hapPath)
} finally { $installInput.Dispose() }
if ($install -notmatch '(?im)\bmsg:\s*install bundle successfully\.?\s*$' -or $install -match '(?i)\bfail(?:ed|ure)?\b') {
    throw "HAP installation failed: $install"
}
$afterInstall = Get-BundleProcessId
if ($afterInstall) {
    $stop = Invoke-Device @('shell', 'aa', 'force-stop', $bundle)
    if ($stop -notmatch '(?i)success') { throw "Could not stop the previous game: $stop" }
    $stopDeadline = [datetime]::UtcNow.AddSeconds(15)
    while (Get-BundleProcessId) {
        if ([datetime]::UtcNow -ge $stopDeadline) { throw 'The previous game process is still running.' }
        Start-Sleep -Milliseconds 200
    }
}
# Snapshot the tag after the old process exits. Other processes retain their HiLog history.
$oldLines = (Invoke-Device @('shell', 'hilog', '-x', '-T', 'Histeries')) -split '\r?\n'
$launch = Invoke-Device @('shell', 'aa', 'start', '-b', $bundle, '-a', 'EntryAbility')
if ($launch -notmatch '(?i)success') {
    throw "Ability launch failed: $launch"
}

$deadline = [datetime]::UtcNow.AddSeconds(90)
do {
    Start-Sleep -Milliseconds 500
    $appProcess = Get-BundleProcessId
    if (-not $appProcess) { continue }
    if ($appProcess -in @($beforeInstall, $afterInstall)) {
        throw "Cold launch requires a distinct game process, found previous PID $appProcess."
    }
    $lines = (Invoke-Device @('shell', 'hilog', '-x', '-P', $appProcess, '-T', 'Histeries')) -split '\r?\n'
    $freshLines = @($lines | Where-Object { $_ -and $_ -cnotin $oldLines })
    $frame = Find-SubmittedFrame $freshLines
} until ($frame -or [datetime]::UtcNow -ge $deadline)

$freshLines | Set-Content -LiteralPath (Join-Path $directory 'hilog.txt')
if (-not $frame) {
    throw "Timed out waiting for this launch's Vulkan adapter and frame submission. See hilog.txt."
}

$remoteImage = "/data/local/tmp/histeries-$([guid]::NewGuid().ToString('N')).png"
$imagePath = Join-Path $directory 'screen.png'
try {
    Invoke-Device @('shell', 'uitest', 'screenCap', '-p', $remoteImage) | Out-Null
    Invoke-Device @('file', 'recv', $remoteImage, $imagePath) | Out-Null
} finally {
    Invoke-Device @('shell', 'rm', '-f', $remoteImage) | Out-Null
}
$png = [IO.File]::ReadAllBytes($imagePath)
if ($png.Length -lt 24 -or [BitConverter]::ToString($png, 0, 8) -ne '89-50-4E-47-0D-0A-1A-0A') {
    throw 'Device screenshot is missing the PNG signature.'
}
if ((Get-BundleProcessId) -ne $appProcess) { throw 'The game process changed during screenshot capture.' }
$latestLines = (Invoke-Device @('shell', 'hilog', '-x', '-P', $appProcess, '-T', 'Histeries')) -split '\r?\n'
$latestFrame = Find-SubmittedFrame @($latestLines | Where-Object { $_ -and $_ -cnotin $oldLines })
if (-not $latestFrame -or $latestFrame.adapterLine -cne $frame.adapterLine -or $latestFrame.frameLine -cne $frame.frameLine) {
    throw 'The renderer or submission changed during screenshot capture. Run the capture again.'
}
if ((Get-FileHash -LiteralPath $hapPath -Algorithm SHA256).Hash -ne $hapDigest) {
    throw 'The HAP changed between validation and capture.'
}
[ordered]@{
    capturedUtc = [datetime]::UtcNow.ToString('o')
    bundle = $bundle
    ability = 'EntryAbility'
    deviceType = $deviceType
    model = $model
    systemVersion = $version
    processId = $appProcess
    vulkanAdapter = $frame.adapter
    vulkanFrameSubmittedForPresentation = $true
    visualVerification = 'pending inspection of screen.png'
    surfaceWidth = $frame.width
    surfaceHeight = $frame.height
    hapSHA256 = $hapDigest
    screenshotSHA256 = (Get-FileHash -LiteralPath $imagePath -Algorithm SHA256).Hash
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $directory 'receipt.json')
Write-Host "Captured $imagePath. Inspect the game board in the screenshot before recording device acceptance."
