#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -and $env:PROCESSOR_ARCHITEW6432 -ne 'AMD64') {
  throw 'Only Windows x86_64 is supported.'
}

$SkillDir = Split-Path -Parent $PSScriptRoot
$Version = 'v0.0.0'
$Asset = 'qqbot-cli-windows-x86_64.exe'
$Bin = Join-Path $SkillDir 'bin'
$QQ = Join-Path $Bin 'qqbot-cli.exe'
New-Item -ItemType Directory -Path $Bin -Force | Out-Null
$Tmp = Join-Path $Bin ('.qqbot-cli-download-' + [guid]::NewGuid().ToString('N'))

try {
  try {
    Invoke-WebRequest -UseBasicParsing -ErrorAction Stop `
      -Uri "https://github.com/y0sh1ne/qqbot-cli/releases/download/$Version/$Asset" `
      -OutFile $Tmp
  } catch {
    throw "Failed to download $Asset from release ${Version}. Verify that this release and asset exist. $($_.Exception.Message)"
  }
  if ((Get-Item -LiteralPath $Tmp).Length -eq 0) {
    throw 'Downloaded executable is empty.'
  }
  Move-Item -LiteralPath $Tmp -Destination $QQ -Force
  & $QQ --help
  if ($LASTEXITCODE -ne 0) { throw 'Executable verification failed.' }
} finally {
  if (Test-Path -LiteralPath $Tmp) { Remove-Item -LiteralPath $Tmp -Force }
}
