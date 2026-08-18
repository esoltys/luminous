[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$AppxPath
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -Path $AppxPath)) {
    throw "Package file not found: $AppxPath"
}

if (-not $env:STORE_TENANT_ID -or -not $env:STORE_CLIENT_ID -or -not $env:STORE_CLIENT_SECRET -or -not $env:STORE_APP_ID) {
    throw "STORE_TENANT_ID, STORE_CLIENT_ID, STORE_CLIENT_SECRET and STORE_APP_ID must all be set."
}

Install-PackageProvider -Name NuGet -MinimumVersion 2.8.5.201 -Force | Out-Null
Set-PSRepository -Name "PSGallery" -InstallationPolicy Trusted
Install-Module -Name StoreBroker -Force -Scope CurrentUser

$cred = New-Object System.Management.Automation.PSCredential (
    $env:STORE_CLIENT_ID,
    (ConvertTo-SecureString $env:STORE_CLIENT_SECRET -AsPlainText -Force)
)
Set-StoreBrokerAuthentication -TenantId $env:STORE_TENANT_ID -Credential $cred

$configPath = Join-Path $PSScriptRoot "sbConfig.json"
$outPath = New-Item -Type Directory -Force -Path (Join-Path ([System.IO.Path]::GetTempPath()) "sbSubmission")

Write-Host "Building submission package from $AppxPath ..."
New-SubmissionPackage -ConfigPath $configPath -AppxPath $AppxPath -OutPath $outPath -OutName "submission"

$submissionDataPath = Join-Path $outPath "submission.json"
$submissionPackagePath = Join-Path $outPath "submission.zip"

Write-Host "Replacing packages and committing submission for app $env:STORE_APP_ID ..."
Update-ApplicationSubmission -Verbose -ReplacePackages -AppId $env:STORE_APP_ID -SubmissionDataPath $submissionDataPath -PackagePath $submissionPackagePath -AutoCommit -Force -NoStatus

Write-Host "Submission committed. Certification will proceed in Partner Center."
