#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Observe a hosted producer without changing its command, resources or parallelism.
# Console samples can survive a runner loss that prevents artifact upload.
set -uo pipefail

if [[ $# -lt 4 || $3 != -- ]]; then
  printf 'usage: %s PHASE LOG -- COMMAND [ARG ...]\n' "$0" >&2
  exit 2
fi
phase=$1
log=$2
shift 3

for tool in awk cat date df dirname mkdir ps rm sleep; do
  if ! command -v "$tool" >/dev/null; then
    printf 'observer: required tool unavailable: %s; producer not started\n' "$tool" >&2
    exit 125
  fi
done
if [[ ${OSTYPE:-} != linux* || ! -r /proc/meminfo || ! -r /proc/self/mountinfo ]]; then
  printf 'observer: Linux procfs is required; producer not started\n' >&2
  exit 125
fi
if ! mkdir -p "$(dirname "$log")" || ! exec 3>>"$log"; then
  printf 'observer: cannot append resource log %s; producer not started\n' "$log" >&2
  exit 125
fi

emit() {
  local status=0
  printf '%s\n' "$1" || status=1
  printf '%s\n' "$1" >&3 || status=1
  return "$status"
}

optional_file() {
  local path=$1 content
  if content=$(cat "$path" 2>&1); then
    printf 'file=%s\n%s\n' "$path" "$content"
  else
    printf 'unavailable=%s reason=%s\n' "$path" "$content"
  fi
}

cgroup_snapshot() {
  local relative mount_info mount_root mount_point directory
  optional_file /proc/self/cgroup
  if ! relative=$(awk -F: '$1 == "0" { print $3; exit }' /proc/self/cgroup) || [[ -z $relative ]]; then
    printf 'unavailable=cgroup-v2 reason=no readable unified membership\n'
    return
  fi
  if ! mount_info=$(awk '/ - cgroup2 / { print $4 "\t" $5; exit }' /proc/self/mountinfo) || [[ -z $mount_info ]]; then
    printf 'unavailable=cgroup-v2 reason=no readable unified mount\n'
    return
  fi
  IFS=$'\t' read -r mount_root mount_point <<<"$mount_info"
  if [[ $mount_info == *\\* || $relative == */../* || $relative == */.. ]]; then
    printf 'unavailable=cgroup-v2 reason=membership or escaped mount cannot be resolved\n'
    return
  fi
  if [[ $mount_root == / ]]; then
    directory=${mount_point}${relative}
  elif [[ $relative == "$mount_root" ]]; then
    directory=$mount_point
  elif [[ $relative == "$mount_root/"* ]]; then
    directory=${mount_point}${relative#"$mount_root"}
  else
    printf 'unavailable=cgroup-v2 reason=membership outside visible mount root\n'
    return
  fi
  printf 'cgroup_scope=%s\n' "$directory"
  local field
  for field in memory.current memory.max memory.peak memory.events memory.events.local memory.swap.current memory.swap.max memory.swap.events; do
    optional_file "$directory/$field"
  done
}

snapshot() {
  printf 'resource_snapshot phase=%s timestamp=' "$phase"
  date -u '+%Y-%m-%dT%H:%M:%SZ' || return
  awk '/^(MemTotal|MemAvailable|SwapTotal|SwapFree):/ { print; found++ }
    END { if (found != 4) { print "required memory fields missing" > "/dev/stderr"; exit 1 } }' /proc/meminfo || return
  printf 'process_scope=visible_processes rss_and_vsz_unit=KiB\n'
  ps -e -o pid,ppid,etimes,stat,nlwp,pcpu,rss,vsz,comm --sort=-rss || return
  printf 'filesystem_scope=workspace\n'
  df -Pk . || return
  df -Pi . || return
  if [[ -n ${RUNNER_TEMP:-} ]]; then
    printf 'filesystem_scope=runner_temp\n'
    df -Pk "$RUNNER_TEMP" || return
    df -Pi "$RUNNER_TEMP" || return
  else
    printf 'unavailable=runner_temp reason=RUNNER_TEMP is unset\n'
  fi
  optional_file /proc/pressure/memory
  optional_file /proc/pressure/io
  cgroup_snapshot
}

capture() {
  local packet status
  if packet=$(snapshot 2>&1); then status=0; else status=$?; fi
  emit "$packet" || return 1
  if ((status != 0)); then
    emit "observer_error=required snapshot failed status=$status" || return 1
    return 1
  fi
}

if ! emit "observer_start phase=$phase commit=${GITHUB_SHA:-unavailable} run=${GITHUB_RUN_ID:-unavailable} attempt=${GITHUB_RUN_ATTEMPT:-unavailable} interval_seconds=30" || ! capture; then
  printf 'observer: baseline incomplete; producer not started\n' >&2
  exit 125
fi
ready_file=${log}.sampler-ready-${BASHPID}
if [[ -e $ready_file || -L $ready_file ]]; then
  printf 'observer: sampler acknowledgement path already exists; producer not started\n' >&2
  exit 125
fi

sample_loop() {
  trap - EXIT
  local sleep_pid='' sample_status=0
  stop_sample_loop() {
    local result=$1
    trap - TERM INT EXIT
    if [[ -n $sleep_pid ]]; then
      # This PID belongs only to this sampler. A completed sleep may already be gone.
      if kill -TERM "$sleep_pid" 2>/dev/null; then wait "$sleep_pid" 2>/dev/null; fi
    fi
    exit "$result"
  }
  trap 'stop_sample_loop "$?"' EXIT
  trap 'exit 0' TERM INT
  if ! printf 'ready\n' >"$ready_file"; then
    printf 'observer: cannot acknowledge sampler startup\n' >&2
    stop_sample_loop 1
  fi
  while :; do
    sleep 30 &
    sleep_pid=$!
    if wait "$sleep_pid"; then
      sleep_pid=
    else
      sample_status=$?
      break
    fi
    if capture; then
      continue
    else
      sample_status=$?
      break
    fi
  done
  stop_sample_loop "$sample_status"
}

sampler_pid=
producer_status=not_started
cleanup() {
  local result=$1 sampler_status=0 stop_requested=0
  trap - EXIT TERM INT
  if [[ -n $sampler_pid ]]; then
    # The subsequent wait is authoritative if the sampler exited before this signal.
    if kill -TERM "$sampler_pid" 2>/dev/null; then
      stop_requested=1
    else
      printf 'observer: sampler already exited before cleanup\n' >&2
    fi
    if wait "$sampler_pid"; then sampler_status=0; else sampler_status=$?; fi
    if ((stop_requested == 0 && sampler_status == 0)); then sampler_status=125; fi
  fi
  if ! rm -f -- "$ready_file" && ((sampler_status == 0)); then sampler_status=125; fi
  # A surviving runner can expose events that occurred at the producer's exit.
  if ! capture && ((sampler_status == 0)); then sampler_status=125; fi
  if ((result != 143 && result != 130)) && [[ $producer_status != running && $producer_status != not_started ]] && ((producer_status != 0)); then
    result=$producer_status
  elif ((result == 0 && sampler_status != 0)); then
    printf 'observer: producer succeeded but observation is incomplete\n' >&2
    result=125
  fi
  if ! emit "observer_end phase=$phase producer_exit=$producer_status wrapper_status=$result observation_status=$sampler_status"; then
    printf 'observer: cannot publish final observation\n' >&2
    if ((result == 0)); then result=125; fi
  fi
  exit "$result"
}
trap 'cleanup "$?"' EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
sample_loop &
sampler_pid=$!
# Do not race a fast producer against installation of the sampler's TERM handler.
while [[ ! -f $ready_file ]]; do
  if ! kill -0 "$sampler_pid" 2>/dev/null; then
    printf 'observer: sampler exited before acknowledgement; producer not started\n' >&2
    exit 125
  fi
  if ! sleep 0.01; then exit 125; fi
done
if ! rm -- "$ready_file"; then exit 125; fi
producer_status=running
if "$@" 3>&-; then producer_status=0; else producer_status=$?; fi
cleanup "$producer_status"
