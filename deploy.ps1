# Builds the mod, wipes the deployed copy, and lays it down fresh.
#
# Wiping rather than overwriting is deliberate: a file you delete from the repo
# would otherwise linger in the game folder forever and keep being loaded.
#
# Run with:  .\deploy.ps1  [-SkipBuild] [-GameDir <path>] [-WhatIf]

[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$GameDir = "D:\SteamLibrary\steamapps\common\Teamfight Manager2",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$modId = Split-Path -Leaf $repo

# --- what gets deployed -----------------------------------------------------
$Include = @(
    "mod.mod_info",
    "mod.override_info",
    "LICENSE",
    "$modId.dll",
    "preview.png",
    "thumbnail.png",
    "ui",
    "text"
)

$ExcludeExtensions = @(".xcf", ".psd", ".bak", ".orig")
# ---------------------------------------------------------------------------

$deploy = Join-Path (Join-Path $GameDir "mods") $modId

# The game holds the DLL open, so a wipe would half-finish and leave the mod in
# a state that loads badly. Refuse rather than kill anything.
if (Get-Process -Name "TeamfightManager2" -ErrorAction SilentlyContinue) {
    Write-Error "Teamfight Manager 2 is running - close it first (it holds $modId.dll open)."
}

if (-not $SkipBuild) {
    Write-Host "Building $modId..." -ForegroundColor Cyan
    cargo build --release --manifest-path (Join-Path $repo "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $built = Join-Path $repo "target\release\$modId.dll"
    if (-not (Test-Path -LiteralPath $built)) {
        Write-Error "Build finished but $built is missing."
    }
    Copy-Item -LiteralPath $built -Destination (Join-Path $repo "$modId.dll") -Force
}

# Guard rails before anything is deleted: the target must be a `mods\<mod id>`
# folder and nothing else, so a bad -GameDir cannot wipe a game install.
$parent = Split-Path -Parent $deploy
if ((Split-Path -Leaf $deploy) -ne $modId -or (Split-Path -Leaf $parent) -ne "mods") {
    Write-Error "Refusing to touch '$deploy' - it is not a mods\$modId folder."
}
if (-not (Test-Path -LiteralPath $parent)) {
    Write-Error "No mods folder at '$parent' - is -GameDir right?"
}

if (Test-Path -LiteralPath $deploy) {
    if ($PSCmdlet.ShouldProcess($deploy, "Delete contents")) {
        Remove-Item -LiteralPath (Join-Path $deploy "*") -Recurse -Force
    }
} else {
    New-Item -ItemType Directory -Path $deploy | Out-Null
}

$copied = 0
$skipped = 0
foreach ($name in $Include) {
    $source = Join-Path $repo $name
    if (-not (Test-Path -LiteralPath $source)) {
        Write-Warning "not found, skipping: $name"
        continue
    }

    if (Test-Path -LiteralPath $source -PathType Container) {
        # Rebuild the tree by hand so the extension filter applies at any depth.
        foreach ($file in Get-ChildItem -LiteralPath $source -Recurse -File) {
            if ($ExcludeExtensions -contains $file.Extension.ToLower()) {
                $skipped++
                continue
            }
            $relative = $file.FullName.Substring($repo.Length).TrimStart('\')
            $destination = Join-Path $deploy $relative
            $destinationDir = Split-Path -Parent $destination
            if (-not (Test-Path -LiteralPath $destinationDir)) {
                New-Item -ItemType Directory -Path $destinationDir -Force | Out-Null
            }
            Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
            $copied++
        }
    } else {
        Copy-Item -LiteralPath $source -Destination (Join-Path $deploy $name) -Force
        $copied++
    }
}

Write-Host "Deployed $copied files to $deploy" -ForegroundColor Green
if ($skipped) { Write-Host "Skipped $skipped source files ($($ExcludeExtensions -join ', '))" }
