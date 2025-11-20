#!/usr/bin/env bash
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

DOCKER_BUILDKIT=1 docker build \
  -f rust/Dockerfile \
  -t hyperlane \
  .

# docker tag hyperlane ghcr.io/zetamarkets/hyperlane:latest