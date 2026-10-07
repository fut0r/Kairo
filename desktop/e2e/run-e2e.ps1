<#
.SYNOPSIS
  Runs the end-to-end scenario against the real desktop app on Windows.

.DESCRIPTION
  Starts kairo-desktop.exe with WebView2 remote debugging enabled, drives the
  actual window through the Chrome DevTools Protocol (run.mjs), and reports
  each check. The app talks to a real SQLite database through real Tauri
  commands; nothing is mocked.

  Everything the run writes (database, settings, webview profile) goes under
  a fresh folder in the temp directory, so your own Kairo settings are not
  touched.

  Build first:  npm run tauri build -- --debug --no-bundle
                cargo build   (for the kairo CLI, used to create the project)

.PARAMETER Screenshots
  Folder to write screenshots to. Defaults to a folder inside the run folder.

.PARAMETER Postgres
  A PostgreSQL URL, including its password. When given, the PostgreSQL
  scenario (postgres.mjs) runs instead of the SQLite one. It creates and
  drops the tables customers, products and orders, so use a scratch database.

  For a server that puts every client on one session, such as PGlite, set
  $env:KAIRO_PG_POOL_SIZE = '1' first.
#>
param(
    [string]$Screenshots,
    [string]$Postgres,
    # Where the run keeps its project, settings and webview profile. It must
    # not exist yet. Defaults to a new folder in the temp directory.
    [string]$WorkDir,
    [string]$Profile = 'debug',
    [int]$Port = 9222
)
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$exe = Join-Path $repo "target\$Profile\kairo-desktop.exe"
$kairo = Join-Path $repo "target\$Profile\kairo.exe"
foreach ($binary in $exe, $kairo) {
    if (-not (Test-Path $binary)) { throw "Missing $binary. Build it first; see Get-Help $PSCommandPath." }
}

$root = if ($WorkDir) { $WorkDir } else { Join-Path ([IO.Path]::GetTempPath()) ("kairo-e2e-" + (Get-Date -Format 'yyyyMMdd-HHmmss')) }
if (Test-Path $root) { throw "$root already exists. Choose a folder that does not, so the run starts clean." }
$project = Join-Path $root 'demo-shop'
$config = Join-Path $root 'config'
if (-not $Screenshots) { $Screenshots = Join-Path $root 'shots' }
New-Item -ItemType Directory -Force $project, $config, $Screenshots, (Join-Path $root 'webview') | Out-Null
# The scenario compares paths, so use the canonical spelling of each.
$project = (Resolve-Path $project).Path
$config = (Resolve-Path $config).Path
$Screenshots = (Resolve-Path $Screenshots).Path

# A real project, made with the CLI, whose database starts empty.
Push-Location $project
& $kairo init | Out-Null
[IO.File]::WriteAllText((Join-Path $project 'kairo.config'), "# KairoDB Configuration`nadapter = `"sqlite`"`ndatabase = `"data/shop.db`"`n")
& $kairo query 'SELECT 1' | Out-Null
Pop-Location
if (-not (Test-Path (Join-Path $project 'data\shop.db'))) { throw 'The CLI did not create the database.' }

$env:KAIRO_CONFIG_DIR = $config
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $root 'webview'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"

$app = Start-Process -FilePath $exe -PassThru
$code = 1
try {
    Start-Sleep -Seconds 3
    "Launched $exe (pid $($app.Id), window '$((Get-Process -Id $app.Id).MainWindowTitle)')"
    if ($Postgres) {
        node (Join-Path $PSScriptRoot 'postgres.mjs') $Postgres $Screenshots $config
    }
    else {
        node (Join-Path $PSScriptRoot 'run.mjs') $project $Screenshots $config
    }
    $code = $LASTEXITCODE
}
finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
}
"Run folder:  $root"
"Screenshots: $Screenshots"
exit $code
