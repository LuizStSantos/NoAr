#!/usr/bin/env bash
set -euo pipefail
source /workspace/noar-cloud/review-20261109/browser/test-env.sh
cd /workspace/NoAr/qa/e2e
npx playwright test --config=/workspace/noar-cloud/review-20261109/browser/playwright-final.config.ts "$@"
