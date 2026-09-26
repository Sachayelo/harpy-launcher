<#
  Publie une nouvelle version du launcher : compile l'installeur, le signe et le
  met en ligne sur GitHub. Les launchers des joueurs se mettent à jour tout seuls.
  Normalement lancé depuis l'Atelier du launcher.
#>
param(
    [string]$Version = '',
    [switch]$Yes
)

$ErrorActionPreference = 'Stop'
try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch { }
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')

$Repo = 'Sachayelo/harpy-launcher'
$Root = Split-Path -Parent $PSScriptRoot
$Key = Join-Path $env:USERPROFILE '.harpy\launcher-update.key'
$CargoToml = Join-Path $Root 'src-tauri\Cargo.toml'
$BundleDir = Join-Path $Root 'src-tauri\target\release\bundle\nsis'
$BuildLog = Join-Path $env:TEMP 'harpy-launcher-build.log'
$Staging = Join-Path $env:TEMP 'harpy-launcher-release'
$AssetName = 'Harpy-Launcher-Setup.exe'
$Utf8NoBom = New-Object System.Text.UTF8Encoding $false

function Assert-LastExit([string]$Step) {
    if ($LASTEXITCODE -ne 0) { throw "$Step a échoué (code $LASTEXITCODE)." }
}

# Runs a command through cmd so its output lands in the build log instead of
# being turned into PowerShell errors.
function Invoke-Logged([string]$Step, [string]$CommandLine) {
    cmd /d /c "$CommandLine >> `"$BuildLog`" 2>&1"
    if ($LASTEXITCODE -ne 0) {
        Get-Content $BuildLog -Tail 15 -Encoding UTF8 | ForEach-Object { Write-Output "  $_" }
        throw "$Step a échoué, le détail est dans $BuildLog."
    }
}

function Test-Release([string]$Tag) {
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    gh release view $Tag --repo $Repo --json tagName *> $null
    $exists = ($LASTEXITCODE -eq 0)
    $ErrorActionPreference = $saved
    return $exists
}

if (-not (Test-Path $Key)) { throw "Clé de signature introuvable ($Key)." }
$login = gh api user --jq .login
if ($LASTEXITCODE -ne 0) { throw "GitHub CLI n'est pas connecté : lance « gh auth login »." }
if (git -C $Root status --porcelain) { throw "Enregistre d'abord le code du launcher (Commit & push)." }

# The launcher refuses unsigned pack manifests: they must be online before it is.
foreach ($channel in 'dev', 'prod') {
    try {
        Invoke-WebRequest "https://raw.githubusercontent.com/Sachayelo/harpy-pack/main/channels/$channel.json.sig?t=$(Get-Random)" -UseBasicParsing | Out-Null
    } catch {
        throw "Le pack $channel n'est pas encore signé en ligne : fais d'abord Commit & push sur harpy-pack."
    }
}

$manifest = [System.IO.File]::ReadAllText($CargoToml)
$pattern = New-Object System.Text.RegularExpressions.Regex '(?m)^version = "([^"]+)"'
$current = $pattern.Match($manifest).Groups[1].Value
if (-not $Version) { $Version = $current }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Numéro de version invalide : $Version" }
$tag = "v$Version"
if (Test-Release $tag) { throw "La version $Version est déjà publiée." }

if (-not $Yes) {
    $answer = Read-Host "Publier le launcher $Version pour tous les joueurs ? (o/N)"
    if ($answer -notmatch '^[oOyY]') { Write-Output 'Annulé.'; exit 0 }
}

Write-Output "Launcher $Version"
if ($Version -ne $current) {
    [System.IO.File]::WriteAllText($CargoToml, $pattern.Replace($manifest, "version = `"$Version`"", 1), $Utf8NoBom)
}

Set-Content -Path $BuildLog -Value "Build du launcher $Version" -Encoding UTF8
try {
    Write-Output 'Compilation de l''installeur (quelques minutes)…'
    Invoke-Logged 'La compilation' "cd /d `"$Root`" && npx tauri build --bundles nsis"
} catch {
    # Leave the repository as it was: the version only changes once it is published.
    $ErrorActionPreference = 'Continue'
    git -C $Root checkout --quiet -- src-tauri/Cargo.toml src-tauri/Cargo.lock *> $null
    $ErrorActionPreference = 'Stop'
    throw
}

$installer = Get-ChildItem $BundleDir -Filter "*_${Version}_x64-setup.exe" | Select-Object -First 1
if (-not $installer) { throw "Installeur introuvable dans $BundleDir." }

Write-Output 'Signature…'
Remove-Item "$($installer.FullName).sig" -ErrorAction SilentlyContinue
Invoke-Logged 'La signature' "cd /d `"$Root`" && npx tauri signer sign -f `"$Key`" -p `"`" `"$($installer.FullName)`""
$signature = (Get-Content "$($installer.FullName).sig" -Raw).Trim()

Remove-Item $Staging -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Staging | Out-Null
$setup = Join-Path $Staging $AssetName
Copy-Item $installer.FullName $setup
$latest = [ordered]@{
    version   = $Version
    notes     = "Harpy Launcher $Version"
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = $signature
            url       = "https://github.com/$Repo/releases/download/$tag/$AssetName"
        }
    }
}
$latestPath = Join-Path $Staging 'latest.json'
[System.IO.File]::WriteAllText($latestPath, ($latest | ConvertTo-Json -Depth 4), $Utf8NoBom)

Write-Output 'Envoi sur GitHub…'
if (git -C $Root status --porcelain) {
    git -C $Root add src-tauri/Cargo.toml src-tauri/Cargo.lock
    Assert-LastExit 'git add'
    git -C $Root commit --quiet -m "Launcher $Version"
    Assert-LastExit 'git commit'
}
git -C $Root push --quiet
Assert-LastExit 'git push'
$commit = (git -C $Root rev-parse HEAD).Trim()

$notes = "Mise à jour automatique pour les joueurs déjà équipés. Nouveau joueur : télécharge $AssetName, lance-le, c'est tout."
gh release create $tag $setup $latestPath --repo $Repo --target $commit --title "Harpy Launcher $Version" --notes $notes | Out-Null
Assert-LastExit 'La création de la release'
Remove-Item $Staging -Recurse -Force -ErrorAction SilentlyContinue

Write-Output "Launcher $Version en ligne : les joueurs l'auront à leur prochain lancement."
