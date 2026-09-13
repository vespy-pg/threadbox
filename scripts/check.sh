#!/usr/bin/env bash
set -euo pipefail

check_memory="${THREADBOX_CHECK_MEMORY:-2g}"
check_cpu_quota="${THREADBOX_CHECK_CPU_QUOTA:-100000}"
check_jobs="${THREADBOX_CHECK_JOBS:-1}"

nice -n 10 docker build \
  --resource "memory=${check_memory}" \
  --resource "cpu-quota=${check_cpu_quota}" \
  --build-arg "CARGO_BUILD_JOBS=${check_jobs}" \
  -f Dockerfile.check \
  -t threadbox-check \
  .
