# Install workstation dependencies for building and testing ir on Windows.
# This installs system tools only; the first test run still owns the ir/R
# package cache warm-up.

[CmdletBinding()]
param(
    [switch]$DryRun,
    [string[]]$Skip = @()
)

$ErrorActionPreference = "Stop"

$TestRSpec = "oldrel/2"
$TestRName = $null
$TestRVersion = $null
$TestRExcludeNewer = $null
$RustupInitUrl = "https://win.rustup.rs"
$SkipRust = $false
$SkipPython = $false
$SkipQuarto = $false
$SkipRRelease = $false
$SkipTestR = $false

foreach ($component in $Skip) {
    switch ($component) {
        "rust" { $SkipRust = $true }
        "python" { $SkipPython = $true }
        "quarto" { $SkipQuarto = $true }
        "r-release" { $SkipRRelease = $true }
        "test-r" { $SkipTestR = $true }
        default { throw "unsupported skip component: $component" }
    }
}

function Write-Step {
    param(
        [Parameter(Mandatory = $true)][string]$File,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    Write-Host ("+ " + $File + " " + ($Arguments -join " "))
}

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)][string]$File,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    Write-Step $File $Arguments
    if (-not $DryRun) {
        & $File @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$File exited with code $LASTEXITCODE"
        }
    }
}

function Test-Tool {
    param([Parameter(Mandatory = $true)][string]$Name)

    if ($DryRun) {
        return $false
    }

    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Test-AnyTool {
    param([Parameter(Mandatory = $true)][string[]]$Names)

    foreach ($name in $Names) {
        if (Test-Tool $name) {
            return $true
        }
    }
    return $false
}

function Test-RunnableTool {
    param([Parameter(Mandatory = $true)][string]$Name)

    if ($DryRun) {
        return $false
    }

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        return $false
    }

    $windowsApps = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps"
    if ($command.Source -and $command.Source.StartsWith($windowsApps, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $false
    }

    & $Name --version *> $null
    return $LASTEXITCODE -eq 0
}

function Test-AnyRunnableTool {
    param([Parameter(Mandatory = $true)][string[]]$Names)

    foreach ($name in $Names) {
        if (Test-RunnableTool $name) {
            return $true
        }
    }
    return $false
}

function Get-PythonTool {
    foreach ($name in @("python", "python3")) {
        if (Test-RunnableTool $name) {
            return $name
        }
    }
    return "python"
}

function Require-Tool {
    param([Parameter(Mandatory = $true)][string]$Name)

    if (-not $DryRun -and -not (Test-Tool $Name)) {
        throw "required command not found: $Name"
    }
}

function ConvertFrom-RigJson {
    param([Parameter(Mandatory = $true)][string[]]$Lines)

    $json = ($Lines | Where-Object { -not $_.StartsWith("[INFO]") }) -join "`n"
    return $json | ConvertFrom-Json
}

function Get-TestRMetadata {
    $lines = & rig list --json
    if ($LASTEXITCODE -ne 0) {
        throw "rig list --json exited with code $LASTEXITCODE"
    }

    $installed = @(ConvertFrom-RigJson $lines)
    $release = $installed |
        Where-Object { $_.name -eq "release" -or $_.aliases -contains "release" } |
        Select-Object -First 1

    if ($null -eq $release) {
        throw "rig does not report an installed release R"
    }

    if ($TestRSpec -eq "oldrel") {
        $offset = 1
    }
    elseif ($TestRSpec.StartsWith("oldrel/")) {
        $offset = [int](($TestRSpec -split "/", 2)[1])
    }
    else {
        throw "unsupported test R spec: $TestRSpec"
    }

    $releaseVersion = [version]$release.version
    if ($releaseVersion.Minor -lt $offset) {
        throw "cannot resolve $TestRSpec relative to installed release R $($release.version)"
    }

    $targetMinor = $releaseVersion.Minor - $offset
    $matches = @(
        $installed |
            Where-Object { $_.version -match "^\d+\.\d+\.\d+$" } |
            ForEach-Object {
                $version = [version]$_.version
                if ($version.Major -eq $releaseVersion.Major -and $version.Minor -eq $targetMinor) {
                    [pscustomobject]@{
                        Version = $version
                        Install = $_
                    }
                }
            } |
            Sort-Object Version -Descending
    )
    if (-not $matches) {
        throw "R $($releaseVersion.Major).$targetMinor from $TestRSpec is not installed by rig"
    }

    $install = $matches[0].Install
    $dateExpr = 'cat(sprintf("%s-%s-%s\n", R.version$year, R.version$month, R.version$day))'
    $dateLines = & rig run -r $install.name -e $dateExpr
    if ($LASTEXITCODE -ne 0) {
        throw "rig run -r $($install.name) exited with code $LASTEXITCODE"
    }
    $dateMatch = ($dateLines | Select-String -Pattern "\d{4}-\d{2}-\d{2}" | Select-Object -First 1)
    $date = if ($dateMatch) { $dateMatch.Matches[0].Value } else { "" }
    if ($date -notmatch "^\d{4}-\d{2}-\d{2}$") {
        throw "could not read R release date for $($install.name)"
    }

    return [pscustomobject]@{
        Name = $install.name
        Version = $install.version
        Date = $date
    }
}

function Set-TestRMetadata {
    if ($SkipTestR) {
        return
    }

    if ($DryRun) {
        $script:TestRVersion = "<resolved-$TestRSpec-version>"
        $script:TestRExcludeNewer = "<release-date-for-$TestRSpec>"
        return
    }

    $metadata = Get-TestRMetadata
    $script:TestRName = $metadata.Name
    $script:TestRVersion = $metadata.Version
    $script:TestRExcludeNewer = $metadata.Date
}

function Get-RigNameForVersion {
    param([Parameter(Mandatory = $true)][string]$Version)

    $lines = & rig list --json
    if ($LASTEXITCODE -ne 0) {
        throw "rig list --json exited with code $LASTEXITCODE"
    }

    foreach ($install in @(ConvertFrom-RigJson $lines)) {
        if ($install.version -eq $Version) {
            return $install.name
        }
    }

    throw "R $Version is not installed by rig"
}

function Add-PathIfExists {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ((Test-Path $Path) -and (($env:PATH -split [IO.Path]::PathSeparator) -notcontains $Path)) {
        $env:PATH = "$Path$([IO.Path]::PathSeparator)$env:PATH"
    }
}

function Add-KnownInstallPaths {
    Add-PathIfExists (Join-Path $HOME ".cargo\bin")
    Add-PathIfExists (Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps")
    Add-PathIfExists (Join-Path $env:LOCALAPPDATA "Programs\Quarto\bin")
    Add-PathIfExists (Join-Path $env:ProgramFiles "Quarto\bin")
    Add-PathIfExists (Join-Path $env:ProgramFiles "R\bin")
    Add-PathIfExists (Join-Path $env:ProgramFiles "rig")
    Add-PathIfExists (Join-Path $env:ProgramFiles "rig\bin")
    Add-PathIfExists (Join-Path $env:ProgramFiles "R\rig\bin")
    Add-PathIfExists (Join-Path $env:LOCALAPPDATA "Programs\R\rig\bin")
}

function Install-WingetPackage {
    param([Parameter(Mandatory = $true)][string]$Id)

    Require-Tool "winget"
    Invoke-Step "winget" @(
        "install",
        "--id",
        $Id,
        "--exact",
        "--accept-package-agreements",
        "--accept-source-agreements"
    )
}

function Install-Rig {
    if ($env:GITHUB_ACTIONS -eq "true") {
        Require-Tool "choco"
        Invoke-Step "choco" @("install", "rig", "-y", "--no-progress")
    }
    else {
        Install-WingetPackage "posit.rig"
    }
}

function Install-Rustup {
    $rustupInit = Join-Path ([System.IO.Path]::GetTempPath()) "rustup-init-$([System.Guid]::NewGuid().ToString('N')).exe"

    Write-Host "+ Invoke-WebRequest -Uri $RustupInitUrl -OutFile $rustupInit"
    if (-not $DryRun) {
        Invoke-WebRequest -Uri $RustupInitUrl -OutFile $rustupInit
    }

    try {
        Invoke-Step $rustupInit @("-y", "--default-toolchain", "stable")
    }
    finally {
        if (-not $DryRun) {
            Remove-Item $rustupInit -Force -ErrorAction SilentlyContinue
        }
    }
}

Add-KnownInstallPaths

if (-not $SkipRust -and -not (Test-Tool "cl")) {
    Require-Tool "winget"
    Invoke-Step "winget" @(
        "install",
        "--id",
        "Microsoft.VisualStudio.2022.BuildTools",
        "--exact",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--override",
        "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    )
}

if (-not $SkipRust -and -not (Test-Tool "cargo")) {
    Install-Rustup
    Add-KnownInstallPaths
}

if (-not $SkipRust -and ($DryRun -or (Test-Tool "rustup"))) {
    Invoke-Step "rustup" @("toolchain", "install", "stable", "--component", "rustfmt", "--component", "clippy")
    Invoke-Step "rustup" @("default", "stable")
}

if (-not $SkipPython -and -not (Test-AnyRunnableTool @("python", "python3"))) {
    Install-WingetPackage "Python.Python.3.13"
    Add-KnownInstallPaths
}

if (-not (Test-Tool "rig")) {
    Install-Rig
    Add-KnownInstallPaths
}

if (-not $SkipQuarto -and -not (Test-Tool "quarto")) {
    Install-WingetPackage "Posit.Quarto"
    Add-KnownInstallPaths
}

if (-not $DryRun -and -not (Test-Tool "rig")) {
    throw "rig is not on PATH after installation; restart PowerShell and rerun this script"
}

if (-not $SkipRRelease) {
    Invoke-Step "rig" @("add", "release")
}
if (-not $SkipTestR) {
    Invoke-Step "rig" @("add", $TestRSpec)
}

Set-TestRMetadata

Invoke-Step "cargo" @("--version")
Invoke-Step "rustc" @("--version")
Invoke-Step (Get-PythonTool) @("--version")
Invoke-Step "rig" @("--version")
Invoke-Step "Rscript" @("--version")
if (-not $SkipTestR) {
    if ($DryRun) {
        $testRName = "<rig-name-for-$TestRSpec>"
    }
    else {
        $testRName = if ($TestRName) { $TestRName } else { Get-RigNameForVersion $TestRVersion }
    }
    Invoke-Step "rig" @("run", "-r", $testRName, "-e", "stopifnot(as.character(getRversion()) == '$TestRVersion')")
}
Invoke-Step "quarto" @("--version")

if (-not $SkipTestR -and -not $DryRun -and $env:GITHUB_ENV) {
    Add-Content -Path $env:GITHUB_ENV -Value "IR_TEST_R_VERSION=$TestRVersion"
    Add-Content -Path $env:GITHUB_ENV -Value "IR_TEST_R_EXCLUDE_NEWER=$TestRExcludeNewer"
}

Write-Host ""
Write-Host "Developer dependencies are installed."
if ($SkipTestR) {
    exit 0
}
Write-Host "To enable the version-selection tests in this PowerShell session, run:"
Write-Host ""
Write-Host "  `$env:IR_TEST_R_VERSION=$TestRVersion"
Write-Host "  `$env:IR_TEST_R_EXCLUDE_NEWER=$TestRExcludeNewer"
Write-Host ""
Write-Host "Then run:"
Write-Host ""
Write-Host "  cargo test"
