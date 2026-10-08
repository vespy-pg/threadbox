#!/usr/bin/env bash
set -euo pipefail

check_memory="${THREADBOX_CHECK_MEMORY:-2g}"
check_cpu_quota="${THREADBOX_CHECK_CPU_QUOTA:-100000}"
check_jobs="${THREADBOX_CHECK_JOBS:-1}"
cache_limit="${THREADBOX_BUILD_CACHE_LIMIT:-5GB}"

cleanup_build_cache() {
  docker builder prune --force --max-used-space "${cache_limit}"
}

trap cleanup_build_cache EXIT

previous_check_image="$(docker image inspect --format '{{.Id}}' threadbox-check:latest 2>/dev/null || true)"

nice -n 10 docker build \
  --resource "memory=${check_memory}" \
  --resource "cpu-quota=${check_cpu_quota}" \
  --build-arg "CARGO_BUILD_JOBS=${check_jobs}" \
  -f Dockerfile.release \
  --target check \
  -t threadbox-check \
  .

current_check_image="$(docker image inspect --format '{{.Id}}' threadbox-check:latest)"

if [[ -n "${previous_check_image}" && "${previous_check_image}" != "${current_check_image}" ]]; then
  previous_image_tags="$(docker image inspect --format '{{join .RepoTags ","}}' "${previous_check_image}" 2>/dev/null || true)"

  if [[ -z "${previous_image_tags}" ]]; then
    docker image rm "${previous_check_image}"
  fi
fi
