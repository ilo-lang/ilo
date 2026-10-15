# Closed-loop metric gate only. ilo itself builds with cargo.
# See bench/MANIFESTO-METRIC.md. No API key.

.PHONY: help validate-closed-loop

.DEFAULT_GOAL := help

help:
	@echo "validate-closed-loop  honest closed-loop schema gate (no API key)"

validate-closed-loop:
	bash scripts/check-closed-loop-schema.sh
