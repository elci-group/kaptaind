#!/usr/bin/env bash
set -euo pipefail

ERRORS=0

check_env() {
  local name="$1"
  local required="${2:-true}"
  local value="${!name:-}"
  
  if [ -z "$value" ]; then
    if [ "$required" = "true" ]; then
      echo "❌ MISSING: $name (required)"
      ERRORS=$((ERRORS + 1))
    else
      echo "⚠️  MISSING: $name (optional)"
    fi
  else
    echo "✅ $name is set"
  fi
}

echo "=== Stripe Environment Validation ==="
check_env "STRIPE_SECRET_KEY"
check_env "STRIPE_WEBHOOK_SECRET"
check_env "STRIPE_PRICE_ID_PRO"
check_env "STRIPE_PRICE_ID_TEAM" "false"
check_env "STRIPE_PRICE_ID_ENTERPRISE" "false"

echo ""
if [ "$ERRORS" -gt 0 ]; then
  echo "❌ $ERRORS required Stripe environment variables are missing."
  echo "   Set them in web/.env before starting the billing system."
  exit 1
else
  echo "✅ All required Stripe environment variables are configured."
fi
