#!/bin/sh
# installed by herdr
# managed by herdr; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# HERDR_INTEGRATION_ID=codex
# HERDR_INTEGRATION_VERSION=8

set -eu

action="${1:-}"
case "$action" in
  session) ;;
  *) exit 0 ;;
esac

[ -n "${HERDR_BIN_PATH:-}" ] || exit 0
exec "$HERDR_BIN_PATH" integration hook codex "$action"
