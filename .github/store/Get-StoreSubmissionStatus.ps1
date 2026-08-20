[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

if (-not $env:STORE_TENANT_ID -or -not $env:STORE_CLIENT_ID -or -not $env:STORE_CLIENT_SECRET -or -not $env:STORE_APP_ID) {
    throw "STORE_TENANT_ID, STORE_CLIENT_ID, STORE_CLIENT_SECRET and STORE_APP_ID must all be set."
}

if (-not (Get-Module -ListAvailable -Name StoreBroker)) {
    Set-PSRepository -Name "PSGallery" -InstallationPolicy Trusted
    Install-Module -Name StoreBroker -Force -Scope CurrentUser
}

$cred = New-Object System.Management.Automation.PSCredential (
    $env:STORE_CLIENT_ID,
    (ConvertTo-SecureString $env:STORE_CLIENT_SECRET -AsPlainText -Force)
)
Set-StoreBrokerAuthentication -TenantId $env:STORE_TENANT_ID -Credential $cred

$app = Get-Application -AppId $env:STORE_APP_ID
$submissionId = $app.pendingApplicationSubmission.id
if (-not $submissionId) {
    $submissionId = $app.lastPublishedApplicationSubmission.id
    Write-Host "No pending submission. Reporting last published submission ($submissionId)."
}

Get-ApplicationSubmission -AppId $env:STORE_APP_ID -SubmissionId $submissionId
