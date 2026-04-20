# Set the default target of this Makefile
.PHONY: all
all:: ci ## Default target, runs the CI process

.PHONY: check-features
check-features: ## Check feature flags combinations
	cargo hack check --feature-powerset --no-dev-deps

.PHONY: check-fmt
check-fmt: ## Check code formatting
	cargo fmt -- --config imports_granularity=Item --config format_code_in_doc_comments=true --check

.PHONY: fmt
fmt: ## Format code
	cargo fmt -- --config imports_granularity=Item --config format_code_in_doc_comments=true

.PHONY: clippy
clippy: ## Run Clippy linter
	cargo clippy --all-targets --all-features

.PHONY: test
test: ## Run tests
	cargo nextest run --all-features
	cargo test --all-features --doc

# Build against a bare-metal target that has no `std` crate,
# so any accidental reliance on `std` fails to link. `--skip std`
# excludes the `std` feature from the powerset since enabling it
# intentionally opts back into the `std` crate.
.PHONY: check-no-std
check-no-std: ## Verify crate builds in no_std env
	rustup target add aarch64-unknown-none
	cargo hack build --target aarch64-unknown-none --feature-powerset --no-dev-deps --skip std

.PHONY: doc
doc: ## Generate documentation
	RUSTDOCFLAGS="-Dwarnings --cfg=docsrs -Zunstable-options --generate-link-to-definition" RUSTC_BOOTSTRAP=1 cargo doc --all-features --no-deps

.PHONY: doc-open
doc-open: ## Generate and open documentation
	RUSTDOCFLAGS="--cfg=docsrs -Zunstable-options --generate-link-to-definition" RUSTC_BOOTSTRAP=1 cargo doc --all-features --no-deps --open

.PHONY: is-dirty
is-dirty: ## Checks if repository is dirty
	@(test -z "$$(git diff)" || (git diff && false)) && (test -z "$$(git status --porcelain)" || (git status --porcelain && false))

.PHONY: ci
ci: test clippy check-fmt check-features check-no-std ## Run the full CI process

.PHONY: ci-full
ci-full: ci doc ## Run the full CI process and generate documentation

.PHONY: clean
clean: ## Clean build artifacts
	cargo clean

.PHONY: clean-all
clean-all: clean ## Clean all generated files, including those ignored by Git. Force removal.
	git clean -dXf

.PHONY: help
help: ## Show this help
	@echo "Available targets:"
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
