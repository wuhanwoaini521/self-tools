# DevToolbox — development command entrypoint.
# Requires GNU Make (Windows: `scoop install make` or `choco install make`).
#
#   make          show this help (default target)
#   make dev      run the desktop app in dev mode (Tauri + Vite; installs deps on first run)
#   make dev-web  run the UI in a plain browser (Vite only, no Rust build)
#   make build    build the UI bundle (tsc --noEmit + vite build)
#   make package  build the release bundle (tauri build)
#   make install  install frontend dependencies
#   make test     run Rust workspace tests

.PHONY: help dev dev-web build package install test

help:
	@echo "DevToolbox dev commands:"
	@echo "  make dev        desktop app dev mode (Tauri + Vite, auto npm install)"
	@echo "  make dev-web    UI only in browser (Vite, no Rust)"
	@echo "  make build      build UI bundle (tsc + vite build)"
	@echo "  make package    Tauri release build (bundle installer)"
	@echo "  make install    npm install frontend deps"
	@echo "  make test       cargo test (workspace)"

dev: apps/desktop/ui/node_modules
	npm --prefix apps/desktop/ui exec -- tauri dev

dev-web: apps/desktop/ui/node_modules
	npm --prefix apps/desktop/ui run dev

build: apps/desktop/ui/node_modules
	npm --prefix apps/desktop/ui run build

package: apps/desktop/ui/node_modules
	npm --prefix apps/desktop/ui exec -- tauri build

install:
	npm --prefix apps/desktop/ui install

test:
	cargo test

apps/desktop/ui/node_modules:
	npm --prefix apps/desktop/ui install