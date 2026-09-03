# tattler — developer targets.
#
# Run `make help` for the list. Recipes use TABs, not spaces.

CARGO    ?= cargo
FEATURES ?= --all-features

.PHONY: help init audit fmt fmt-check clippy clippy-fix check test lint build pre-commit verify

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| sort \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

init: ## installs content to run this project
	$(CARGO) install cargo-audit cargo-hack --locked

fmt: ## Format code in place
	$(CARGO) fmt --all

audit: ## audits dependencies
	$(CARGO) audit

fmt-check: ## Fail if code is not formatted
	$(CARGO) fmt --all -- --check

clippy: ## Lint, denying all warnings
	$(CARGO) clippy $(FEATURES) --all-targets -- -D warnings

clippy-fix: ## Auto-apply clippy suggestions
	$(CARGO) clippy $(FEATURES) --all-targets --fix --allow-dirty --allow-staged

check: ## Type-check every feature target
	$(CARGO) check $(FEATURES) --all-targets

test: ## Run tests
	$(CARGO) test $(FEATURES)

lint: fmt-check clippy ## Static checks only (fast)

build: ## Build in release mode
	$(CARGO) build $(FEATURES) --release

# The fast gate a git pre-commit hook should run — no tests, so it stays quick.
pre-commit: fmt-check clippy check ## Everything CI verifies, minus the test suite

pre-push: audit test ## audit check for deps

# Full local CI parity: the pre-commit gate plus tests.
verify: pre-commit pre-push ## Run the entire CI pipeline locally
