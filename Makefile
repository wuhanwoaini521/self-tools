# DevToolbox — development command entrypoint.
# 需要 mise（https://mise.jdx.dev）与 GNU Make；所有目标转发到 mise tasks（定义见 mise.toml）。
#
#   make          show this help (default target)
#   make dev      run the desktop app in dev mode (Tauri + Vite)
#   make dev-web  run the UI in a plain browser (Vite only, no Rust build)
#   make build    build the UI bundle (tsc --noEmit + vite build)
#   make package  build the release bundle (tauri build)
#   make install  install frontend dependencies
#   make fmt      format Rust code
#   make check    cargo fmt/check/clippy
#   make test     run Rust workspace tests

.PHONY: help dev dev-web build package install fmt check test

help:
	mise tasks ls

install:
	mise run install

dev:
	mise run dev

dev-web:
	mise run dev-web

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
