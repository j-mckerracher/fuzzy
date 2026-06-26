#!/usr/bin/env bash
set -euo pipefail

fuzzy init
fuzzy start --mode investigate "Why did CI time double this month?"
fuzzy librarian ask "CI cache duration recent changes"
fuzzy evidence add --source-type note --confidence medium "Initial evidence needs to be gathered from CI logs"
fuzzy question add --blocking "Which CI job first showed the duration regression?"
fuzzy hypothesis add --confidence 0.35 "Cache key changed and caused dependency reinstall"
fuzzy report
fuzzy gate
