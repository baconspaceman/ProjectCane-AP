#requires -Version 5.1
[CmdletBinding()]
param(
    [string]$UpstreamPath = '',
    [switch]$PrepareOnly
)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$pin=Get-Content -LiteralPath (Join-Path $root 'native\upstream.json') -Raw|ConvertFrom-Json
$checkout=Join-Path $root 'native\ProjectCane'
$marker=Join-Path $checkout '.project-kane-ap-base'
foreach($tool in @('git','cmake')) {
    if(-not (Get-Command $tool -ErrorAction SilentlyContinue)){throw "$tool is required. See docs/GETTING_STARTED.md."}
}
function Invoke-Checked([string]$Exe,[string[]]$Arguments) {
    & $Exe @Arguments
    if($LASTEXITCODE -ne 0){throw "$Exe failed with exit code $LASTEXITCODE. Your game data and profiles were not reset."}
}
if(-not (Test-Path -LiteralPath $checkout)) {
    if($UpstreamPath) {
        # Offline verification/developer path. Never uses or edits its working files.
        Invoke-Checked 'git' @('clone','--no-hardlinks','--no-checkout',$UpstreamPath,$checkout)
    } else {
        Invoke-Checked 'git' @('clone','--depth','1','--branch',[string]$pin.tag,[string]$pin.repository,$checkout)
    }
    Invoke-Checked 'git' @('-C',$checkout,'checkout','--detach',[string]$pin.commit)
}
$head=(& git -C $checkout rev-parse HEAD).Trim()
if($head -ne $pin.commit){throw 'Native checkout is not the supported upstream commit. Use a fresh package directory.'}
$patch=Join-Path $root 'native\patches\projectcane-rc1.patch'
$additions=Join-Path $root 'native\additions'
if(-not (Test-Path -LiteralPath $marker)) {
    $dirty=& git -C $checkout status --porcelain
    if($dirty){throw 'Native checkout has existing changes. Preserve it and use a fresh package directory.'}
    Invoke-Checked 'git' @('-C',$checkout,'apply','--check',$patch)
    Invoke-Checked 'git' @('-C',$checkout,'apply',$patch)
    foreach($file in Get-ChildItem -LiteralPath $additions -Recurse -File) {
        $relative=$file.FullName.Substring($additions.Length+1)
        $target=Join-Path $checkout $relative
        New-Item -ItemType Directory -Path (Split-Path $target) -Force|Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $target
    }
    [IO.File]::WriteAllText($marker,[string]$pin.commit)
} elseif((Get-Content -LiteralPath $marker -Raw).Trim() -ne $pin.commit) {
    throw 'Native setup marker does not match this release.'
}
if($PrepareOnly){Write-Output "Prepared pinned native source: $checkout";return}
Push-Location -LiteralPath $checkout
try {
    Invoke-Checked 'cmake' @('--preset','windows-release')
    Invoke-Checked 'cmake' @('--build','--preset','windows-release','--target','ProjectCane','--parallel','1','--','/m:1','/p:UseMultiToolTask=false')
} finally {Pop-Location}
$destination=Join-Path $root 'runtime\bin'
New-Item -ItemType Directory -Path $destination -Force|Out-Null
$game=Join-Path $destination 'ProjectCane.exe'
if(Test-Path -LiteralPath $game) {
    Copy-Item -LiteralPath $game -Destination ($game+'.backup-'+(Get-Date -Format 'yyyyMMdd-HHmmss'))
}
Copy-Item -LiteralPath (Join-Path $checkout 'build\windows-release\bin\Release\ProjectCane.exe') -Destination $game
foreach($dll in @('avcodec-63.dll','avformat-63.dll','avutil-61.dll','swresample-7.dll','swscale-10.dll')) {
    Copy-Item -LiteralPath (Join-Path $checkout ('Libraries\ffmpeg\ffmpeg-master-latest-win64-lgpl-shared\bin\'+$dll)) -Destination $destination
}
Write-Output 'Native AP build installed locally. Next run Setup.cmd with your compatible extracted game data.'
