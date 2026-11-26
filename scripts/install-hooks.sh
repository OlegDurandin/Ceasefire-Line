#!/bin/bash
# Points git at the repository's hooks (scripts/hooks): pre-commit and pre-push.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
chmod +x scripts/hooks/*
git config core.hooksPath scripts/hooks
echo "hooks installed: $(git config core.hooksPath)"
