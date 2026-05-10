#!/usr/bin/env pwsh
# lib-net-retry.ps1 — transient-HTTP retry helper for all fetch scripts.
#
# Dot-source this file, then call Invoke-DownloadWithRetry.
#
# Retry policy (mirrors lib-net-retry.sh — dual-script discipline applies):
#   - 3 total attempts per URL.
#   - Exponential back-off: 1 s before attempt 2, 4 s before attempt 3.
#   - Retries on: WebException, HttpRequestException, IO failures, HTTP 5xx,
#     HTTP 429 (rate-limit).
#   - No retry on: HTTP 4xx except 429 (404 = stale version pin; 403 = auth;
#     silent retry would mask a genuine pin regression).
#   - Integrity verification (SHA256, GPG) is NEVER inside this wrapper — it
#     runs exactly once on the final payload AFTER download succeeds.
#
# Usage:
#   . (Join-Path $PSScriptRoot 'lib-net-retry.ps1')
#   Invoke-DownloadWithRetry -Uri $url -OutFile $dest
#
# This library is paired with lib-net-retry.sh — every change in one MUST
# land in the other in the same PR (see scripts/README.md § Dual-script
# discipline).

# Suppress Invoke-WebRequest's progress bar in CI (it's noise in GHA logs).
$ProgressPreference = 'SilentlyContinue'

# Invoke-DownloadWithRetry -Uri <string> -OutFile <string>
#
# Wraps Invoke-WebRequest with up to 3 attempts and exponential back-off.
# On HTTP 4xx (except 429), gives up immediately — a 4xx from a pinned
# release URL is a permanent error (stale version pin, deleted release, auth
# required) that retrying cannot fix.
# Throws on non-retriable failure or exhausted retries.
function Invoke-DownloadWithRetry {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [string] $Uri,
        [Parameter(Mandatory = $true)] [string] $OutFile
    )

    # Back-off delays (seconds) before attempt 2, then attempt 3.
    $delays    = @(1, 4)
    $maxAttempts = 3
    $lastError = $null

    for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
        try {
            Invoke-WebRequest -Uri $Uri -OutFile $OutFile -UseBasicParsing -ErrorAction Stop
            return  # success
        } catch {
            $lastError = $_

            # Extract HTTP status code from the exception, if available.
            # Works for both PS 5.1 (System.Net.WebException) and PS 7
            # (Microsoft.PowerShell.Commands.HttpResponseException).
            $statusCode = 0
            try {
                if ($null -ne $_.Exception.Response) {
                    $statusCode = [int]$_.Exception.Response.StatusCode
                }
            } catch { }

            # 4xx except 429: configuration error; don't retry.
            if ($statusCode -ge 400 -and $statusCode -lt 500 -and $statusCode -ne 429) {
                $statusMsg = "HTTP $statusCode"
                [Console]::Error.WriteLine("${statusMsg} from ${Uri} (4xx; not retrying — check version pin)")
                [Console]::Error.WriteLine($_.Exception.Message)
                throw
            }

            $statusMsg = if ($statusCode -gt 0) { "HTTP $statusCode" } else { "network error" }

            if ($attempt -lt $maxAttempts) {
                $delay = $delays[$attempt - 1]
                [Console]::Error.WriteLine(
                    "download attempt $attempt/$maxAttempts failed ($statusMsg): " +
                    "$($_.Exception.Message); retrying in ${delay}s…"
                )
                Start-Sleep -Seconds $delay
            } else {
                [Console]::Error.WriteLine(
                    "all $maxAttempts download attempts failed for $Uri ($statusMsg)"
                )
                throw  # re-throw last exception to surface original error
            }
        }
    }
}
