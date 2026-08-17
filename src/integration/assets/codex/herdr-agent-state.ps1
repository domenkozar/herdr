# installed by herdr
# managed by herdr; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# HERDR_INTEGRATION_ID=codex
# HERDR_INTEGRATION_VERSION=8

param([string]$Action = "")

if ($Action -ne "session") { exit 0 }
if ([string]::IsNullOrWhiteSpace($env:HERDR_BIN_PATH)) { exit 0 }
try {
    & $env:HERDR_BIN_PATH integration hook codex $Action 2>$null | Out-Null
} catch {
}
