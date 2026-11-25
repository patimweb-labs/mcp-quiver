#Requires -Version 5.1
<#
.SYNOPSIS
    Die Liste der mitgelieferten Werkzeuge für das MSI schreiben (Aufgabe 35).

.DESCRIPTION
    `quiver.wxs` beschreibt das Paket; welche Werkzeuge darin liegen, weiß es
    nicht — die Liste entsteht erst beim Bauen. Dieses Skript schreibt dafür
    ein eigenes Fragment, das `wix build` neben `quiver.wxs` bekommt:

        verteilung\tools\wix-werkzeuge.ps1 -Werkzeuge dist\tools -Ziel dist\werkzeuge.wxs

    Die Werkzeuge landen in `<Installation>\tools` — dort sucht der Client
    (neben `quiver.exe`), und dort findet sie auch das portable ZIP.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Werkzeuge,
    [Parameter(Mandatory = $true)][string]$Ziel
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not (Test-Path $Werkzeuge)) {
    throw "$Werkzeuge fehlt — erst verteilung\tools\bauen.ps1 laufen lassen."
}
$dateien = @(Get-ChildItem $Werkzeuge -File | Sort-Object Name)
if ($dateien.Count -eq 0) {
    throw "$Werkzeuge ist leer — erst verteilung\tools\bauen.ps1 laufen lassen."
}

# Eine Kennung je Datei: nur Buchstaben und Ziffern, mit einem Buchstaben
# beginnend (`QuiverDevops` aus `quiver-devops.exe`).
function Kennung([string]$name) {
    $ohne = [System.IO.Path]::GetFileNameWithoutExtension($name)
    $sauber = ($ohne -split '[^A-Za-z0-9]') | ForEach-Object {
        if ($_) { $_.Substring(0, 1).ToUpper() + $_.Substring(1) }
    }
    "Werkzeug$(($sauber -join ''))"
}

$kennungen = @{}
$teile = @()
foreach ($datei in $dateien) {
    $kennung = Kennung $datei.Name
    if ($kennungen.ContainsKey($kennung)) {
        throw "zwei Werkzeuge ergeben dieselbe Kennung »$kennung« ($($datei.Name))."
    }
    $kennungen[$kennung] = $true
    $teile += @"
            <Component>
                <File Id="$kennung" Source="$($datei.FullName)" />
            </Component>
"@
}

$text = @"
<?xml version="1.0" encoding="utf-8"?>
<!--
  Erzeugt von ``verteilung/tools/wix-werkzeuge.ps1`` beim Bauen (Aufgabe 35):
  die Programme aus quiver-tools, die im Paket liegen. Nicht von Hand ändern.

  Sie stehen in ``<Installation>\tools`` — dort sucht der Client sie neben
  ``quiver.exe``.
-->
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
    <Fragment>
        <DirectoryRef Id="INSTALLFOLDER">
            <Directory Id="WerkzeugOrdner" Name="tools" />
        </DirectoryRef>
    </Fragment>

    <Fragment>
        <ComponentGroup Id="Werkzeuge" Directory="WerkzeugOrdner">
$($teile -join "`r`n")
        </ComponentGroup>
    </Fragment>
</Wix>
"@

Set-Content -Path $Ziel -Value $text -Encoding utf8
Write-Host "==> $($dateien.Count) Werkzeuge für das MSI: $Ziel"
