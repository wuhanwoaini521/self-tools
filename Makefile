# DevToolbox — development command entrypoint.
# 需要 mise（https://mise.jdx.dev）与 GNU Make；所有目标转发到 mise tasks（定义见 mise.toml）。
#
#   make          show this help (default target)
#   make dev      run the desktop app in dev mode (Tauri + Vite)
#   make dev-web  run the UI in a plain browser (Vite only, no Rust build)
#   make web      run the browser/PWA setup: local API (8080) + UI (1420)
#   make web-lan  same, but also reachable from phones/other devices on the LAN
#   make build    build the UI bundle (tsc --noEmit + vite build)
#   make package  build the release bundle (tauri build)
#   make install  install frontend dependencies
#   make fmt      format Rust code
#   make check    cargo fmt/check/clippy
#   make test     run Rust workspace tests
#   make english download  download NCE1–4 after choosing a local directory
export CXXFLAGS ?= -std=c++17

.PHONY: help dev dev-web web web-lan build package install fmt check test english download

help:
	mise tasks ls
	@echo "make english download  download NCE1–4 (asks for a local directory)"

install:
	mise run install

dev:
	mise run dev

dev-web:
	mise run dev-web

web:
	mise run web

web-lan:
	mise run web-lan

build:
	mise run build

package:
	mise run package

fmt:
	mise run fmt

check:
	mise run check

test:
	mise run test

# `make english download` is a readable alias for the same download workflow.
english: download

download:
	bash scripts/download_english.sh
