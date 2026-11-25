#Requires -Version 5.1
<#
.SYNOPSIS
    Windows-Pakete: portables ZIP und MSI (je Benutzer, ohne Administratorrechte).

.DESCRIPTION
    Dieselbe Datei, die die CI baut und die am Release hängt:

        verteilung\windows\paket.ps1                  # baut und paketiert
        verteilung\windows\paket.ps1 -OhneBau         # nimmt target\release

    Ergebnisse in dist\ :
        quiver-<version>-windows-x86_64.zip
        quiver-<version>-windows-x86_64.msi

    Das MSI entsteht mit WiX 5 (`dotnet tool install --global wix`); es
    installiert nach %LOCALAPPDATA%\Programs\mcp-quiver, legt eine
    Startmenü-Verknüpfung an und lässt sich ohne Administratorrechte
    deinstallieren. Es ist **unsigniert** — SmartScreen fragt beim ersten
    Start. Was das heißt, steht in docs\release.de.md.

    Autostart richtet der Client selbst ein (HKCU\...\Run, siehe
    crates\fenster\src\autostart.rs); das Paket bringt keinen mit.
#>
[CmdletBinding()]
param(
    [switch]$OhneBau
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$wurzel = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $wurzel

# ---------------------------------------------------------------- Version
$version = $env:VERSION
if (-not $version) {
    $cargo = Get-Content -Raw (Join-Path $wurzel 'Cargo.toml')
    $abschnitt = [regex]::Match($cargo, '(?ms)^\[workspace\.package\](.*?)(^\[|\z)')
    if (-not $abschnitt.Success) {
        throw 'Cargo.toml hat keinen [workspace.package]-Abschnitt.'
    }
    $treffer = [regex]::Match($abschnitt.Groups[1].Value, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $treffer.Success) {
        throw 'Cargo.toml hat keine Version im [workspace.package]-Abschnitt.'
    }
    $version = $treffer.Groups[1].Value
}

$dist = Join-Path $wurzel 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$name = "quiver-$version-windows-x86_64"

Write-Host "==> quiver $version für Windows x86_64"

# ---------------------------------------------------------------- Bauen
if (-not $OhneBau) {
    cargo build --release --locked -p quiver-fenster
    if ($LASTEXITCODE -ne 0) { throw 'cargo build fehlgeschlagen.' }
}
# `CARGO_TARGET_DIR` gilt auch hier: sonst suchte das Skript in `target\`.
$zielordner = $env:CARGO_TARGET_DIR
if (-not $zielordner) { $zielordner = Join-Path $wurzel 'target' }
elseif (-not [System.IO.Path]::IsPathRooted($zielordner)) {
    $zielordner = Join-Path $wurzel $zielordner
}
$programm = Join-Path $zielordner 'release\quiver.exe'
if (-not (Test-Path $programm)) {
    throw "$programm fehlt — erst bauen (ohne -OhneBau)."
}

# ---------------------------------------------------------------- Symbol
# `.ico` aus den vorhandenen Bildern in assets\marke — kein neues Logo.
# Ein ICO ist ein kleiner Kopf (6 Byte) mit einem Verzeichnis (16 Byte je Bild)
# und den PNG-Daten dahinter; genau das schreibt der Block unten.
function New-Symbol {
    param([string]$Ziel)

    $kanten = @(32, 64, 256)
    $bilder = @()
    foreach ($kante in $kanten) {
        $pfad = Join-Path $wurzel "assets\marke\quiver-dunkel-${kante}.png"
        if (-not (Test-Path $pfad)) { throw "$pfad fehlt." }
        # Das Komma hält das Byte-Array als ein Element der Liste.
        $bilder += , ([System.IO.File]::ReadAllBytes($pfad))
    }

    $speicher = New-Object System.IO.MemoryStream
    $schreiber = New-Object System.IO.BinaryWriter($speicher)
    $schreiber.Write([UInt16]0)              # reserviert
    $schreiber.Write([UInt16]1)              # 1 = Symbol
    $schreiber.Write([UInt16]$kanten.Count)  # Anzahl der Bilder
    $versatz = 6 + 16 * $kanten.Count
    for ($i = 0; $i -lt $kanten.Count; $i++) {
        # 256 passt nicht in ein Byte und wird als 0 geschrieben.
        $kante = if ($kanten[$i] -ge 256) { 0 } else { $kanten[$i] }
        $schreiber.Write([byte]$kante)               # Breite
        $schreiber.Write([byte]$kante)               # Höhe
        $schreiber.Write([byte]0)                    # keine Farbtabelle
        $schreiber.Write([byte]0)                    # reserviert
        $schreiber.Write([UInt16]1)                  # Ebenen
        $schreiber.Write([UInt16]32)                 # Bit je Punkt
        $schreiber.Write(([UInt32]$bilder[$i].Length)) # Größe der Daten
        $schreiber.Write([UInt32]$versatz)           # wo sie anfangen
        $versatz += $bilder[$i].Length
    }
    foreach ($bild in $bilder) { $schreiber.Write($bild) }
    $schreiber.Flush()
    [System.IO.File]::WriteAllBytes($Ziel, $speicher.ToArray())
    $schreiber.Dispose()
    $speicher.Dispose()
}

$symbol = Join-Path $dist 'quiver.ico'
New-Symbol -Ziel $symbol

# ---------------------------------------------------------------- Portables ZIP
$ordner = Join-Path $dist $name
if (Test-Path $ordner) { Remove-Item -Recurse -Force $ordner }
New-Item -ItemType Directory -Force $ordner | Out-Null
Copy-Item $programm, (Join-Path $wurzel 'LICENSE'), (Join-Path $wurzel 'README.md') $ordner
$zip = Join-Path $dist "${name}.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $ordner '*') -DestinationPath $zip
Remove-Item -Recurse -Force $ordner

# ---------------------------------------------------------------- MSI
if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
    Write-Host '==> WiX installieren'
    dotnet tool install --global wix --version 5.0.2
    $env:PATH = "$env:USERPROFILE\.dotnet\tools;$env:PATH"
    # Kein Blick auf $LASTEXITCODE: meldet dotnet „schon installiert“, ist das
    # in Ordnung. Entscheidend ist, ob `wix` jetzt da ist.
    if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
        throw 'WiX ist nicht verfügbar (dotnet tool install --global wix).'
    }
}

$msi = Join-Path $dist "${name}.msi"
if (Test-Path $msi) { Remove-Item -Force $msi }
wix build `
    -arch x64 `
    -d "Version=$version" `
    -d "Quelle=$(Join-Path $zielordner 'release')" `
    -d "Wurzel=$wurzel" `
    -d "Symbol=$symbol" `
    -o $msi `
    (Join-Path $wurzel 'verteilung\windows\quiver.wxs')
if ($LASTEXITCODE -ne 0) { throw 'wix build fehlgeschlagen.' }

Write-Host '==> fertig'
Get-ChildItem $dist | Format-Table Name, Length
