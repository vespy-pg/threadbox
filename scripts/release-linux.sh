#!/usr/bin/env bash
set -euo pipefail

build_memory="${THREADBOX_BUILD_MEMORY:-2g}"
build_cpu_quota="${THREADBOX_BUILD_CPU_QUOTA:-100000}"
build_jobs="${THREADBOX_BUILD_JOBS:-1}"
cache_limit="${THREADBOX_BUILD_CACHE_LIMIT:-5GB}"

docker build \
  --resource "memory=${build_memory}" \
  --resource "cpu-quota=${build_cpu_quota}" \
  --build-arg "CARGO_BUILD_JOBS=${build_jobs}" \
  -f Dockerfile.release \
  --target artifacts \
  --output type=local,dest=release \
  --progress=plain \
  .

docker builder prune --force --max-used-space "${cache_limit}"
