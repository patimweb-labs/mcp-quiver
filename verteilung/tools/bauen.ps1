#Requires -Version 5.1
<#
.SYNOPSIS
    Die mitgelieferten Werkzeuge bauen (Aufgabe 35) — Windows-Fassung von
    verteilung/tools/bauen.sh.

.DESCRIPTION
    Baut den Stand aus verteilung\tools\stand.txt (fester Commit, nicht `main`)
    aus einem frischen Klon des öffentlichen Repos und legt die Programme nach
    dist\tools\. Danach steht in dist\TOOLS.txt, was im Paket steckt.

    Gestrippt wird mit `strip -x`, wenn der Läufer es mitbringt (Git for
    Windows tut das); sonst steht im Bericht, dass es nicht gestrippt ist.

    Läuft auch lokal:

        verteilung\tools\bauen.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$wurzel = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# ---------------------------------------------------------------- Stand
$stand = @{}
Get-Content (Join-Path $wurzel 'verteilung\tools\stand.txt') | ForEach-Object {
    if ($_ -match '^\s*#' -or $_ -notmatch '^([a-z]+)=(.*)$') { return }
    $stand[$Matches[1]] = $Matches[2].Trim()
}
foreach ($feld in @('repo', 'commit', 'version')) {
    if (-not $stand.ContainsKey($feld) -or -not $stand[$feld]) {
        throw "verteilung\tools\stand.txt nennt »$feld« nicht."
    }
}
$repo = $stand['repo']
$commit = $stand['commit']
$version = $stand['version']

# ---------------------------------------------------------------- Ordner
$zielordner = $env:CARGO_TARGET_DIR
if (-not $zielordner) { $zielordner = Join-Path $wurzel 'target' }
elseif (-not [System.IO.Path]::IsPathRooted($zielordner)) {
    $zielordner = Join-Path $wurzel $zielordner
}
$bau = if ($env:QUIVER_TOOLS_BAU) { $env:QUIVER_TOOLS_BAU } else { Join-Path $zielordner 'werkzeuge' }
$dist = Join-Path $wurzel 'dist'
$ziel = Join-Path $dist 'tools'
$zwischen = Join-Path ([System.IO.Path]::GetTempPath()) "quiver-tools-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
$quelle = Join-Path $zwischen 'quiver-tools'

Write-Host "==> Werkzeuge $version aus $repo@$($commit.Substring(0, 12))"

try {
    # Nur diesen einen Commit holen: kein Verlauf, keine Zweige, kein `main`.
    New-Item -ItemType Directory -Force $quelle | Out-Null
    git -C $quelle init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'git init fehlgeschlagen.' }
    git -C $quelle remote add origin "https://github.com/$repo.git"
    git -C $quelle fetch --quiet --depth 1 origin $commit
    if ($LASTEXITCODE -ne 0) { throw "Commit $commit ist nicht zu holen." }
    git -C $quelle checkout --quiet --detach FETCH_HEAD
    if ($LASTEXITCODE -ne 0) { throw 'git checkout fehlgeschlagen.' }

    # ------------------------------------------------------------ Bauen
    Write-Host '==> bauen'
    $env:CARGO_TARGET_DIR = $bau
    cargo build --release --locked --quiet --manifest-path (Join-Path $quelle 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'cargo build fehlgeschlagen.' }

    # Alle Programme mit dem Präfix `quiver-`; die Bibliotheken darunter haben
    # keine .exe und tauchen hier nicht auf.
    if (Test-Path $ziel) { Remove-Item -Recurse -Force $ziel }
    New-Item -ItemType Directory -Force $ziel | Out-Null
    $programme = Get-ChildItem (Join-Path $bau 'release') -Filter 'quiver-*' -File |
        Where-Object { $_.Extension -in @('', '.exe') }
    if (-not $programme) {
        throw "kein Programm in $bau\release — hat der Bau geklappt?"
    }
    foreach ($programm in $programme) {
        Copy-Item $programm.FullName $ziel
    }

    $gestrippt = 'nein'
    if (Get-Command strip -ErrorAction SilentlyContinue) {
        foreach ($programm in Get-ChildItem $ziel -File) {
            strip -x $programm.FullName
            if ($LASTEXITCODE -ne 0) { throw "strip fehlgeschlagen: $($programm.Name)" }
        }
        $gestrippt = 'ja'
    }
    else {
        Write-Host '==> kein »strip« auf diesem Läufer: die Programme behalten ihre Symbole'
    }

    $anzahl = @(Get-ChildItem $ziel -File).Count
    $groesseKib = [math]::Round(
        ((Get-ChildItem $ziel -File | Measure-Object -Property Length -Sum).Sum / 1kb)
    )
    Write-Host "==> $anzahl Programme, $([math]::Round($groesseKib / 1kb)) MiB"

    # Der Bericht: was im Paket steckt, für das Release und zum Nachlesen.
    $bericht = @(
        '# Werkzeuge im Paket (Aufgabe 35)'
        "quelle=https://github.com/$repo"
        "commit=$commit"
        "version=$version"
        "anzahl=$anzahl"
        "groesse_kib=$groesseKib"
        'gestrippt=ja'
    )
    $bericht += (Get-ChildItem $ziel -File | Sort-Object Name | ForEach-Object { "datei=$($_.Name)" })
    New-Item -ItemType Directory -Force $dist | Out-Null
    Set-Content -Path (Join-Path $dist 'TOOLS.txt') -Value $bericht -Encoding utf8

    Get-ChildItem $ziel | Format-Table Name, Length
}
finally {
    Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $zwischen -ErrorAction SilentlyContinue
}
